use super::{ByteSpan, PieceBuffer, PieceProvenance, PieceSource};
use crate::app::domain::buffer::history::PieceProvenanceStore;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct PieceTreeStorage {
    original: Arc<str>,
    add: String,
    provenance: PieceProvenanceStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AddTextSpan {
    pub(super) start_byte: usize,
    pub(super) byte_len: usize,
}

impl AddTextSpan {
    pub(super) fn byte_span(self) -> ByteSpan {
        add_byte_span(self.start_byte, self.byte_len)
    }
}

impl PieceTreeStorage {
    pub(super) fn from_original(text: String) -> Self {
        Self {
            original: Arc::from(text.into_boxed_str()),
            add: String::new(),
            provenance: PieceProvenanceStore::default(),
        }
    }

    pub(super) fn original_text(&self) -> &str {
        &self.original
    }

    pub(super) fn original_len(&self) -> usize {
        self.original.len()
    }

    #[cfg(test)]
    pub(super) fn shares_original_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.original, &other.original)
    }

    pub(super) fn add_is_empty(&self) -> bool {
        self.add.is_empty()
    }

    pub(super) fn text_for_span(&self, span: ByteSpan) -> &str {
        let start = span.start_byte as usize;
        let end = start.saturating_add(span.byte_len as usize);
        match span.buffer {
            PieceBuffer::Original => &self.original[start..end],
            PieceBuffer::Add => &self.add[start..end],
        }
    }

    pub(super) fn piece_text(
        &self,
        buffer: PieceBuffer,
        start_byte: usize,
        byte_len: usize,
    ) -> &str {
        let end = start_byte + byte_len;
        match buffer {
            PieceBuffer::Original => &self.original[start_byte..end],
            PieceBuffer::Add => &self.add[start_byte..end],
        }
    }

    pub(super) fn append_add_text(
        &mut self,
        text: &str,
        source: PieceSource,
        generation: u64,
    ) -> AddTextSpan {
        let span = AddTextSpan {
            start_byte: self.add.len(),
            byte_len: text.len(),
        };
        self.add.push_str(text);
        self.record_add_provenance(span, source, generation);
        span
    }

    pub(super) fn take_add_if_nonempty(&mut self) -> Option<String> {
        (!self.add.is_empty()).then(|| std::mem::take(&mut self.add))
    }

    pub(super) fn replace_add(&mut self, add: String) {
        self.add = add;
    }

    pub(super) fn provenance_entry_count(&self) -> usize {
        self.provenance.len()
    }

    pub(super) fn provenance_for_span(&self, span: ByteSpan) -> PieceProvenance {
        self.provenance.provenance_for(span)
    }

    pub(super) fn rewrite_add_spans(&mut self, moves: Vec<(ByteSpan, ByteSpan)>) {
        self.provenance.rewrite_add_spans(moves);
    }

    fn record_add_provenance(&mut self, span: AddTextSpan, source: PieceSource, generation: u64) {
        self.provenance.record(
            span.byte_span(),
            PieceProvenance {
                change_id: generation,
                source,
                session_generation: 0,
            },
        );
    }
}

pub(super) fn add_byte_span(start_byte: usize, byte_len: usize) -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: start_byte.min(u32::MAX as usize) as u32,
        byte_len: byte_len.min(u32::MAX as usize) as u32,
    }
}
