//! Small text helpers shared by shell consumers.

/// Trims and returns the value when non-empty.
pub fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// Trims an owned value and returns it when non-empty.
pub fn trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{non_empty, trimmed};

    #[test]
    fn non_empty_trims_and_rejects_blank() {
        assert_eq!(non_empty("  x "), Some("x"));
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty(""), None);
    }

    #[test]
    fn trimmed_returns_owned_non_blank() {
        assert_eq!(trimmed("  x ".to_owned()), Some("x".to_owned()));
        assert_eq!(trimmed("   ".to_owned()), None);
    }
}
