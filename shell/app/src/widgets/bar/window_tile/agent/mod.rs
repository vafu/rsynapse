mod source;

pub(in crate::widgets::bar) use source::agent_for_window;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct Agent {
    pub(in crate::widgets::bar) icon: String,
    pub(in crate::widgets::bar) attention: bool,
    pub(in crate::widgets::bar) state: State,
    pub(in crate::widgets::bar) context_pct: u32,
    pub(in crate::widgets::bar) unseen: bool,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) enum State {
    #[default]
    None,
    Idle,
    Thinking,
    ToolUse,
    Compacting,
}
