use super::*;

fn locator() -> TaskLocator {
    TaskLocator {
        note_id: NoteId::new("tasks/root.md"),
        note_revision: NoteRevision::new("rev-1"),
        line: 7,
    }
}

#[test]
fn task_refs_round_trip() {
    let encoded = encode_ref(&locator());
    assert_eq!(decode_ref(&encoded).unwrap(), locator());
}

#[test]
fn malformed_refs_are_rejected() {
    assert!(decode_ref("nope").is_err());
    assert!(decode_ref("tr1.a.b.0").is_err());
}
