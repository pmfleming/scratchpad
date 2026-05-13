use super::super::{ByteSpan, PieceBuffer, PieceProvenance, PieceSource, PieceTreeLite};
use std::collections::HashMap;

impl PieceTreeLite {
    pub fn append_history_text(&mut self, text: &str, source: PieceSource) -> ByteSpan {
        let start = self.add.len();
        self.add.push_str(text);
        self.record_add_provenance(start, text.len(), source);
        add_byte_span(start, text.len())
    }

    pub fn compact_add_buffer(&mut self, history_spans: &mut [ByteSpan]) {
        if self.add.is_empty() {
            return;
        }

        let old_add = std::mem::take(&mut self.add);
        let mut new_add = String::with_capacity(old_add.len());
        let mut relocated = HashMap::<ByteSpan, ByteSpan>::new();
        let mut provenance_moves = Vec::<(ByteSpan, ByteSpan)>::new();

        self.relocate_visible_add_pieces(
            &old_add,
            &mut new_add,
            &mut relocated,
            &mut provenance_moves,
        );
        relocate_history_add_spans(
            history_spans,
            &old_add,
            &mut new_add,
            &relocated,
            &mut provenance_moves,
        );

        self.provenance.rewrite_add_spans(provenance_moves);
        self.add = new_add;
        self.root.recalculate();
    }

    pub fn provenance_for_span(&self, span: ByteSpan) -> PieceProvenance {
        self.provenance.provenance_for(span)
    }

    pub(super) fn record_add_provenance(
        &mut self,
        start_byte: usize,
        byte_len: usize,
        source: PieceSource,
    ) {
        self.provenance.record(
            add_byte_span(start_byte, byte_len),
            PieceProvenance {
                change_id: self.generation,
                source,
                session_generation: 0,
            },
        );
    }

    fn relocate_visible_add_pieces(
        &mut self,
        old_add: &str,
        new_add: &mut String,
        relocated: &mut HashMap<ByteSpan, ByteSpan>,
        provenance_moves: &mut Vec<(ByteSpan, ByteSpan)>,
    ) {
        for node in &mut self.root.nodes {
            for leaf in &mut node.leaves {
                for piece in &mut leaf.pieces {
                    if piece.buffer != PieceBuffer::Add || piece.byte_len == 0 {
                        continue;
                    }

                    let old_span = add_byte_span(piece.start_byte, piece.byte_len);
                    let new_span = move_add_text(old_add, new_add, old_span);
                    relocated.insert(old_span, new_span);
                    provenance_moves.push((old_span, new_span));
                    piece.start_byte = new_span.start_byte as usize;
                }
            }
        }
    }
}

fn relocate_history_add_spans(
    history_spans: &mut [ByteSpan],
    old_add: &str,
    new_add: &mut String,
    relocated: &HashMap<ByteSpan, ByteSpan>,
    provenance_moves: &mut Vec<(ByteSpan, ByteSpan)>,
) {
    for span in history_spans {
        if span.buffer != PieceBuffer::Add || span.byte_len == 0 {
            continue;
        }

        let old_span = *span;
        *span = relocated
            .get(span)
            .copied()
            .unwrap_or_else(|| move_add_text(old_add, new_add, old_span));
        provenance_moves.push((old_span, *span));
    }
}

fn move_add_text(old_add: &str, new_add: &mut String, old_span: ByteSpan) -> ByteSpan {
    let old_start = old_span.start_byte as usize;
    let old_end = old_start.saturating_add(old_span.byte_len as usize);
    let new_start = new_add.len();
    new_add.push_str(&old_add[old_start..old_end]);
    add_byte_span(new_start, old_span.byte_len as usize)
}

fn add_byte_span(start_byte: usize, byte_len: usize) -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: start_byte.min(u32::MAX as usize) as u32,
        byte_len: byte_len.min(u32::MAX as usize) as u32,
    }
}
