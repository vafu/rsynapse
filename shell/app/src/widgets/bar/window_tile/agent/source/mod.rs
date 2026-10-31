mod actual;
mod properties;

const AGENT_DBUS_BUS: &str = "io.github.AgentDBus";
const AGENT_DBUS_ROOT_PATH: &str = "/io/github/AgentDBus";
const AGENT_SESSION_INTERFACE: &str = "io.github.AgentDBus1.Session";

#[cfg(test)]
mod test;

use shell_core::source::Observable;

use super::Agent;
use crate::widgets::bar::WindowNode;

pub(in crate::widgets::bar) fn agent_for_window(window: WindowNode) -> Observable<Option<Agent>> {
    actual::agent_for_window(window)
}
