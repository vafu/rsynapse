#[derive(Debug)]
pub(in crate::widgets::bar) enum ProjectLabelInput {
    Source(super::project_label_sources::Msg),
    SetIconOverride(String),
    ClearIconOverride,
}

impl From<super::project_label_sources::Msg> for ProjectLabelInput {
    fn from(msg: super::project_label_sources::Msg) -> Self {
        Self::Source(msg)
    }
}
