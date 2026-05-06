use crate::app::domain::{BufferId, PieceSource, source_label};
use crate::app::text_history::TextHistoryEntryView;
use egui_phosphor::regular::{
    ARROWS_LEFT_RIGHT, BACKSPACE, CLIPBOARD, MAGNIFYING_GLASS, PENCIL_SIMPLE, SCISSORS, STACK,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) struct TextHistoryRow {
    pub(super) buffer_id: BufferId,
    pub(super) entry_id: u64,
    pub(super) global_seq: u64,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) icon: &'static str,
    pub(super) undone: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TextHistoryAction {
    pub(super) buffer_id: BufferId,
    pub(super) entry_id: u64,
}

#[derive(Clone)]
pub(super) struct TextHistoryFileGroup {
    pub(super) buffer_id: BufferId,
    pub(super) label: String,
    pub(super) rows: Vec<TextHistoryRow>,
}

pub(super) fn row_from_entry(entry: &TextHistoryEntryView) -> TextHistoryRow {
    TextHistoryRow {
        buffer_id: entry.buffer_id,
        entry_id: entry.id,
        global_seq: entry.global_seq,
        title: entry.summary.clone(),
        detail: format!("{} · {}", entry.label, source_label(entry.source)),
        icon: entry_icon(entry),
        undone: entry.undone,
    }
}

pub(super) fn file_groups_from_entries<'a>(
    entries: impl Iterator<Item = &'a TextHistoryEntryView>,
) -> Vec<TextHistoryFileGroup> {
    let mut groups = Vec::<TextHistoryFileGroup>::new();
    for entry in entries {
        let row = row_from_entry(entry);
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.buffer_id == entry.buffer_id)
        {
            group.rows.push(row);
        } else {
            groups.push(TextHistoryFileGroup {
                buffer_id: entry.buffer_id,
                label: entry.label.clone(),
                rows: vec![row],
            });
        }
    }
    for group in &mut groups {
        group.rows.reverse();
    }
    groups.sort_by(|left, right| {
        let left_seq = left
            .rows
            .first()
            .map(|row| row.global_seq)
            .unwrap_or_default();
        let right_seq = right
            .rows
            .first()
            .map(|row| row.global_seq)
            .unwrap_or_default();
        right_seq
            .cmp(&left_seq)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.buffer_id.cmp(&right.buffer_id))
    });
    groups
}

fn entry_icon(entry: &TextHistoryEntryView) -> &'static str {
    match entry.source {
        PieceSource::SearchReplace => MAGNIFYING_GLASS,
        PieceSource::Paste => CLIPBOARD,
        PieceSource::Cut => SCISSORS,
        _ if entry.edit_count != 1 => STACK,
        _ => match (
            entry.first_deleted_text.is_empty(),
            entry.first_inserted_text.is_empty(),
        ) {
            (false, true) => BACKSPACE,
            (false, false) => ARROWS_LEFT_RIGHT,
            (true, _) => PENCIL_SIMPLE,
        },
    }
}

/// Each buffer with a mixed applied/undone state contributes one Now-line in
/// the global Timeline, anchored to its newest applied entry. Rows are already
/// newest-first, so the Now-line appears just above that row, separating the
/// buffer's redo targets (newer, above) from its applied entries (older, below).
pub(super) fn timeline_now_line_anchors(rows: &[TextHistoryRow]) -> HashSet<(BufferId, u64)> {
    let mut newest_applied: HashMap<BufferId, u64> = HashMap::new();
    let mut has_undone: HashSet<BufferId> = HashSet::new();
    for row in rows {
        if row.undone {
            has_undone.insert(row.buffer_id);
        } else {
            newest_applied.entry(row.buffer_id).or_insert(row.entry_id);
        }
    }
    newest_applied
        .into_iter()
        .filter(|(buffer_id, _)| has_undone.contains(buffer_id))
        .collect()
}

pub(super) fn newest_applied_index(rows: &[TextHistoryRow]) -> Option<usize> {
    rows.iter().position(|row| !row.undone)
}

pub(super) fn per_file_now_line_insert_index(rows: &[TextHistoryRow]) -> Option<usize> {
    let insert_index = newest_applied_index(rows)?;
    (insert_index > 0).then_some(insert_index)
}
