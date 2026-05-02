use crate::app::domain::buffer::{PieceHistoryEdit, PieceHistoryEntry, preview_text};
use crate::app::domain::{BufferId, BufferState, PieceSource};

#[derive(Default)]
pub(crate) struct TextHistoryCache {
    pub(crate) revisions: Vec<(BufferId, u64)>,
    pub(crate) entries: Vec<TextHistoryEntryView>,
}

#[derive(Clone, Debug)]
pub(crate) struct TextHistoryEntryView {
    pub(crate) id: u64,
    pub(crate) seq: u64,
    pub(crate) buffer_id: BufferId,
    pub(crate) label: String,
    pub(crate) source: PieceSource,
    pub(crate) summary: String,
    pub(crate) undone: bool,
    pub(crate) replayable: bool,
    pub(crate) edit_count: usize,
    pub(crate) first_deleted_text: String,
    pub(crate) first_inserted_text: String,
}

pub(crate) fn entries_for_buffer(buffer: &BufferState) -> Vec<TextHistoryEntryView> {
    let label = buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| buffer.name.clone());
    buffer
        .document()
        .history_entries()
        .iter()
        .map(|entry| entry_view(buffer.id, &label, buffer, entry))
        .collect()
}

fn entry_view(
    buffer_id: BufferId,
    label: &str,
    buffer: &BufferState,
    entry: &PieceHistoryEntry,
) -> TextHistoryEntryView {
    let (deleted, inserted) = first_text_pair(buffer, entry);
    TextHistoryEntryView {
        id: entry.id,
        seq: entry.seq,
        buffer_id,
        label: label.to_owned(),
        source: entry.source,
        summary: if entry.summary.is_empty() {
            operation_summary(entry.source, entry.edits.len(), &deleted, &inserted)
        } else {
            entry.summary.clone()
        },
        undone: entry.is_undone(),
        replayable: entry.flags.replayable,
        edit_count: entry.edits.len(),
        first_deleted_text: deleted,
        first_inserted_text: inserted,
    }
}

fn first_text_pair(buffer: &BufferState, entry: &PieceHistoryEntry) -> (String, String) {
    let Some(edit) = entry.edits.first() else {
        return (String::new(), String::new());
    };
    let tree = buffer.document().piece_tree();
    match edit {
        PieceHistoryEdit::Inserted { span, .. } => {
            (String::new(), tree.text_for_span(*span).to_owned())
        }
        PieceHistoryEdit::Deleted { spans, .. } => (text_for_spans(buffer, spans), String::new()),
        PieceHistoryEdit::Replaced {
            deleted, inserted, ..
        } => (
            text_for_spans(buffer, deleted),
            tree.text_for_span(*inserted).to_owned(),
        ),
    }
}

fn text_for_spans(buffer: &BufferState, spans: &[crate::app::domain::buffer::ByteSpan]) -> String {
    let mut text = String::new();
    let tree = buffer.document().piece_tree();
    for span in spans {
        text.push_str(tree.text_for_span(*span));
    }
    text
}

fn operation_summary(
    source: PieceSource,
    edit_count: usize,
    deleted_text: &str,
    inserted_text: &str,
) -> String {
    match source {
        PieceSource::SearchReplace if edit_count == 1 => "Replace match".to_owned(),
        PieceSource::SearchReplace => format!("Replace {edit_count} matches"),
        PieceSource::Paste => format!("Paste \"{}\"", preview_text(inserted_text)),
        PieceSource::Cut => format!("Cut \"{}\"", preview_text(deleted_text)),
        _ if edit_count != 1 => format!("Edit {edit_count} ranges"),
        _ => match (deleted_text.is_empty(), inserted_text.is_empty()) {
            (true, false) => format!("Insert \"{}\"", preview_text(inserted_text)),
            (false, true) => format!("Delete \"{}\"", preview_text(deleted_text)),
            (false, false) => format!("Replace with \"{}\"", preview_text(inserted_text)),
            (true, true) => "Edit".to_owned(),
        },
    }
}
