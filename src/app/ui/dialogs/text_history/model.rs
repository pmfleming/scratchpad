use crate::app::domain::{BufferId, PieceSource, source_label};
use crate::app::text_history::TextHistoryEntryView;
use egui_phosphor::regular::{
    ARROWS_LEFT_RIGHT, BACKSPACE, CLIPBOARD, MAGNIFYING_GLASS, PENCIL_SIMPLE, SCISSORS, STACK,
};
use std::collections::HashMap;

pub(super) struct TextHistoryRow<'a> {
    pub(super) buffer_id: BufferId,
    pub(super) entry_id: u64,
    pub(super) global_seq: u64,
    pub(super) title: &'a str,
    pub(super) label: &'a str,
    pub(super) source: &'static str,
    pub(super) icon: &'static str,
    pub(super) undone: bool,
    pub(super) deleted: &'a str,
    pub(super) inserted: &'a str,
    pub(super) edit_count: usize,
}

#[derive(Clone, Copy)]
pub(super) struct TextHistoryAction {
    pub(super) buffer_id: BufferId,
    pub(super) entry_id: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HistoryAction {
    State {
        buffer_id: BufferId,
        after: Option<u64>,
    },
    Latest {
        buffer_id: BufferId,
    },
    Snapshot {
        buffer_id: BufferId,
        version_id: i64,
    },
}

pub(super) fn row_from_entry(entry: &TextHistoryEntryView) -> TextHistoryRow<'_> {
    TextHistoryRow {
        buffer_id: entry.buffer_id,
        entry_id: entry.id,
        global_seq: entry.global_seq,
        title: &entry.summary,
        label: &entry.label,
        source: source_label(entry.source),
        icon: entry_icon(entry),
        undone: entry.undone,
        deleted: &entry.deleted_preview,
        inserted: &entry.inserted_preview,
        edit_count: entry.edit_count,
    }
}

pub(super) fn timeline_rows_from_entries<'a>(
    entries: impl Iterator<Item = &'a TextHistoryEntryView>,
) -> Vec<TextHistoryRow<'a>> {
    let mut rows = entries.map(row_from_entry).collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .global_seq
            .cmp(&left.global_seq)
            .then_with(|| left.buffer_id.cmp(&right.buffer_id))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    rows
}

pub(super) fn entry_counts(entries: &[TextHistoryEntryView]) -> HashMap<BufferId, usize> {
    let mut counts = HashMap::new();
    for entry in entries {
        *counts.entry(entry.buffer_id).or_default() += 1;
    }
    counts
}

fn entry_icon(entry: &TextHistoryEntryView) -> &'static str {
    match entry.source {
        PieceSource::SearchReplace => MAGNIFYING_GLASS,
        PieceSource::Paste => CLIPBOARD,
        PieceSource::Cut => SCISSORS,
        _ if entry.edit_count != 1 => STACK,
        _ => match (
            entry.deleted_preview.is_empty(),
            entry.inserted_preview.is_empty(),
        ) {
            (false, true) => BACKSPACE,
            (false, false) => ARROWS_LEFT_RIGHT,
            (true, _) => PENCIL_SIMPLE,
        },
    }
}
