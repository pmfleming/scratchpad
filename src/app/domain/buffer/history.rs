mod budget;
mod coalescing;
mod persistence;
mod provenance;

pub use budget::TextHistoryBudget;
#[cfg(test)]
pub(crate) use coalescing::TEXT_HISTORY_SOFT_DIVIDER_PAUSE;
pub(crate) use coalescing::{CoalescedEdit, coalesced_local_edit_record, entry_sealed_by_divider};
pub use persistence::{PersistedHistoryEdit, PersistedHistoryEntry};
pub(crate) use persistence::{persist_cursor_range, restore_cursor_range};
pub use provenance::{ByteSpan, PieceSource};

use super::piece_tree::PieceTreeLite;
use crate::app::domain::CursorRange;
use smallvec::SmallVec;
use std::sync::atomic::{AtomicU64, Ordering};

pub type PieceHistoryEdits = SmallVec<[PieceHistoryEdit; 1]>;

static NEXT_TEXT_HISTORY_GLOBAL_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) const TEXT_HISTORY_COALESCE_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1200);
pub(crate) const TEXT_HISTORY_PREVIEW_MAX_CHARS: usize = 80;
#[cfg(test)]
pub(crate) const TEXT_HISTORY_ROOT_CHECKPOINT_MIN_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PieceHistoryFlags {
    pub undone: bool,
    pub replayable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PieceHistoryEdit {
    Inserted {
        start_char: u64,
        span: ByteSpan,
    },
    Deleted {
        start_char: u64,
        spans: Vec<ByteSpan>,
    },
    Replaced {
        start_char: u64,
        deleted: Vec<ByteSpan>,
        inserted: ByteSpan,
    },
}

#[derive(Clone, Debug)]
pub struct PieceHistoryEntry {
    pub id: u64,
    pub parent_id: u64,
    pub edited_at: u64,
    pub global_seq: u64,
    pub source: PieceSource,
    pub summary: String,
    pub edits: PieceHistoryEdits,
    pub flags: PieceHistoryFlags,
    pub previous_selection: CursorRange,
    pub next_selection: CursorRange,
}

impl PieceHistoryEdit {
    pub fn start_char(&self) -> u64 {
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
}

/// Exact, bounded payload used only for history labels, never replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryPreview {
    pub inserted: bool,
    pub text: String,
}

impl HistoryPreview {
    pub const MAX_BYTES: u64 = 256;

    /// Summaries flatten line endings, so whitespace must be read from the span.
    /// Ordinary text and multi-range edits need no payload for their labels.
    pub fn needed(source: PieceSource, summary: &str) -> bool {
        if matches!(source, PieceSource::Load | PieceSource::External) {
            return false;
        }
        summary == "Replace match"
            || summary.split_once('"').is_some_and(|(_, text)| {
                let text = text.strip_suffix('"').unwrap_or(text);
                let text = text.strip_suffix("...").unwrap_or(text);
                !text.is_empty() && text.chars().all(char::is_whitespace)
            })
    }
}

impl PieceHistoryEntry {
    pub(crate) fn label_preview(&self, tree: &PieceTreeLite) -> Option<HistoryPreview> {
        if !HistoryPreview::needed(self.source, &self.summary) {
            return None;
        }
        let [edit] = self.edits.as_slice() else {
            return None;
        };
        let mut spans = edit.spans();
        let span = spans.next()?;
        if spans.next().is_some() || span.byte_len > HistoryPreview::MAX_BYTES {
            return None;
        }
        Some(HistoryPreview {
            inserted: edit.inserted_span().is_some(),
            text: tree.text_for_span(span).to_string(),
        })
    }

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

pub(crate) fn preview_text(text: &str) -> String {
    preview_text_parts([text])
}

/// Flatten and truncate without allocating or scanning the remainder of an edit.
pub(crate) fn preview_text_parts(parts: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut preview = String::new();
    let mut count = 0;
    for part in parts {
        for ch in part.as_ref().chars() {
            if count == TEXT_HISTORY_PREVIEW_MAX_CHARS {
                preview.push_str("...");
                return preview;
            }
            preview.push(if matches!(ch, '\r' | '\n') { ' ' } else { ch });
            count += 1;
        }
    }
    preview
}

pub(crate) fn fingerprint_parts(parts: impl IntoIterator<Item = impl AsRef<str>>) -> u64 {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part.as_ref().as_bytes());
        hasher.update(&[255]);
    }
    u64::from_le_bytes(
        hasher.finalize().as_bytes()[..8]
            .try_into()
            .expect("eight digest bytes"),
    )
}

// =============================================================================
// Operation records and direction
// =============================================================================

#[derive(Clone, Copy)]
pub(crate) enum OperationDirection {
    Undo,
    Redo,
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

/// Editor input before it is recorded in document history.
#[derive(Clone, Debug, PartialEq)]
pub struct OperationRecord {
    pub previous_cursor: CursorRange,
    pub next_cursor: CursorRange,
    pub edits: Vec<TextDocumentEditOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextDocumentOperationRecord {
    pub previous_selection: CursorRange,
    pub next_selection: CursorRange,
    pub edits: Vec<TextDocumentEditOperation>,
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
    if source == PieceSource::External {
        return "External changes loaded".to_owned();
    }
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
    let [edit] = operation.edits.as_slice() else {
        return format!("Edit {edit_count} ranges");
    };
    match (edit.deleted_text.is_empty(), edit.inserted_text.is_empty()) {
        (true, false) => format!("Insert \"{}\"", preview_text(&edit.inserted_text)),
        (false, true) => format!("Delete \"{}\"", preview_text(&edit.deleted_text)),
        (false, false) => format!("Replace with \"{}\"", preview_text(&edit.inserted_text)),
        (true, true) => "Edit".to_owned(),
    }
}
