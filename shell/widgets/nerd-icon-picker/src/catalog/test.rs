use super::{NerdIcon, search_icons};

#[test]
fn search_finds_named_nerd_font_icons() {
    let results = search_icons("rust", 12);

    assert!(!results.is_empty());
    assert!(
        results
            .iter()
            .any(|icon| icon.name().contains("rust") && !icon.glyph().is_empty())
    );
}

#[test]
fn search_requires_every_term() {
    let results = search_icons("account arrow", 24);

    assert!(!results.is_empty());
    assert!(results.iter().all(|icon| {
        let name = icon.name();
        name.contains("account") && name.contains("arrow")
    }));
}

#[test]
fn specific_icons_reject_empty_values() {
    assert!(NerdIcon::specific("Suggested", "").is_none());
    assert!(NerdIcon::specific("", "x").is_none());
    assert_eq!(NerdIcon::specific("Suggested", "x").unwrap().glyph(), "x");
}
