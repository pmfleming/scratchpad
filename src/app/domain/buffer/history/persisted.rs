use super::{ByteSpan, PieceBuffer, PieceHistoryEdit, PieceHistoryFlags, PieceSource};
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

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
