mod budget;
mod coalescing;
mod persistence;
mod provenance;

pub use budget::TextHistoryBudget;
#[cfg(test)]
pub(crate) use coalescing::TEXT_HISTORY_SOFT_DIVIDER_PAUSE;
pub(crate) use coalescing::{CoalescedEdit, coalesced_local_edit_record, entry_sealed_by_divider};
pub use persistence::{PersistedCursorRange, PersistedHistoryEdit, PersistedHistoryEntry};
pub(crate) use persistence::{persist_cursor_range, restore_cursor_range};
#[cfg(test)]
pub(crate) use provenance::PIECE_PROVENANCE_ENTRY_LIMIT;
pub(crate) use provenance::PieceProvenanceStore;
pub use provenance::{ByteSpan, PieceProvenance, PieceSource};

use super::piece_tree::PieceTreeLite;
use crate::app::ui::editor_content::native_editor::CursorRange;
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

pub type PieceHistoryEdits = SmallVec<[PieceHistoryEdit; 1]>;

static NEXT_TEXT_HISTORY_GLOBAL_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) const TEXT_HISTORY_COALESCE_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1200);
pub(crate) const TEXT_HISTORY_PREVIEW_MAX_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PieceHistoryFlags {
    pub undone: bool,
    pub replayable: bool,
    pub persisted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PieceHistoryEdit {
    Inserted {
        start_char: u32,
        span: ByteSpan,
    },
    Deleted {
        start_char: u32,
        spans: Vec<ByteSpan>,
    },
    Replaced {
        start_char: u32,
        deleted: Vec<ByteSpan>,
        inserted: ByteSpan,
    },
}

#[derive(Clone, Debug)]
pub struct PieceHistoryEntry {
    pub id: u64,
    pub global_seq: u64,
    pub source: PieceSource,
    pub visible_generation_before: u32,
    pub visible_generation_after: u32,
    pub fingerprint: u64,
    pub summary: String,
    pub edits: PieceHistoryEdits,
    pub flags: PieceHistoryFlags,
    pub previous_selection: CursorRange,
    pub next_selection: CursorRange,
}

impl PieceHistoryEdit {
    pub fn start_char(&self) -> u32 {
        match self {
            Self::Inserted { start_char, .. }
            | Self::Deleted { start_char, .. }
            | Self::Replaced { start_char, .. } => *start_char,
        }
    }

    pub fn deleted_spans(&self) -> &[ByteSpan] {
        match self {
            Self::Inserted { .. } => &[],
            Self::Deleted { spans, .. } => spans,
            Self::Replaced { deleted, .. } => deleted,
        }
    }

    pub fn inserted_span(&self) -> Option<ByteSpan> {
        match self {
            Self::Inserted { span, .. } => Some(*span),
            Self::Deleted { .. } => None,
            Self::Replaced { inserted, .. } => Some(*inserted),
        }
    }

    /// All byte spans this edit references, in fingerprint order
    /// (deleted spans first, then any inserted span).
    pub fn spans(&self) -> impl Iterator<Item = ByteSpan> + '_ {
        self.deleted_spans()
            .iter()
            .copied()
            .chain(self.inserted_span())
    }

    pub fn each_span_mut(&mut self, mut visit: impl FnMut(&mut ByteSpan)) {
        match self {
            Self::Inserted { span, .. } => visit(span),
            Self::Deleted { spans, .. } => spans.iter_mut().for_each(&mut visit),
            Self::Replaced {
                deleted, inserted, ..
            } => {
                deleted.iter_mut().for_each(&mut visit);
                visit(inserted);
            }
        }
    }
}

impl PieceHistoryEntry {
    pub fn is_undone(&self) -> bool {
        self.flags.undone
    }

    pub fn byte_cost(&self) -> usize {
        let edit_bytes: usize = self
            .edits
            .iter()
            .flat_map(PieceHistoryEdit::spans)
            .map(|span| span.byte_len as usize)
            .sum();
        std::mem::size_of::<Self>() + edit_bytes
    }
}

pub(crate) fn next_text_history_global_seq() -> u64 {
    NEXT_TEXT_HISTORY_GLOBAL_SEQ.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn register_text_history_global_seq(seq: u64) {
    let mut current = NEXT_TEXT_HISTORY_GLOBAL_SEQ.load(Ordering::Relaxed);
    while current <= seq {
        match NEXT_TEXT_HISTORY_GLOBAL_SEQ.compare_exchange_weak(
            current,
            seq.saturating_add(1),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next_current) => current = next_current,
        }
    }
}

#[must_use]
pub fn source_label(source: PieceSource) -> &'static str {
    match source {
        PieceSource::Load => "Load",
        PieceSource::Edit => "Editor",
        PieceSource::Paste => "Paste",
        PieceSource::Cut => "Cut",
        PieceSource::SearchReplace => "Search/replace",
    }
}

pub(crate) fn preview_text(text: &str) -> String {
    let flattened = text.replace(['\r', '\n'], " ");
    let mut preview = flattened
        .chars()
        .take(TEXT_HISTORY_PREVIEW_MAX_CHARS)
        .collect::<String>();
    if flattened.chars().count() > TEXT_HISTORY_PREVIEW_MAX_CHARS {
        preview.push_str("...");
    }
    preview
}

pub(crate) fn fingerprint_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    for part in parts {
        part.hash(&mut hasher);
        0xff_u8.hash(&mut hasher);
    }
    hasher.finish()
}

// =============================================================================
// Operation records and direction
// =============================================================================

#[derive(Clone, Copy)]
pub(crate) enum OperationDirection {
    Undo,
    Redo,
}

impl OperationDirection {
    pub(crate) fn selection(self, record: &TextDocumentOperationRecord) -> CursorRange {
        match self {
            OperationDirection::Undo => record.previous_selection,
            OperationDirection::Redo => record.next_selection,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextHistoryApplyError {
    OutOfBounds,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDocumentEditOperation {
    pub start_char: usize,
    pub deleted_text: String,
    pub inserted_text: String,
    pub deleted_spans: Vec<ByteSpan>,
}

impl TextDocumentEditOperation {
    /// Text that is expected to currently exist at `start_char` for the given
    /// replay direction (the "before" side).
    pub(crate) fn expected_text(&self, direction: OperationDirection) -> &str {
        match direction {
            OperationDirection::Undo => &self.inserted_text,
            OperationDirection::Redo => &self.deleted_text,
        }
    }

    /// Text that should replace `expected_text` for the given replay direction
    /// (the "after" side).
    pub(crate) fn replacement_text(&self, direction: OperationDirection) -> &str {
        match direction {
            OperationDirection::Undo => &self.deleted_text,
            OperationDirection::Redo => &self.inserted_text,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextDocumentOperationRecord {
    pub previous_selection: CursorRange,
    pub next_selection: CursorRange,
    pub edits: Vec<TextDocumentEditOperation>,
}

pub(crate) fn record_expected_parts(
    record: &TextDocumentOperationRecord,
    direction: OperationDirection,
) -> Vec<&str> {
    record
        .edits
        .iter()
        .map(|edit| edit.expected_text(direction))
        .collect()
}

pub(crate) fn record_current_parts(
    tree: &PieceTreeLite,
    record: &TextDocumentOperationRecord,
    direction: OperationDirection,
) -> Result<Vec<String>, TextHistoryApplyError> {
    record
        .edits
        .iter()
        .map(|edit| {
            let range =
                edit.start_char..edit.start_char + edit.expected_text(direction).chars().count();
            if range.end > tree.len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            Ok(tree.extract_range(range))
        })
        .collect()
}

pub(crate) fn deleted_spans_or_payload(
    tree: &mut PieceTreeLite,
    edit: &TextDocumentEditOperation,
    source: PieceSource,
) -> Vec<ByteSpan> {
    if !edit.deleted_spans.is_empty() {
        return edit.deleted_spans.clone();
    }
    vec![tree.append_history_text(&edit.deleted_text, source)]
}

// =============================================================================
// Operation summary
// =============================================================================

pub(crate) fn operation_summary(
    source: PieceSource,
    operation: &TextDocumentOperationRecord,
) -> String {
    let edit_count = operation.edits.len();
    if source == PieceSource::SearchReplace {
        return if edit_count == 1 {
            "Replace match".to_owned()
        } else {
            format!("Replace {edit_count} matches")
        };
    }

    let first_edit = operation.edits.first();
    if source == PieceSource::Paste {
        return first_edit.map_or_else(
            || "Paste".to_owned(),
            |edit| format!("Paste \"{}\"", preview_text(&edit.inserted_text)),
        );
    }
    if source == PieceSource::Cut {
        return first_edit.map_or_else(
            || "Cut".to_owned(),
            |edit| format!("Cut \"{}\"", preview_text(&edit.deleted_text)),
        );
    }
    if edit_count != 1 {
        return format!("Edit {edit_count} ranges");
    }

    let Some(edit) = first_edit else {
        return "Edit".to_owned();
    };
    match (edit.deleted_text.is_empty(), edit.inserted_text.is_empty()) {
        (true, false) => format!("Insert \"{}\"", preview_text(&edit.inserted_text)),
        (false, true) => format!("Delete \"{}\"", preview_text(&edit.deleted_text)),
        (false, false) => format!("Replace with \"{}\"", preview_text(&edit.inserted_text)),
        (true, true) => "Edit".to_owned(),
    }
}
