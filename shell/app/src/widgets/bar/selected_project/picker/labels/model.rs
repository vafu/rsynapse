#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub workspace: Option<String>,
    pub project: String,
    pub generation: u64,
}

pub(super) fn new_label(text: &str, labels: Option<&[String]>, busy: bool) -> Option<String> {
    let labels = labels?;
    let label = text.trim();
    (!busy
        && !label.is_empty()
        && !text.chars().any(char::is_control)
        && !labels.iter().any(|existing| existing == label))
    .then(|| label.to_owned())
}

#[cfg(test)]
mod tests {
    use super::new_label;

    #[test]
    fn normalization_and_add_availability() {
        assert_eq!(
            new_label("  needs review  ", Some(&[]), false).as_deref(),
            Some("needs review")
        );
        assert_eq!(
            new_label("one,two", Some(&[]), false).as_deref(),
            Some("one,two")
        );
        assert_eq!(new_label("a  b", Some(&[]), false).as_deref(), Some("a  b"));
        assert_eq!(new_label(" \t\n", Some(&[]), false), None);
        assert_eq!(new_label("ready", None, false), None);
        assert_eq!(new_label("ready", Some(&[]), true), None);
        assert_eq!(new_label(" ready ", Some(&["ready".into()]), false), None);
        assert_eq!(new_label("ready\n", Some(&[]), false), None);
        assert_eq!(new_label("a\tb", Some(&[]), false), None);
        assert_eq!(new_label("a\u{7f}b", Some(&[]), false), None);
        assert_eq!(
            new_label("Ready", Some(&["ready".into()]), false).as_deref(),
            Some("Ready")
        );
        assert_eq!(
            new_label("\u{2003}équipe\u{2003}", Some(&[]), false).as_deref(),
            Some("équipe")
        );
    }
}
