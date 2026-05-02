use super::piece_tree::PieceBuffer;
use crate::app::ui::editor_content::native_editor::CursorRange;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub type PieceHistoryEdits = SmallVec<[PieceHistoryEdit; 1]>;

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
    pub seq: u64,
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
    pub seq: u64,
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

impl PieceHistoryEntry {
    pub fn is_undone(&self) -> bool {
        self.flags.undone
    }

    pub fn byte_cost(&self) -> usize {
        let edit_bytes = self
            .edits
            .iter()
            .map(|edit| match edit {
                PieceHistoryEdit::Inserted { span, .. } => span.byte_len as usize,
                PieceHistoryEdit::Deleted { spans, .. } => {
                    spans.iter().map(|span| span.byte_len as usize).sum()
                }
                PieceHistoryEdit::Replaced {
                    deleted, inserted, ..
                } => {
                    deleted
                        .iter()
                        .map(|span| span.byte_len as usize)
                        .sum::<usize>()
                        + inserted.byte_len as usize
                }
            })
            .sum::<usize>();
        std::mem::size_of::<Self>() + edit_bytes
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
