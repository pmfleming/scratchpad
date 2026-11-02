use crate::app::domain::buffer::{
    HistoryToken, PieceHistoryEdit, PieceHistoryEntry, preview_text, preview_text_parts,
};
use crate::app::domain::{BufferId, BufferState, PieceSource};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct TextHistoryCache {
    pub(crate) revisions: Vec<(BufferId, HistoryToken, String)>,
    pub(crate) entries: Arc<[TextHistoryEntryView]>,
}

#[derive(Clone, Debug)]
pub(crate) struct TextHistoryEntryView {
    pub(crate) id: u64,
    pub(crate) global_seq: u64,
    pub(crate) buffer_id: BufferId,
    pub(crate) label: String,
    pub(crate) source: PieceSource,
    pub(crate) summary: String,
    pub(crate) undone: bool,
    pub(crate) replayable: bool,
    pub(crate) edit_count: usize,
    pub(crate) deleted_preview: String,
    pub(crate) inserted_preview: String,
}

pub(crate) fn entries_for_buffer(buffer: &BufferState) -> Vec<TextHistoryEntryView> {
    let label = buffer_label(buffer);
    buffer
        .document()
        .history_entries()
        .iter()
        .map(|entry| entry_view(buffer.id, &label, buffer, entry))
        .collect()
}

pub(crate) fn buffer_label(buffer: &BufferState) -> String {
    buffer
        .path
        .as_ref()
        .map_or_else(|| buffer.name.clone(), |path| path.display().to_string())
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
        global_seq: entry.global_seq,
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
        deleted_preview: deleted,
        inserted_preview: inserted,
    }
}

fn first_text_pair(buffer: &BufferState, entry: &PieceHistoryEntry) -> (String, String) {
    let Some(edit) = entry.edits.first() else {
        return (String::new(), String::new());
    };
    let tree = buffer.document().piece_tree();
    match edit {
        PieceHistoryEdit::Inserted { span, .. } => {
            (String::new(), preview_text(&tree.text_for_span(*span)))
        }
        PieceHistoryEdit::Deleted { spans, .. } => (text_for_spans(buffer, spans), String::new()),
        PieceHistoryEdit::Replaced {
            deleted, inserted, ..
        } => (
            text_for_spans(buffer, deleted),
            preview_text(&tree.text_for_span(*inserted)),
        ),
    }
}

fn text_for_spans(buffer: &BufferState, spans: &[crate::app::domain::buffer::ByteSpan]) -> String {
    let tree = buffer.document().piece_tree();
    preview_text_parts(spans.iter().map(|span| tree.text_for_span(*span)))
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

#[cfg(test)]
mod tests {
    use super::operation_summary;
    use crate::app::domain::PieceSource;

    #[test]
    fn history_previews_bound_large_unicode_edits_and_join_deleted_spans() {
        use crate::app::domain::{BufferState, CursorRange};
        let original = "é\n".repeat(100_000);
        let mut buffer = BufferState::new("large".into(), original, None);
        buffer
            .replace_char_ranges_with_undo(
                &[(0..200_000, "🦀".repeat(100_000))],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        let entries = super::entries_for_buffer(&buffer);
        assert_eq!(
            entries[0].deleted_preview,
            format!("{}...", "é ".repeat(40))
        );
        assert_eq!(
            entries[0].inserted_preview,
            format!("{}...", "🦀".repeat(80))
        );
        assert_eq!(buffer.current_file_length().chars, 100_000);
        assert_eq!(super::preview_text_parts(["é\r", "\n🦀"]), "é  🦀");
        assert_eq!(
            super::preview_text_parts(["x".repeat(40), "y".repeat(40)]),
            format!("{}{}", "x".repeat(40), "y".repeat(40))
        );
        // A full preview must not even fetch subsequent pieces of a large edit.
        let parts =
            std::iter::once("z".repeat(81)).chain(std::iter::from_fn(|| -> Option<String> {
                panic!("read past preview")
            }));
        assert_eq!(
            super::preview_text_parts(parts),
            format!("{}...", "z".repeat(80))
        );
    }

    #[test]
    fn operation_summaries_cover_sources_and_edit_shapes() {
        let summary =
            |source, count, deleted, inserted| operation_summary(source, count, deleted, inserted);
        assert_eq!(
            summary(PieceSource::SearchReplace, 1, "", "new"),
            "Replace match"
        );
        assert_eq!(
            summary(PieceSource::SearchReplace, 3, "", "new"),
            "Replace 3 matches"
        );
        assert_eq!(summary(PieceSource::Paste, 1, "", "new"), "Paste \"new\"");
        assert_eq!(summary(PieceSource::Cut, 1, "old", ""), "Cut \"old\"");
        assert_eq!(summary(PieceSource::Edit, 2, "old", "new"), "Edit 2 ranges");
        assert_eq!(summary(PieceSource::Edit, 1, "", "new"), "Insert \"new\"");
        assert_eq!(summary(PieceSource::Edit, 1, "old", ""), "Delete \"old\"");
        assert_eq!(
            summary(PieceSource::Edit, 1, "old", "new"),
            "Replace with \"new\""
        );
        assert_eq!(summary(PieceSource::Edit, 1, "", ""), "Edit");
    }
}
