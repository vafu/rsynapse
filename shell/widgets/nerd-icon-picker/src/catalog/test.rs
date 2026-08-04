use super::{NerdIcon, parse_pick_icon_output, pick_icon_arguments};

#[test]
fn pick_icon_query_requests_ranked_nerd_results() {
    let arguments = pick_icon_arguments("rstu", 12)
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        &arguments[1..],
        [
            "--family", "nerd", "--string", "rstu", "--top", "12", "--json"
        ]
    );
}

#[test]
fn pick_icon_json_preserves_fuzzy_ranking() {
    let results = parse_pick_icon_output(
        br#"[
            {"glyph":"r","icon":"nf-dev-rstudio","score":0.72},
            {"glyph":"R","icon":"nf-dev-r","score":0.68}
        ]"#,
        12,
    );

    assert_eq!(results[0].name(), "nf-dev-rstudio");
    assert_eq!(results[0].glyph(), "r");
    assert_eq!(results[1].name(), "nf-dev-r");
}

#[test]
fn pick_icon_json_rejects_empty_results_and_applies_limit() {
    let results = parse_pick_icon_output(
        br#"[
            {"glyph":"","icon":"nf-dev-empty","score":1.0},
            {"glyph":"a","icon":"nf-dev-a","score":0.9},
            {"glyph":"b","icon":"nf-dev-b","score":0.8}
        ]"#,
        1,
    );

    assert_eq!(results, vec![NerdIcon::specific("nf-dev-a", "a").unwrap()]);
}

#[test]
fn specific_icons_reject_empty_values() {
    assert!(NerdIcon::specific("Suggested", "").is_none());
    assert!(NerdIcon::specific("", "x").is_none());
    assert_eq!(NerdIcon::specific("Suggested", "x").unwrap().glyph(), "x");
}
