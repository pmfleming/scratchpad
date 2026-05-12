mod budget;
mod coalescing;

pub use budget::TextHistoryBudget;
#[cfg(test)]
pub(crate) use coalescing::TEXT_HISTORY_SOFT_DIVIDER_PAUSE;
pub(crate) use coalescing::{CoalescedEdit, coalesced_local_edit_record, entry_sealed_by_divider};

use super::piece_tree::{PieceBuffer, PieceTreeLite};
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
use smallvec::SmallVec;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

pub type PieceHistoryEdits = SmallVec<[PieceHistoryEdit; 1]>;

static NEXT_TEXT_HISTORY_GLOBAL_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) const TEXT_HISTORY_COALESCE_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1200);
pub(crate) const TEXT_HISTORY_PREVIEW_MAX_CHARS: usize = 80;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum PieceSource {
    #[default]
    Load = 0,
    Edit,
    Paste,
    Cut,
    SearchReplace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PieceProvenance {
    pub change_id: u64,
    pub source: PieceSource,
    pub session_generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ByteSpan {
    pub buffer: PieceBuffer,
    pub start_byte: u32,
    pub byte_len: u32,
}

impl ByteSpan {
    pub fn byte_end(self) -> u32 {
        self.start_byte.saturating_add(self.byte_len)
    }
}

pub(crate) const PIECE_PROVENANCE_ENTRY_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PieceProvenanceKey {
    buffer: PieceBuffer,
    start_byte: u32,
}

impl From<ByteSpan> for PieceProvenanceKey {
    fn from(span: ByteSpan) -> Self {
        Self {
            buffer: span.buffer,
            start_byte: span.start_byte,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PieceProvenanceEntry {
    byte_len: u32,
    provenance: PieceProvenance,
}

#[derive(Clone, Debug, Default)]
pub struct PieceProvenanceStore {
    sparse: HashMap<PieceProvenanceKey, PieceProvenanceEntry>,
    insertion_order: VecDeque<PieceProvenanceKey>,
}

impl PieceProvenanceStore {
    pub fn record(&mut self, span: ByteSpan, provenance: PieceProvenance) {
        if provenance.source == PieceSource::Load || span.byte_len == 0 {
            return;
        }
        let key = PieceProvenanceKey::from(span);
        if !self.sparse.contains_key(&key) {
            self.insertion_order.push_back(key);
        }
        self.sparse.insert(
            key,
            PieceProvenanceEntry {
                byte_len: span.byte_len,
                provenance,
            },
        );
        self.evict_over_limit();
    }

    pub fn provenance_for(&self, span: ByteSpan) -> PieceProvenance {
        self.sparse
            .get(&PieceProvenanceKey::from(span))
            .filter(|entry| entry.byte_len == span.byte_len)
            .map(|entry| entry.provenance)
            .unwrap_or_default()
    }

    pub fn rewrite_add_spans(&mut self, spans: impl IntoIterator<Item = (ByteSpan, ByteSpan)>) {
        let mut rewritten = Self::default();
        for (old_span, new_span) in spans {
            if old_span.buffer != PieceBuffer::Add
                || new_span.buffer != PieceBuffer::Add
                || new_span.byte_len == 0
            {
                continue;
            }
            let provenance = self.provenance_for(old_span);
            if provenance.source != PieceSource::Load {
                rewritten.record(new_span, provenance);
            }
        }
        *self = rewritten;
    }

    fn evict_over_limit(&mut self) {
        while self.sparse.len() > PIECE_PROVENANCE_ENTRY_LIMIT {
            let Some(key) = self.insertion_order.pop_front() else {
                break;
            };
            self.sparse.remove(&key);
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.sparse.len()
    }
}

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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistedCursorRange {
    pub primary_index: usize,
    pub primary_prefer_next_row: bool,
    pub secondary_index: usize,
    pub secondary_prefer_next_row: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistedHistoryEntry {
    pub id: u64,
    pub global_seq: u64,
    pub source: PieceSource,
    pub visible_generation_before: u32,
    pub visible_generation_after: u32,
    pub fingerprint: u64,
    #[serde(default)]
    pub summary: String,
    pub flags: PieceHistoryFlags,
    pub previous_selection: PersistedCursorRange,
    pub next_selection: PersistedCursorRange,
    pub edits: Vec<PersistedHistoryEdit>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedHistoryEdit {
    Inserted {
        start_char: u32,
        inserted_len: u32,
        inserted_payload: Option<String>,
    },
    Deleted {
        start_char: u32,
        deleted_len: u32,
        deleted_payload: Option<String>,
    },
    Replaced {
        start_char: u32,
        deleted_len: u32,
        inserted_len: u32,
        deleted_payload: Option<String>,
        inserted_payload: Option<String>,
    },
}

impl PersistedHistoryEdit {
    pub fn payload_bytes(&self) -> usize {
        match self {
            Self::Inserted {
                inserted_payload, ..
            } => inserted_payload.as_ref().map_or(0, String::len),
            Self::Deleted {
                deleted_payload, ..
            } => deleted_payload.as_ref().map_or(0, String::len),
            Self::Replaced {
                deleted_payload,
                inserted_payload,
                ..
            } => {
                deleted_payload.as_ref().map_or(0, String::len)
                    + inserted_payload.as_ref().map_or(0, String::len)
            }
        }
    }

    pub fn drop_payload(&mut self) {
        match self {
            Self::Inserted {
                inserted_payload, ..
            } => *inserted_payload = None,
            Self::Deleted {
                deleted_payload, ..
            } => *deleted_payload = None,
            Self::Replaced {
                deleted_payload,
                inserted_payload,
                ..
            } => {
                *deleted_payload = None;
                *inserted_payload = None;
            }
        }
    }

    pub fn has_all_payloads(&self) -> bool {
        match self {
            Self::Inserted {
                inserted_payload, ..
            } => inserted_payload.is_some(),
            Self::Deleted {
                deleted_payload, ..
            } => deleted_payload.is_some(),
            Self::Replaced {
                deleted_payload,
                inserted_payload,
                ..
            } => deleted_payload.is_some() && inserted_payload.is_some(),
        }
    }
}

impl PersistedHistoryEntry {
    pub fn payload_bytes(&self) -> usize {
        self.edits
            .iter()
            .map(PersistedHistoryEdit::payload_bytes)
            .sum()
    }

    pub fn drop_payloads(&mut self) {
        for edit in &mut self.edits {
            edit.drop_payload();
        }
        self.flags.replayable = false;
    }

    pub fn has_all_payloads(&self) -> bool {
        self.edits
            .iter()
            .all(PersistedHistoryEdit::has_all_payloads)
    }
}

fn char_len_u32(text: &str) -> u32 {
    text.chars().count().min(u32::MAX as usize) as u32
}

pub(crate) fn empty_byte_span() -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: 0,
        byte_len: 0,
    }
}

impl PieceHistoryEdit {
    /// Build a persisted form. `text_for_span` is the caller's view onto a
    /// byte span (typically `tree.text_for_span(...)`).
    pub fn to_persisted(
        &self,
        mut text_for_span: impl FnMut(ByteSpan) -> String,
    ) -> PersistedHistoryEdit {
        match self {
            PieceHistoryEdit::Inserted { start_char, span } => {
                let text = text_for_span(*span);
                PersistedHistoryEdit::Inserted {
                    start_char: *start_char,
                    inserted_len: char_len_u32(&text),
                    inserted_payload: Some(text),
                }
            }
            PieceHistoryEdit::Deleted { start_char, spans } => {
                let text = spans
                    .iter()
                    .copied()
                    .map(&mut text_for_span)
                    .collect::<String>();
                PersistedHistoryEdit::Deleted {
                    start_char: *start_char,
                    deleted_len: char_len_u32(&text),
                    deleted_payload: Some(text),
                }
            }
            PieceHistoryEdit::Replaced {
                start_char,
                deleted,
                inserted,
            } => {
                let deleted_text = deleted
                    .iter()
                    .copied()
                    .map(&mut text_for_span)
                    .collect::<String>();
                let inserted_text = text_for_span(*inserted);
                PersistedHistoryEdit::Replaced {
                    start_char: *start_char,
                    deleted_len: char_len_u32(&deleted_text),
                    inserted_len: char_len_u32(&inserted_text),
                    deleted_payload: Some(deleted_text),
                    inserted_payload: Some(inserted_text),
                }
            }
        }
    }
}

impl PersistedHistoryEdit {
    /// Reconstruct a piece-tree edit. `append_text` is the caller's
    /// span-allocator (typically `tree.append_history_text(text, source)`).
    pub fn into_piece(self, mut append_text: impl FnMut(&str) -> ByteSpan) -> PieceHistoryEdit {
        let span_or_empty = |payload: Option<String>, append: &mut dyn FnMut(&str) -> ByteSpan| {
            payload
                .as_deref()
                .map(append)
                .unwrap_or_else(empty_byte_span)
        };
        let span_vec = |payload: Option<String>, append: &mut dyn FnMut(&str) -> ByteSpan| {
            payload
                .as_deref()
                .map(append)
                .map(|span| vec![span])
                .unwrap_or_default()
        };
        match self {
            PersistedHistoryEdit::Inserted {
                start_char,
                inserted_payload,
                ..
            } => PieceHistoryEdit::Inserted {
                start_char,
                span: span_or_empty(inserted_payload, &mut append_text),
            },
            PersistedHistoryEdit::Deleted {
                start_char,
                deleted_payload,
                ..
            } => PieceHistoryEdit::Deleted {
                start_char,
                spans: span_vec(deleted_payload, &mut append_text),
            },
            PersistedHistoryEdit::Replaced {
                start_char,
                deleted_payload,
                inserted_payload,
                ..
            } => PieceHistoryEdit::Replaced {
                start_char,
                deleted: span_vec(deleted_payload, &mut append_text),
                inserted: span_or_empty(inserted_payload, &mut append_text),
            },
        }
    }
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

// =============================================================================
// Persistence helpers
// =============================================================================

pub(crate) fn persist_cursor_range(range: CursorRange) -> PersistedCursorRange {
    PersistedCursorRange {
        primary_index: range.primary.index,
        primary_prefer_next_row: range.primary.prefer_next_row,
        secondary_index: range.secondary.index,
        secondary_prefer_next_row: range.secondary.prefer_next_row,
    }
}

pub(crate) fn restore_cursor_range(range: PersistedCursorRange) -> CursorRange {
    CursorRange {
        primary: CharCursor {
            index: range.primary_index,
            prefer_next_row: range.primary_prefer_next_row,
        },
        secondary: CharCursor {
            index: range.secondary_index,
            prefer_next_row: range.secondary_prefer_next_row,
        },
    }
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
        return first_edit
            .map(|edit| format!("Paste \"{}\"", preview_text(&edit.inserted_text)))
            .unwrap_or_else(|| "Paste".to_owned());
    }
    if source == PieceSource::Cut {
        return first_edit
            .map(|edit| format!("Cut \"{}\"", preview_text(&edit.deleted_text)))
            .unwrap_or_else(|| "Cut".to_owned());
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
