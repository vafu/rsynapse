use data_sources::zvariant::OwnedObjectPath;
use data_sources::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};

use super::{AGENT_DBUS_BUS, AGENT_SESSION_INTERFACE};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AgentSession {
    pub(super) path: OwnedObjectPath,
}

impl AgentSession {
    pub(super) fn agent_name(&self) -> Observable<String> {
        dbus::property_or(self.property("AgentName"), String::new())
    }

    pub(super) fn agent_nickname(&self) -> Observable<String> {
        dbus::property_or(self.property("AgentNickname"), String::new())
    }

    pub(super) fn agent_role(&self) -> Observable<String> {
        dbus::property_or(self.property("AgentRole"), String::new())
    }

    pub(super) fn state(&self) -> Observable<String> {
        dbus::property_or(self.property("State"), String::new())
    }

    pub(super) fn requires_attention(&self) -> Observable<bool> {
        dbus::property_or(self.property("RequiresAttention"), false)
    }

    pub(super) fn cwd(&self) -> Observable<String> {
        dbus::property_or(self.property("Cwd"), String::new())
    }

    pub(super) fn session_title(&self) -> Observable<String> {
        dbus::property_or(self.property("SessionTitle"), String::new())
    }

    pub(super) fn session_id(&self) -> Observable<String> {
        dbus::property_or(self.property("SessionId"), String::new())
    }

    pub(super) fn context_pct(&self) -> Observable<Option<u8>> {
        dbus::property::<f64>(self.property("ContextPct"))
            .map(|value| {
                value
                    .filter(|value| value.is_finite())
                    .map(|value| value.clamp(0.0, 100.0).round() as u8)
            })
            // Optional telemetry must never delay the agent's lifecycle/icon.
            .start_with(vec![None])
            .distinct_until_changed()
            .box_it()
    }

    fn property(&self, name: &'static str) -> PropertyDescriptor {
        let object = ObjectDescriptor::parse(
            Bus::Session,
            AGENT_DBUS_BUS,
            self.path.as_str(),
            AGENT_SESSION_INTERFACE,
        )
        .expect("static AgentDBus descriptor should be valid");
        PropertyDescriptor::new(object, name)
    }
}
