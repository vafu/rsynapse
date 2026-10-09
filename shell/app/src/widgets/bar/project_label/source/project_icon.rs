use super::workspace_icon::{WorkspaceIcon, WorkspaceIconChoice};
use crate::widgets::bar::{
    icon_resolver::{IconEvidence, IconEvidenceKind, IconPolicy, IconRequest, resolve_icon_once},
    project::ProjectDetails,
};
use shell_core::source::{self, Observable, rx::Observable as _};
#[cfg(test)]
mod test;
pub(super) fn icon(project: ProjectDetails, id: String) -> Observable<WorkspaceIcon> {
    let input = project.name.clone().unwrap_or_default();
    let fallback = crate::widgets::nerd_icon::NerdIcon::folder()
        .glyph()
        .to_owned();
    let request = IconRequest::new(
        "project-icon",
        WorkspaceIconChoice::new(fallback.clone()).unwrap(),
        IconPolicy::workspace_project(),
        project
            .name
            .and_then(|n| IconEvidence::new(IconEvidenceKind::ProjectName, n))
            .into_iter()
            .collect(),
    );
    source::switch_map(
        source::proj::project_icon(id.clone())
            .combine_latest(
                source::proj::project_icon_origin(id.clone()),
                |glyph, origin| (glyph, origin),
            )
            .box_it(),
        move |(glyph, origin): (String, String)| {
            if !glyph.is_empty() {
                source::once(WorkspaceIcon {
                    glyph,
                    empty: false,
                    picker_input: input.clone(),
                    candidates: Vec::new(),
                    overridden: origin == "manual",
                })
            } else {
                let id = id.clone();
                let request = request.clone();
                let input = input.clone();
                let fallback = fallback.clone();
                source::from_task(move |sender| {
                    let id = id.clone();
                    let request = request.clone();
                    let input = input.clone();
                    let fallback = fallback.clone();
                    async move {
                        let _ = sender
                            .send(Ok(WorkspaceIcon {
                                glyph: fallback,
                                empty: false,
                                picker_input: input,
                                candidates: Vec::new(),
                                overridden: false,
                            }))
                            .await;
                        let resolution = resolve_icon_once(request).await;
                        let result = async {
                            let conn = zbus::Connection::session().await?;
                            let proxy = zbus::Proxy::new(
                                &conn,
                                "org.rsynapse.Proj",
                                "/org/rsynapse/Proj",
                                "org.rsynapse.Proj.Manager1",
                            )
                            .await?;
                            let _: source::proj::ProjectInfo = proxy
                                .call("SetProjectIconIfUnset", &(id, resolution.selected.glyph))
                                .await?;
                            Ok::<_, zbus::Error>(())
                        }
                        .await;
                        if let Err(error) = result {
                            let _ = sender.send(Err(error.to_string())).await;
                        }
                    }
                })
            }
        },
    )
    .distinct_until_changed()
    .box_it()
}
