use super::super::piece_tree::PieceBuffer;
use std::collections::{HashMap, VecDeque};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PieceSource {
    #[default]
    Load,
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
    pub start_byte: u64,
    pub byte_len: u64,
}

impl ByteSpan {
    pub fn byte_end(self) -> u64 {
        self.start_byte.saturating_add(self.byte_len)
    }
}

pub(crate) const PIECE_PROVENANCE_ENTRY_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PieceProvenanceKey {
    buffer: PieceBuffer,
    start_byte: u64,
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
    byte_len: u64,
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

pub(crate) fn empty_byte_span() -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: 0,
        byte_len: 0,
    }
}
