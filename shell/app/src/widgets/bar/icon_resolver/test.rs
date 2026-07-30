use super::{
    IconCandidate, IconCandidateSource, IconChoice, IconEvidence, IconEvidenceKind, IconPolicy,
    IconRequest, parse_pick_icon_output, picker_cache_key_for_request, picker_input_for_request,
    resolve_icon_for_test,
};
use crate::widgets::nerd_icon::NerdIcon;

#[test]
fn app_alias_resolves_firefox_without_picker() {
    let request = IconRequest::new(
        "window-app-icon",
        IconChoice::from_nerd_icon(NerdIcon::application()),
        IconPolicy::window_app(),
        vec![IconEvidence::new(IconEvidenceKind::AppId, "firefox").unwrap()],
    );

    let resolution = resolve_icon_for_test(&request, Vec::new());

    assert_eq!(resolution.selected.icon, "nf-fa-firefox");
    assert_eq!(resolution.selected.glyph.as_deref(), Some(""));
    assert_eq!(resolution.candidates[0].score_millis, 1000);
    assert_eq!(resolution.candidates[0].source, IconCandidateSource::Alias);
}

#[test]
fn picker_policy_is_chosen_by_request_source() {
    let app_request = IconRequest::new(
        "workspace-icon",
        IconChoice::from_nerd_icon(NerdIcon::workspace()),
        IconPolicy::workspace_apps(),
        vec![IconEvidence::new(IconEvidenceKind::AppId, "unknown-app").unwrap()],
    );
    assert_eq!(picker_cache_key_for_request(&app_request), None);

    let project_request = IconRequest::new(
        "workspace-icon",
        IconChoice::from_nerd_icon(NerdIcon::workspace()),
        IconPolicy::workspace_project(),
        vec![IconEvidence::new(IconEvidenceKind::ProjectName, "rsynapse").unwrap()],
    );
    assert!(picker_cache_key_for_request(&project_request).is_some());
    assert_eq!(picker_input_for_request(&project_request), "rsynapse");
}

#[test]
fn override_wins_but_candidates_remain_available() {
    let request = IconRequest::new(
        "workspace-icon",
        IconChoice::from_nerd_icon(NerdIcon::workspace()),
        IconPolicy::workspace_project(),
        vec![IconEvidence::new(IconEvidenceKind::ProjectName, "rsynapse").unwrap()],
    )
    .with_override(Some(IconChoice::named("custom-icon")));
    let picker_candidate = IconCandidate::new(
        IconChoice::new("nf-dev-rust".to_owned(), Some("".to_owned())).unwrap(),
        740,
        IconCandidateSource::Picker,
    );

    let resolution = resolve_icon_for_test(&request, vec![picker_candidate]);

    assert_eq!(resolution.selected.icon, "custom-icon");
    assert_eq!(resolution.candidates[0].icon, "nf-dev-rust");
    assert!(resolution.overridden);
}

#[test]
fn picker_json_preserves_score_and_thresholds() {
    let candidates = parse_pick_icon_output(
        br#"[{"icon":"communication","glyph":"x","score":1.0},{"icon":"terminal","score":0.721}]"#,
        720,
    );

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| (
                candidate.icon.as_str(),
                candidate.glyph.as_deref(),
                candidate.score_millis,
                candidate.source,
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "communication",
                Some("x"),
                1000,
                IconCandidateSource::Picker
            ),
            ("terminal", None, 721, IconCandidateSource::Picker),
        ]
    );
    assert!(
        parse_pick_icon_output(br#"[{"icon":"phishing","score":0.6931895017623901}]"#, 720)
            .is_empty()
    );
    assert_eq!(
        parse_pick_icon_output(br#"[{"icon":"folder","score":0.67}]"#, 660)[0].score_millis,
        670
    );
}
