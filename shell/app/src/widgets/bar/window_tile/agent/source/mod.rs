mod actual;

#[cfg(test)]
mod test;

use shell_core::source::Observable;

use super::Agent;
use crate::widgets::bar::WindowNode;

pub(in crate::widgets::bar) fn agent_for_window(window: WindowNode) -> Observable<Option<Agent>> {
    actual::agent_for_window(window)
}

pub(in crate::widgets::bar) fn agent_window_ids() -> Observable<Vec<u64>> {
    actual::agent_window_ids()
}
