use crate::app::{domain::TextDocument, services::session_store::BufferVersionInfo};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Boundary {
    pub entry_id: Option<u64>,
    pub before: bool,
}

#[derive(Clone)]
pub(super) struct SnapshotRow {
    pub point: BufferVersionInfo,
    pub covered: Option<Boundary>,
}

#[cfg(test)]
pub(super) fn classify(
    document: &TextDocument,
    versions: Vec<BufferVersionInfo>,
) -> Vec<SnapshotRow> {
    classify_cancellable(document, versions, &AtomicBool::new(false))
}

/// Replay proves reachability; exact decoded-text fingerprints prove equality.
/// Even a snapshot from another branch may contain a reachable text state, so
/// sequence anchors and timestamps must never exclude a possible content match.
pub(super) fn classify_cancellable(
    document: &TextDocument,
    versions: Vec<BufferVersionInfo>,
    cancelled: &AtomicBool,
) -> Vec<SnapshotRow> {
    let mut rows: Vec<_> = versions
        .into_iter()
        .map(|point| SnapshotRow {
            point,
            covered: None,
        })
        .collect();
    if rows.is_empty() {
        return rows;
    }
    let entries = document.history_entries();
    let current = entries
        .iter()
        .rev()
        .find(|entry| !entry.is_undone())
        .map(|entry| (entry, false))
        .or_else(|| entries.first().map(|entry| (entry, true)));
    match_state(
        document,
        Boundary {
            entry_id: current.map(|(entry, _)| entry.id),
            before: current.is_some_and(|(_, before)| before),
        },
        &mut rows,
        cancelled,
    );
    let mut candidate = document.clone();
    for entry in entries.iter().rev().filter(|entry| !entry.is_undone()) {
        if cancelled.load(Ordering::Relaxed)
            || rows.iter().all(|row| row.covered.is_some())
            || candidate.apply_text_history_undo(entry.id).is_err()
        {
            break;
        }
        match_state(
            &candidate,
            Boundary {
                entry_id: Some(entry.id),
                before: true,
            },
            &mut rows,
            cancelled,
        );
    }
    let mut candidate = document.clone();
    for entry in entries.iter().filter(|entry| entry.is_undone()) {
        if cancelled.load(Ordering::Relaxed)
            || rows.iter().all(|row| row.covered.is_some())
            || candidate.apply_text_history_redo(entry.id).is_err()
        {
            break;
        }
        match_state(
            &candidate,
            Boundary {
                entry_id: Some(entry.id),
                before: false,
            },
            &mut rows,
            cancelled,
        );
    }
    // Equal contents get one destination, retaining the richest return branch.
    rows.sort_by_key(|row| {
        (
            std::cmp::Reverse(row.point.has_history),
            std::cmp::Reverse(row.point.id),
        )
    });
    let mut seen = std::collections::HashSet::new();
    rows.retain(|row| seen.insert(row.point.text_digest.clone()));
    rows.sort_by_key(|row| std::cmp::Reverse(row.point.id));
    rows
}

fn match_state(
    document: &TextDocument,
    boundary: Boundary,
    rows: &mut [SnapshotRow],
    cancelled: &AtomicBool,
) {
    let bytes = document.piece_tree().metrics().bytes;
    // Most typing changes length. Do not hash states whose length cannot match
    // any outstanding snapshot; do not allocate or flatten whole documents.
    let eligible = |row: &SnapshotRow| row.covered.is_none() && row.point.bytes == bytes;
    if !rows.iter().any(eligible) {
        return;
    }
    let snapshot = document.snapshot();
    let mut hash = blake3::Hasher::new();
    for span in snapshot.spans_for_range(0..snapshot.len_chars()) {
        for chunk in span.text.as_bytes().chunks(64 * 1024) {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            hash.update(chunk);
        }
    }
    let digest = hash.finalize();
    for row in rows.iter_mut().filter(|row| eligible(row)) {
        if row.point.text_digest == digest.as_bytes() {
            row.covered = Some(boundary);
        }
    }
}
