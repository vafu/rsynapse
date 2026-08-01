use std::collections::HashMap;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as DeError, MapAccess, Visitor},
    ser::SerializeMap,
};
use zbus::proxy;
use zvariant::Type;

pub const BUS_NAME: &str = "org.rsynapse.Locus";
pub const OBJECT_PATH: &str = "/org/rsynapse/Locus";
pub const RELATIONS_INTERFACE: &str = "org.rsynapse.Locus.Relations1";

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Type)]
#[zvariant(signature = "a{ss}")]
pub enum RelationEndpoint {
    StableKey {
        kind: String,
        id: String,
    },
    DBusObject {
        bus: String,
        service: String,
        path: String,
        interface: String,
    },
}

impl RelationEndpoint {
    pub fn stable_key(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self::StableKey {
            kind: kind.into(),
            id: id.into(),
        }
    }

    pub fn dbus_object(
        bus: impl Into<String>,
        service: impl Into<String>,
        path: impl Into<String>,
        interface: impl Into<String>,
    ) -> Self {
        Self::DBusObject {
            bus: bus.into(),
            service: service.into(),
            path: path.into(),
            interface: interface.into(),
        }
    }
}

impl Serialize for RelationEndpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        match self {
            Self::StableKey { kind, id } => {
                map.serialize_entry("type", "stable-key")?;
                map.serialize_entry("kind", kind)?;
                map.serialize_entry("id", id)?;
            }
            Self::DBusObject {
                bus,
                service,
                path,
                interface,
            } => {
                map.serialize_entry("type", "dbus-object")?;
                map.serialize_entry("bus", bus)?;
                map.serialize_entry("service", service)?;
                map.serialize_entry("path", path)?;
                map.serialize_entry("interface", interface)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for RelationEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RelationEndpointVisitor)
    }
}

struct RelationEndpointVisitor;

impl<'de> Visitor<'de> for RelationEndpointVisitor {
    type Value = RelationEndpoint;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a relation endpoint dictionary or legacy endpoint string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        legacy_endpoint(value).ok_or_else(|| {
            E::custom(format!(
                "unsupported legacy relation endpoint string {value:?}"
            ))
        })
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(&value)
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut fields = HashMap::new();
        while let Some((key, value)) = access.next_entry::<String, String>()? {
            fields.insert(key, value);
        }
        let endpoint_type = take_required(&mut fields, "type")?;
        match endpoint_type.as_str() {
            "stable-key" => Ok(RelationEndpoint::StableKey {
                kind: take_required(&mut fields, "kind")?,
                id: take_required(&mut fields, "id")?,
            }),
            "dbus-object" => Ok(RelationEndpoint::DBusObject {
                bus: take_required(&mut fields, "bus")?,
                service: take_required(&mut fields, "service")?,
                path: take_required(&mut fields, "path")?,
                interface: take_required(&mut fields, "interface")?,
            }),
            value => Err(A::Error::custom(format!(
                "unknown relation endpoint type {value:?}"
            ))),
        }
    }
}

fn legacy_endpoint(value: &str) -> Option<RelationEndpoint> {
    let (prefix, id) = value.split_once(':')?;
    if id.is_empty() {
        return None;
    }

    let kind = match prefix {
        "agent-session" => keys::AGENT_SESSION_ID,
        "app-instance" => keys::APP_INSTANCE_ID,
        "bazel-invocation" | "build-invocation" => keys::BAZEL_INVOCATION_ID,
        "niri-window" => keys::NIRI_WINDOW_ID,
        "niri-workspace" => keys::NIRI_WORKSPACE_ID,
        "project" => keys::PROJECT_PATH,
        _ => return None,
    };

    Some(RelationEndpoint::stable_key(kind, id))
}

fn take_required<E>(fields: &mut HashMap<String, String>, key: &str) -> Result<String, E>
where
    E: DeError,
{
    fields
        .remove(key)
        .ok_or_else(|| E::custom(format!("missing relation endpoint field {key:?}")))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub struct RelationRecord {
    pub subject: RelationEndpoint,
    pub relation: String,
    pub target: RelationEndpoint,
    pub metadata: HashMap<String, String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[proxy(
    interface = "org.rsynapse.Locus.Relations1",
    default_service = "org.rsynapse.Locus",
    default_path = "/org/rsynapse/Locus"
)]
pub trait Relations {
    #[zbus(property)]
    fn record_count(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn relations(&self) -> zbus::Result<Vec<String>>;

    async fn set(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
    ) -> zbus::Result<RelationRecord>;

    async fn set_one(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
    ) -> zbus::Result<RelationRecord>;

    async fn unset(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
    ) -> zbus::Result<bool>;

    async fn clear(&self, subject: RelationEndpoint, relation: &str) -> zbus::Result<u32>;

    async fn targets(
        &self,
        subject: RelationEndpoint,
        relation: &str,
    ) -> zbus::Result<Vec<RelationEndpoint>>;

    async fn subjects(
        &self,
        relation: &str,
        target: RelationEndpoint,
    ) -> zbus::Result<Vec<RelationEndpoint>>;

    async fn list(&self, relation: &str) -> zbus::Result<Vec<RelationRecord>>;
}

pub mod keys {
    pub const APP_INSTANCE_ID: &str = "org.rsynapse.app-instance.id";
    pub const BAZEL_INVOCATION_ID: &str = "org.rsynapse.bazel.invocation.id";
    pub const NIRI_OUTPUT_NAME: &str = "org.rsynapse.niri.output.name";
    pub const NIRI_WORKSPACE_ID: &str = "org.rsynapse.niri.workspace.id";
    pub const NIRI_WORKSPACE_NAME: &str = "org.rsynapse.niri.workspace.name";
    pub const NIRI_WINDOW_ID: &str = "org.rsynapse.niri.window.id";
    pub const PROJECT_PATH: &str = "org.rsynapse.project.path";
    pub const AGENT_SESSION_ID: &str = "org.rsynapse.agent.session.id";
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zvariant::{LE, Type, serialized::Context, to_bytes};

    use super::{RelationEndpoint, RelationRecord};

    #[test]
    fn endpoint_uses_dictionary_signature() {
        assert_eq!(RelationEndpoint::signature(), "a{ss}");
    }

    #[test]
    fn endpoint_roundtrips_over_zvariant() {
        let endpoint = RelationEndpoint::stable_key("org.rsynapse.niri.workspace.id", "5");
        let bytes = to_bytes(Context::new_dbus(LE, 0), &endpoint).expect("serialize endpoint");
        let decoded: RelationEndpoint = bytes.deserialize().expect("deserialize endpoint").0;
        assert_eq!(decoded, endpoint);
    }

    #[test]
    fn endpoint_reads_legacy_strings() {
        let endpoint: RelationEndpoint =
            serde_json::from_str(r#""niri-workspace:10""#).expect("deserialize workspace");
        assert_eq!(
            endpoint,
            RelationEndpoint::stable_key("org.rsynapse.niri.workspace.id", "10")
        );

        let endpoint: RelationEndpoint =
            serde_json::from_str(r#""project:/home/v47/proj/rsynapse""#)
                .expect("deserialize project");
        assert_eq!(
            endpoint,
            RelationEndpoint::stable_key("org.rsynapse.project.path", "/home/v47/proj/rsynapse")
        );

        let endpoint: RelationEndpoint = serde_json::from_str(r#""agent-session:codex/session""#)
            .expect("deserialize agent session");
        assert_eq!(
            endpoint,
            RelationEndpoint::stable_key("org.rsynapse.agent.session.id", "codex/session")
        );

        let endpoint: RelationEndpoint = serde_json::from_str(r#""build-invocation:inv-1""#)
            .expect("deserialize build invocation");
        assert_eq!(
            endpoint,
            RelationEndpoint::stable_key("org.rsynapse.bazel.invocation.id", "inv-1")
        );
    }

    #[test]
    fn record_roundtrips_over_zvariant() {
        let record = RelationRecord {
            subject: RelationEndpoint::stable_key("subject.kind", "subject-id"),
            relation: "org.rsynapse.test".to_owned(),
            target: RelationEndpoint::dbus_object(
                "session",
                "org.example.Service",
                "/org/example/Object",
                "org.example.Interface",
            ),
            metadata: HashMap::from([("name".to_owned(), "value".to_owned())]),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };
        let bytes = to_bytes(Context::new_dbus(LE, 0), &record).expect("serialize record");
        let decoded: RelationRecord = bytes.deserialize().expect("deserialize record").0;
        assert_eq!(decoded, record);
    }

    #[test]
    fn record_reads_legacy_endpoint_strings() {
        let record: RelationRecord = serde_json::from_str(
            r#"{
                "subject": "niri-window:80",
                "relation": "org.rsynapse.window.agent-session",
                "target": "agent-session:codex/session",
                "metadata": {},
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 2
            }"#,
        )
        .expect("deserialize legacy record");

        assert_eq!(
            record.subject,
            RelationEndpoint::stable_key("org.rsynapse.niri.window.id", "80")
        );
        assert_eq!(
            record.target,
            RelationEndpoint::stable_key("org.rsynapse.agent.session.id", "codex/session")
        );
    }
}
