#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar::project_label) struct WorkspaceIconChoice {
    pub(in crate::widgets::bar::project_label) icon: String,
    pub(in crate::widgets::bar::project_label) glyph: Option<String>,
}

impl WorkspaceIconChoice {
    pub(super) fn new(icon: String, glyph: Option<String>) -> Option<Self> {
        let icon = non_empty(icon)?;
        Some(Self {
            icon,
            glyph: glyph.and_then(non_empty),
        })
    }

    pub(super) fn material(icon: &str) -> Self {
        Self {
            icon: icon.to_owned(),
            glyph: None,
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
