use super::piece_tree::{PieceBuffer, PieceTreeLite};
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

pub type PieceHistoryEdits = SmallVec<[PieceHistoryEdit; 1]>;

static NEXT_TEXT_HISTORY_GLOBAL_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) const TEXT_HISTORY_COALESCE_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1200);
pub(crate) const TEXT_HISTORY_PREVIEW_MAX_CHARS: usize = 80;
const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextHistoryBudget {
    pub per_file_entry_limit: usize,
    pub per_file_byte_budget: u64,
    pub aggregate_byte_budget: u64,
    pub persisted_payload_budget: u64,
    pub derived_from_memory: bool,
}

impl Default for TextHistoryBudget {
    fn default() -> Self {
        Self::derive_from_available_memory()
    }
}

impl TextHistoryBudget {
    pub fn derive_from_available_memory() -> Self {
        let available = available_memory_bytes().unwrap_or(2 * 1024 * MIB);
        let aggregate = clamp_u64(available / 50, 16 * MIB, 512 * MIB);
        let per_file = clamp_u64(aggregate / 8, 4 * MIB, 64 * MIB);
        let persisted = clamp_u64(aggregate / 16, MIB, 16 * MIB);
        let entries = clamp_u64(per_file / (8 * 1024), 500, 10_000) as usize;
        Self {
            per_file_entry_limit: entries,
            per_file_byte_budget: per_file,
            aggregate_byte_budget: aggregate,
            persisted_payload_budget: persisted,
            derived_from_memory: true,
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.per_file_entry_limit = self.per_file_entry_limit.clamp(100, 100_000);
        self.per_file_byte_budget = self.per_file_byte_budget.clamp(MIB, 1024 * MIB);
        self.aggregate_byte_budget = self.aggregate_byte_budget.clamp(4 * MIB, 4096 * MIB);
        self.persisted_payload_budget = self.persisted_payload_budget.clamp(0, 1024 * MIB);
        self
    }
}

fn clamp_u64(value: u64, min: u64, max: u64) -> u64 {
    value.clamp(min, max)
}

fn available_memory_bytes() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let available = system.available_memory();
    (available > 0).then_some(available)
}

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

#[derive(Clone, Debug, Default)]
pub struct PieceProvenanceStore {
    sparse: HashMap<ByteSpan, PieceProvenance>,
}

impl PieceProvenanceStore {
    pub fn record(&mut self, span: ByteSpan, provenance: PieceProvenance) {
        if provenance.source == PieceSource::Load || span.byte_len == 0 {
            return;
        }
        self.sparse.insert(span, provenance);
    }

    pub fn provenance_for(&self, span: ByteSpan) -> PieceProvenance {
        self.sparse.get(&span).copied().unwrap_or_default()
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

/// After a soft divider (`,` `;` `:`) the entry is sealed only if the next
/// keystroke arrives later than this. Inside this window the entry keeps
/// growing past the soft boundary so a continuous typing burst stays one entry.
pub(crate) const TEXT_HISTORY_SOFT_DIVIDER_PAUSE: std::time::Duration =
    std::time::Duration::from_millis(400);

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

pub(crate) enum CoalescedEdit {
    Record(TextDocumentOperationRecord),
    Noop,
}

// =============================================================================
// Coalescing
// =============================================================================

fn is_hard_divider(ch: char) -> bool {
    matches!(ch, '.' | '?' | '!' | '\n' | '\r')
}

fn is_soft_divider(ch: char) -> bool {
    matches!(ch, ',' | ';' | ':' | '-')
}

fn cursor_ranges_share_position(left: &CursorRange, right: &CursorRange) -> bool {
    left.primary.index == right.primary.index && left.secondary.index == right.secondary.index
}

fn coalescable_edit_text(edit: &TextDocumentEditOperation) -> bool {
    // Inserts may contain a hard divider (newline) that joins the prior entry
    // and then seals it; only the delete side is restricted to single line.
    is_single_line(&edit.deleted_text)
}

/// Decide whether the latest entry has been "sealed" by a divider in its
/// inserted text. Hard dividers always seal; soft dividers seal only if the
/// user paused after typing them.
pub(crate) fn entry_sealed_by_divider(
    latest: &TextDocumentOperationRecord,
    elapsed: Option<std::time::Duration>,
) -> bool {
    let Some(edit) = latest.edits.last() else {
        return false;
    };
    let Some(last_char) = edit.inserted_text.chars().next_back() else {
        return false;
    };
    if is_hard_divider(last_char) {
        return true;
    }
    if is_soft_divider(last_char) {
        return elapsed.is_none_or(|d| d >= TEXT_HISTORY_SOFT_DIVIDER_PAUSE);
    }
    false
}

pub(crate) fn coalesced_local_edit_record(
    latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let ([latest_edit], [incoming_edit]) = (latest.edits.as_slice(), incoming.edits.as_slice())
    else {
        return None;
    };
    // Compare by index only — `prefer_next_row` is a UI hint that flips at
    // soft-wrap boundaries, and treating it as a cursor jump would split
    // continuous typing into spurious entries.
    if !cursor_ranges_share_position(&latest.next_selection, &incoming.previous_selection)
        || !latest.next_selection.is_empty()
        || !incoming.next_selection.is_empty()
        || !coalescable_edit_text(latest_edit)
        || !coalescable_edit_text(incoming_edit)
    {
        return None;
    }

    if !latest_edit.inserted_text.is_empty() {
        return coalesce_into_inserted_text(latest, incoming);
    }

    if !latest_edit.deleted_text.is_empty() && latest_edit.inserted_text.is_empty() {
        return coalesce_after_delete(latest, incoming);
    }

    None
}

fn coalesce_into_inserted_text(
    mut latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let latest_edit = latest.edits.first_mut()?;
    let incoming_edit = incoming.edits.first()?;
    let inserted_len = latest_edit.inserted_text.chars().count();
    let deleted_len = incoming_edit.deleted_text.chars().count();
    let inserted_start = latest_edit.start_char;
    let incoming_end = incoming_edit.start_char.checked_add(deleted_len)?;
    let inserted_end = inserted_start.checked_add(inserted_len)?;
    if incoming_edit.start_char < inserted_start || incoming_end > inserted_end {
        return None;
    }

    let relative_start = incoming_edit.start_char - inserted_start;
    let relative_end = relative_start + deleted_len;
    latest_edit.inserted_text = replace_char_range_in_text(
        &latest_edit.inserted_text,
        relative_start..relative_end,
        &incoming_edit.inserted_text,
    );
    latest.next_selection = incoming.next_selection;

    if latest_edit.deleted_text.is_empty() && latest_edit.inserted_text.is_empty() {
        return Some(CoalescedEdit::Noop);
    }

    Some(CoalescedEdit::Record(latest))
}

fn coalesce_after_delete(
    mut latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let latest_edit = latest.edits.first_mut()?;
    let incoming_edit = incoming.edits.first()?;
    let latest_start = latest_edit.start_char;
    let incoming_deleted_len = incoming_edit.deleted_text.chars().count();
    let merged = match (
        incoming_edit.deleted_text.is_empty(),
        incoming_edit.inserted_text.is_empty(),
    ) {
        (true, false) if incoming_edit.start_char == latest_start => {
            latest_edit.inserted_text = incoming_edit.inserted_text.clone();
            true
        }
        (false, true) if incoming_edit.start_char == latest_start => {
            latest_edit
                .deleted_text
                .push_str(&incoming_edit.deleted_text);
            latest_edit.deleted_spans.clear();
            true
        }
        (false, true) if incoming_edit.start_char + incoming_deleted_len == latest_start => {
            latest_edit.start_char = incoming_edit.start_char;
            latest_edit.deleted_text =
                format!("{}{}", incoming_edit.deleted_text, latest_edit.deleted_text);
            latest_edit.deleted_spans.clear();
            true
        }
        _ => false,
    };
    if !merged {
        return None;
    }
    latest.next_selection = incoming.next_selection;
    Some(CoalescedEdit::Record(latest))
}

fn replace_char_range_in_text(text: &str, range: Range<usize>, replacement: &str) -> String {
    let start = byte_index_for_char(text, range.start);
    let end = byte_index_for_char(text, range.end);
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.push_str(&text[..start]);
    result.push_str(replacement);
    result.push_str(&text[end..]);
    result
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_index)
        .unwrap_or(text.len())
}

fn is_single_line(text: &str) -> bool {
    !text.contains('\n') && !text.contains('\r')
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
