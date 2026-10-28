mod navigation;
use super::coverage::classify;
use crate::app::{
    domain::{CursorRange, TextDocument},
    services::session_store::BufferVersionInfo,
};

fn document() -> TextDocument {
    let mut document = TextDocument::new("zero".into());
    for (end, text) in [(4, "one"), (3, "two")] {
        document
            .replace_char_ranges_with_undo(
                &[(0..end, text.into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
    }
    document
}

fn point(id: i64, text: &str) -> BufferVersionInfo {
    BufferVersionInfo {
        id,
        created_at_unix_seconds: id,
        bytes: text.len(),
        undo_boundary: false,
        text_digest: blake3::hash(text.as_bytes()).as_bytes().to_vec(),
        anchor: None,
        has_history: false,
        history_bytes: 0,
    }
}

#[test]
fn snapshots_extend_history_only_when_the_state_is_not_reachable() {
    let mut document = document();
    let points = vec![
        point(1, "ancient"),
        point(2, "zero"),
        point(3, "one"),
        point(4, "two"),
        point(5, "two"),
    ];
    let rows = classify(&document, points.clone());
    assert_eq!(rows.len(), 4, "identical text is not listed twice");
    assert_eq!(
        rows.iter()
            .filter(|row| row.covered.is_none())
            .map(|row| row.point.id)
            .collect::<Vec<_>>(),
        [1]
    );
    document.drop_oldest_history_entry().unwrap();
    let rows = classify(&document, points);
    assert_eq!(
        rows.iter()
            .filter(|row| row.covered.is_none())
            .map(|row| row.point.id)
            .collect::<Vec<_>>(),
        [2, 1]
    );
}

#[test]
fn identical_text_on_a_different_branch_is_still_covered() {
    let document = document();
    let mut branch = point(2, "one");
    branch.anchor = Some((u64::MAX, true));
    branch.has_history = true;
    let rows = classify(&document, vec![point(1, "one"), branch.clone()]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].point.id, branch.id);
    assert!(rows[0].covered.is_some());
    assert!(classify(&document, vec![branch])[0].covered.is_some());
}

#[test]
fn coverage_checks_redo_and_rejects_same_size_or_wrong_anchor_content() {
    let mut document = document();
    let first = document.history_entries()[0].id;
    document.apply_text_history_undo(first).unwrap();
    let mut wrong = point(3, "bad");
    wrong.anchor = Some((document.history_entries()[1].global_seq, false));
    let rows = classify(&document, vec![point(1, "one"), point(2, "two"), wrong]);
    assert_eq!(
        rows.iter()
            .filter(|row| row.covered.is_none())
            .map(|row| row.point.id)
            .collect::<Vec<_>>(),
        [3]
    );
    let mut history = document.exported_history();
    history[0].drop_payloads();
    document.restore_exported_history(history);
    assert!(
        classify(&document, vec![point(1, "one")])[0]
            .covered
            .is_none()
    );
}
