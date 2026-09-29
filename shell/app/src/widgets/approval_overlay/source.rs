use shell_core::source::{
    self, Observable,
    dbus::{self, Bus, DbusObject, ObjectDescriptor, ObjectManagerDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use shell_rx_macros::combine_latest;
use zbus::zvariant::OwnedObjectPath;

#[cfg(test)]
mod test;

const AGENT_DBUS_BUS: &str = "io.github.AgentDBus";
const AGENT_DBUS_ROOT_PATH: &str = "/io/github/AgentDBus";
const AGENT_SESSION_INTERFACE: &str = "io.github.AgentDBus1.Session";

const NIRI_BUS: &str = "org.rsynapse.Niri";
const NIRI_ROOT_PATH: &str = "/org/rsynapse/Niri";
const NIRI_ROOT_INTERFACE: &str = "org.rsynapse.Niri1";
const NIRI_WORKSPACE_INTERFACE: &str = "org.rsynapse.Niri1.Workspace";
const NIRI_OUTPUT_INTERFACE: &str = "org.rsynapse.Niri1.Output";

/// One answerable approval request, mirroring the AGS `PendingApproval` shape.
///
/// A session can hold several pending requests; the parallel
/// `Pending*` arrays on the session object stay aligned by index, with the
/// legacy singular properties as fallback when no request ids are present.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingApproval {
    pub(super) session_path: String,
    pub(super) session_id: String,
    pub(super) request_id: String,
    pub(super) agent_name: String,
    pub(super) model_name: String,
    pub(super) cwd: String,
    pub(super) session_title: String,
    pub(super) prompt: String,
    pub(super) detail_kind: String,
    pub(super) detail_text: String,
    pub(super) options: Vec<String>,
    pub(super) option_descriptions: Vec<String>,
}

pub(super) fn pending_approvals() -> Observable<Vec<PendingApproval>> {
    source::shared_by_key("rsynapse.approval-overlay", "pending", || {
        source::switch_map_list_distinct(session_paths(), session_approvals)
            .map(|groups| groups.into_iter().flatten().collect())
            .distinct_until_changed()
            .box_it()
    })
}

/// Answers a pending request through the session object.
///
/// Uses `RespondToElicitationById` when the request id is known, otherwise
/// the oldest-pending `RespondToElicitation`, matching the AGS overlay.
pub(super) fn respond_to_elicitation(session_path: String, request_id: String, answer: String) {
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                runtime.block_on(async move { respond(session_path, request_id, answer).await })
            });
        if let Err(error) = result {
            eprintln!("[approval-overlay] respond failed: {error}");
        }
    });
}

async fn respond(session_path: String, request_id: String, answer: String) -> Result<(), String> {
    let path = zbus::zvariant::OwnedObjectPath::try_from(session_path)
        .map_err(|error| error.to_string())?;
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| error.to_string())?;
    let proxy = zbus::Proxy::new(&connection, AGENT_DBUS_BUS, path, AGENT_SESSION_INTERFACE)
        .await
        .map_err(|error| error.to_string())?;
    if request_id.is_empty() {
        proxy
            .call_method("RespondToElicitation", &answer)
            .await
            .map_err(|error| error.to_string())?;
    } else {
        proxy
            .call_method("RespondToElicitationById", &(request_id, answer))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Connector name (e.g. `DP-1`) of the currently focused output, if known.
///
/// The approval overlay window follows this so approvals pop up on the
/// screen the user is looking at, mirroring the AGS active-monitor window.
/// The niri descriptors are intentionally local: `bar::niri` is scoped to
/// bar widgets, while overlay placement is approval-overlay policy.
pub(crate) fn focused_output_name() -> Observable<Option<String>> {
    source::shared_by_key("rsynapse.approval-overlay", "focused-output", || {
        source::switch_map(focused_workspace_path(), |workspace| match workspace {
            Some(path) => source::switch_map(workspace_output_path(&path), output_name),
            None => source::once(None),
        })
        .distinct_until_changed()
        .box_it()
    })
}

fn focused_workspace_path() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(PropertyDescriptor::new(
        niri_object(NIRI_ROOT_PATH, NIRI_ROOT_INTERFACE),
        "FocusedWorkspace",
    ))
}

fn workspace_output_path(workspace: &OwnedObjectPath) -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(PropertyDescriptor::new(
        niri_object(workspace.as_str(), NIRI_WORKSPACE_INTERFACE),
        "Output",
    ))
}

fn output_name(output: Option<OwnedObjectPath>) -> Observable<Option<String>> {
    let Some(path) = output else {
        return source::once(None);
    };
    dbus::property_or(
        PropertyDescriptor::new(niri_object(path.as_str(), NIRI_OUTPUT_INTERFACE), "Name"),
        String::new(),
    )
    .map(|name| (!name.is_empty()).then_some(name))
    .distinct_until_changed()
    .box_it()
}

fn niri_object(path: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, NIRI_BUS, path, interface)
        .expect("niri descriptor should be valid")
}

/// Deduplication key for auto-open: one overlay popup per distinct request.
pub(super) fn approval_key(approval: &PendingApproval) -> String {
    format!(
        "{}:{}:{}",
        approval.session_id, approval.request_id, approval.prompt
    )
}

/// Short display path for card headers (`~/…`, like the AGS overlay).
pub(super) fn pretty_path(path: &str) -> String {
    if path.is_empty() {
        return "unknown cwd".to_owned();
    }
    let mut parts = path.splitn(4, '/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(""), Some("home"), Some(_), Some(rest)) => format!("~/{rest}"),
        _ => path.to_owned(),
    }
}

/// AgentDBus session object paths, from the ObjectManager snapshot.
///
/// The snapshot only tracks membership: sessions live for the whole agent
/// turn while their pending-request state changes via `PropertiesChanged`,
/// which the snapshot stream ignores. It is used for discovery only; every
/// volatile field below is subscribed per property so updates arrive live.
fn session_paths() -> Observable<Vec<String>> {
    dbus::object_manager(agent_dbus())
        .map(|objects| {
            objects
                .iter()
                .filter(|object| has_session_interface(object))
                .map(|object| object.path.as_str().to_owned())
                .collect()
        })
        .distinct_until_changed()
        .box_it()
}

fn session_approvals(path: String) -> Observable<Vec<PendingApproval>> {
    combine_latest!(
        session_meta(&path),
        pending_arrays(&path),
        legacy_pending(&path)
        => move |(meta, arrays, legacy)| approvals_for_session(&path, meta, arrays, legacy),
    )
    .distinct_until_changed()
    .box_it()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SessionMeta {
    session_id: String,
    agent_name: String,
    model_name: String,
    cwd: String,
    session_title: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PendingArrays {
    requires_attention: bool,
    request_ids: Vec<String>,
    prompts: Vec<String>,
    detail_kinds: Vec<String>,
    detail_texts: Vec<String>,
    options_list: Vec<Vec<String>>,
    descriptions_list: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LegacyPending {
    prompt: String,
    detail_kind: String,
    detail_text: String,
    options: Vec<String>,
    descriptions: Vec<String>,
}

fn session_meta(path: &str) -> Observable<SessionMeta> {
    combine_latest!(
        session_property(path, "SessionId", String::new()),
        session_property(path, "AgentName", String::new()),
        session_property(path, "ModelName", String::new()),
        session_property(path, "Cwd", String::new()),
        session_property(path, "SessionTitle", String::new())
        => |(session_id, agent_name, model_name, cwd, session_title)| SessionMeta {
            session_id,
            agent_name,
            model_name,
            cwd,
            session_title,
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn pending_arrays(path: &str) -> Observable<PendingArrays> {
    combine_latest!(
        session_property(path, "RequiresAttention", false),
        session_property(path, "PendingRequestIds", Vec::new()),
        session_property(path, "PendingPrompts", Vec::new()),
        session_property(path, "PendingDetailKinds", Vec::new()),
        session_property(path, "PendingDetailTexts", Vec::new()),
        session_property(path, "PendingOptionsList", Vec::new()),
        session_property(path, "PendingOptionDescriptionsList", Vec::new())
        => |(
            requires_attention,
            request_ids,
            prompts,
            detail_kinds,
            detail_texts,
            options_list,
            descriptions_list,
        )| PendingArrays {
            requires_attention,
            request_ids,
            prompts,
            detail_kinds,
            detail_texts,
            options_list,
            descriptions_list,
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn legacy_pending(path: &str) -> Observable<LegacyPending> {
    combine_latest!(
        session_property(path, "PendingPrompt", String::new()),
        session_property(path, "PendingDetailKind", String::new()),
        session_property(path, "PendingDetailText", String::new()),
        session_property(path, "PendingOptions", Vec::new()),
        session_property(path, "PendingOptionDescriptions", Vec::new())
        => |(prompt, detail_kind, detail_text, options, descriptions)| LegacyPending {
            prompt,
            detail_kind,
            detail_text,
            options,
            descriptions,
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn approvals_for_session(
    path: &str,
    meta: SessionMeta,
    arrays: PendingArrays,
    legacy: LegacyPending,
) -> Vec<PendingApproval> {
    if !arrays.requires_attention {
        return Vec::new();
    }

    if !arrays.request_ids.is_empty() {
        return arrays
            .request_ids
            .into_iter()
            .enumerate()
            .filter_map(|(index, request_id)| {
                let prompt = arrays
                    .prompts
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| legacy.prompt.clone());
                if prompt.is_empty() {
                    return None;
                }
                Some(PendingApproval {
                    session_path: path.to_owned(),
                    session_id: meta.session_id.clone(),
                    request_id,
                    agent_name: meta.agent_name.clone(),
                    model_name: meta.model_name.clone(),
                    cwd: meta.cwd.clone(),
                    session_title: meta.session_title.clone(),
                    prompt,
                    detail_kind: arrays
                        .detail_kinds
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| legacy.detail_kind.clone()),
                    detail_text: arrays
                        .detail_texts
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| legacy.detail_text.clone()),
                    options: with_default_options(
                        arrays
                            .options_list
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| legacy.options.clone()),
                    ),
                    option_descriptions: arrays
                        .descriptions_list
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| legacy.descriptions.clone()),
                })
            })
            .collect();
    }

    if legacy.prompt.is_empty() {
        return Vec::new();
    }
    vec![PendingApproval {
        session_path: path.to_owned(),
        session_id: meta.session_id,
        request_id: String::new(),
        agent_name: meta.agent_name,
        model_name: meta.model_name,
        cwd: meta.cwd,
        session_title: meta.session_title,
        prompt: legacy.prompt,
        detail_kind: legacy.detail_kind,
        detail_text: legacy.detail_text,
        options: with_default_options(legacy.options),
        option_descriptions: legacy.descriptions,
    }]
}

fn with_default_options(options: Vec<String>) -> Vec<String> {
    if options.is_empty() {
        vec!["Allow".to_owned(), "Deny".to_owned()]
    } else {
        options
    }
}

fn agent_dbus() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::Session, AGENT_DBUS_BUS, AGENT_DBUS_ROOT_PATH)
        .expect("AgentDBus descriptor should be valid")
}

fn has_session_interface(object: &DbusObject) -> bool {
    object
        .interfaces
        .iter()
        .any(|interface| interface.name.as_str() == AGENT_SESSION_INTERFACE)
}

fn session_property<T>(path: &str, name: &'static str, default: T) -> Observable<T>
where
    T: TryFrom<zbus::zvariant::OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or(
        PropertyDescriptor::new(agent_session_object(path), name),
        default,
    )
}

fn agent_session_object(path: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, AGENT_DBUS_BUS, path, AGENT_SESSION_INTERFACE)
        .expect("AgentDBus session descriptor should be valid")
}
