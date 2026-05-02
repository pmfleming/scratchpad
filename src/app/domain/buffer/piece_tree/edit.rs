use super::{
    ByteSpan, LeafAddress, Piece, PieceBuffer, PieceProvenance, PieceSource, PieceTreeInternalNode,
    PieceTreeLeaf, PieceTreeLite, build_chunked_pieces, byte_range_for_char_range,
    pack_pieces_into_leaves, support::pack_leaves_into_nodes,
};
use std::ops::Range;

impl PieceTreeLite {
    pub fn insert(&mut self, offset_chars: usize, text: &str) {
        self.insert_with_source(offset_chars, text, PieceSource::Edit);
    }

    pub fn insert_with_source(&mut self, offset_chars: usize, text: &str, source: PieceSource) {
        assert!(offset_chars <= self.len_chars());
        if text.is_empty() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let inserted_chars = text.chars().count();

        let add_start = self.add.len();
        self.add.push_str(text);
        self.record_add_provenance(add_start, text.len(), source);
        let inserted_pieces = build_chunked_pieces(PieceBuffer::Add, add_start, text);

        let address = self.find_leaf_for_char_offset(offset_chars);
        let replacement = {
            let leaf = &self.root.nodes[address.node_index].leaves[address.leaf_index];
            self.leaf_with_inserted_pieces(
                leaf,
                offset_chars.saturating_sub(address.leaf_start_char),
                inserted_pieces,
            )
        };
        let mut replacement_leaves = pack_pieces_into_leaves(replacement);
        let anchors = self.reposition_inserted_leaf_anchors(address, offset_chars, inserted_chars);
        self.redistribute_anchors_into_leaves(&mut replacement_leaves, anchors);
        self.replace_leaf_span(address, address, replacement_leaves);
    }

    pub fn remove_range(&mut self, range_chars: Range<usize>) {
        self.remove_char_range(range_chars);
    }

    pub fn remove_char_range(&mut self, range_chars: Range<usize>) {
        assert!(range_chars.start <= range_chars.end);
        assert!(range_chars.end <= self.len_chars());
        if range_chars.is_empty() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);

        let start_address = self.find_leaf_for_char_offset(range_chars.start);
        let end_probe = range_chars.end.saturating_sub(1);
        let end_address = self.find_leaf_for_char_offset(end_probe);
        let anchors =
            self.reposition_removed_span_anchors(start_address, end_address, &range_chars);
        let affected_pieces =
            self.retained_pieces_for_removal(start_address, end_address, range_chars);
        let mut replacement_leaves = pack_pieces_into_leaves(affected_pieces);
        self.redistribute_anchors_into_leaves(&mut replacement_leaves, anchors);
        self.replace_leaf_span(start_address, end_address, replacement_leaves);
    }

    pub fn add_buffer_len(&self) -> usize {
        self.add.len()
    }

    pub fn append_history_text(&mut self, text: &str, source: PieceSource) -> ByteSpan {
        let start = self.add.len();
        self.add.push_str(text);
        self.record_add_provenance(start, text.len(), source);
        ByteSpan {
            buffer: PieceBuffer::Add,
            start_byte: start.min(u32::MAX as usize) as u32,
            byte_len: text.len().min(u32::MAX as usize) as u32,
        }
    }

    pub fn text_for_span(&self, span: ByteSpan) -> &str {
        let start = span.start_byte as usize;
        let end = start.saturating_add(span.byte_len as usize);
        match span.buffer {
            PieceBuffer::Original => &self.original[start..end],
            PieceBuffer::Add => &self.add[start..end],
        }
    }

    pub fn compact_add_buffer(&mut self, history_spans: &mut [ByteSpan]) {
        if self.add.is_empty() {
            return;
        }

        let old_add = std::mem::take(&mut self.add);
        let mut new_add = String::with_capacity(old_add.len());
        let mut relocated = std::collections::HashMap::<ByteSpan, ByteSpan>::new();

        for node in &mut self.root.nodes {
            for leaf in &mut node.leaves {
                for piece in &mut leaf.pieces {
                    if piece.buffer != PieceBuffer::Add || piece.byte_len == 0 {
                        continue;
                    }
                    let old_start = piece.start_byte;
                    let old_end = old_start.saturating_add(piece.byte_len);
                    let text = &old_add[old_start..old_end];
                    let new_start = new_add.len();
                    new_add.push_str(text);
                    let old_span = ByteSpan {
                        buffer: PieceBuffer::Add,
                        start_byte: old_start.min(u32::MAX as usize) as u32,
                        byte_len: piece.byte_len.min(u32::MAX as usize) as u32,
                    };
                    let new_span = ByteSpan {
                        buffer: PieceBuffer::Add,
                        start_byte: new_start.min(u32::MAX as usize) as u32,
                        byte_len: piece.byte_len.min(u32::MAX as usize) as u32,
                    };
                    relocated.insert(old_span, new_span);
                    piece.start_byte = new_start;
                }
            }
        }

        for span in history_spans {
            if span.buffer != PieceBuffer::Add || span.byte_len == 0 {
                continue;
            }
            if let Some(new_span) = relocated.get(span) {
                *span = *new_span;
                continue;
            }
            let old_start = span.start_byte as usize;
            let old_end = old_start.saturating_add(span.byte_len as usize);
            let text = &old_add[old_start..old_end];
            let new_start = new_add.len();
            new_add.push_str(text);
            span.start_byte = new_start.min(u32::MAX as usize) as u32;
        }

        self.add = new_add;
        self.root.recalculate();
    }

    pub fn provenance_for_span(&self, span: ByteSpan) -> PieceProvenance {
        self.provenance.provenance_for(span)
    }

    fn record_add_provenance(&mut self, start_byte: usize, byte_len: usize, source: PieceSource) {
        self.provenance.record(
            ByteSpan {
                buffer: PieceBuffer::Add,
                start_byte: start_byte.min(u32::MAX as usize) as u32,
                byte_len: byte_len.min(u32::MAX as usize) as u32,
            },
            PieceProvenance {
                change_id: self.generation,
                source,
                session_generation: 0,
            },
        );
    }

    fn leaf_with_inserted_pieces(
        &self,
        leaf: &PieceTreeLeaf,
        offset_in_leaf_chars: usize,
        inserted_pieces: Vec<Piece>,
    ) -> Vec<Piece> {
        let mut result = Vec::with_capacity(leaf.pieces.len() + inserted_pieces.len() + 2);
        let mut current_char = 0usize;
        let mut inserted = Some(inserted_pieces);

        for piece in &leaf.pieces {
            let piece_end_char = current_char + piece.char_len;
            if let Some(new_pieces) = inserted.take() {
                if offset_in_leaf_chars <= current_char {
                    result.extend(new_pieces);
                } else if offset_in_leaf_chars < piece_end_char {
                    let inner_offset = offset_in_leaf_chars - current_char;
                    if inner_offset > 0 {
                        result.push(self.slice_piece_by_chars(piece, 0, inner_offset));
                    }
                    result.extend(new_pieces);
                    if inner_offset < piece.char_len {
                        result.push(self.slice_piece_by_chars(
                            piece,
                            inner_offset,
                            piece.char_len - inner_offset,
                        ));
                    }
                    current_char = piece_end_char;
                    continue;
                } else {
                    inserted = Some(new_pieces);
                }
            }

            result.push(piece.clone());
            current_char = piece_end_char;
        }

        if let Some(new_pieces) = inserted {
            result.extend(new_pieces);
        }

        result
    }

    fn slice_piece_by_chars(&self, piece: &Piece, start_char: usize, char_len: usize) -> Piece {
        let text = self.piece_text(piece);
        let byte_range = if piece.is_ascii {
            start_char..(start_char + char_len)
        } else {
            byte_range_for_char_range(text, start_char, start_char + char_len)
        };
        Piece::from_slice(
            piece.buffer,
            piece.start_byte + byte_range.start,
            &text[byte_range],
        )
    }

    pub(super) fn piece_text<'a>(&'a self, piece: &Piece) -> &'a str {
        let end = piece.start_byte + piece.byte_len;
        match piece.buffer {
            PieceBuffer::Original => &self.original[piece.start_byte..end],
            PieceBuffer::Add => &self.add[piece.start_byte..end],
        }
    }

    pub(super) fn find_leaf_for_char_offset(&self, offset_chars: usize) -> LeafAddress {
        if self.root.nodes.is_empty() || self.len_chars() == 0 {
            return LeafAddress::default();
        }
        self.find_leaf_by(offset_chars, &self.root.node_start_chars, |node| {
            &node.leaf_start_chars
        })
    }

    fn find_leaf_by(
        &self,
        target: usize,
        node_starts: &[usize],
        leaf_starts: impl Fn(&PieceTreeInternalNode) -> &[usize],
    ) -> LeafAddress {
        let node_index = node_starts
            .partition_point(|start| *start <= target)
            .saturating_sub(1)
            .min(self.root.nodes.len() - 1);
        let node = &self.root.nodes[node_index];
        let offset_in_node = target.saturating_sub(node_starts[node_index]);
        let leaf_starts_slice = leaf_starts(node);
        let leaf_index = leaf_starts_slice
            .partition_point(|start| *start <= offset_in_node)
            .saturating_sub(1)
            .min(node.leaves.len() - 1);

        LeafAddress {
            node_index,
            leaf_index,
            leaf_start_char: self.root.node_start_chars[node_index]
                + node.leaf_start_chars[leaf_index],
            leaf_start_newline: self.root.node_start_newlines[node_index]
                + node.leaf_start_newlines[leaf_index],
        }
    }

    fn retained_pieces_for_removal(
        &self,
        start_address: LeafAddress,
        end_address: LeafAddress,
        range_chars: Range<usize>,
    ) -> Vec<Piece> {
        let mut affected_pieces = Vec::new();
        let mut current_char = start_address.leaf_start_char;

        for node_index in start_address.node_index..=end_address.node_index {
            let node = &self.root.nodes[node_index];
            let leaf_start = if node_index == start_address.node_index {
                start_address.leaf_index
            } else {
                0
            };
            let leaf_end = if node_index == end_address.node_index {
                end_address.leaf_index
            } else {
                node.leaves.len() - 1
            };

            for leaf in &node.leaves[leaf_start..=leaf_end] {
                for piece in &leaf.pieces {
                    let piece_start_char = current_char;
                    let piece_end_char = current_char + piece.char_len;

                    if range_chars.end <= piece_start_char || range_chars.start >= piece_end_char {
                        affected_pieces.push(piece.clone());
                    } else {
                        let left_chars = range_chars.start.saturating_sub(piece_start_char);
                        if left_chars > 0 {
                            affected_pieces.push(self.slice_piece_by_chars(piece, 0, left_chars));
                        }

                        let right_start_char = range_chars
                            .end
                            .saturating_sub(piece_start_char)
                            .min(piece.char_len);
                        if right_start_char < piece.char_len {
                            affected_pieces.push(self.slice_piece_by_chars(
                                piece,
                                right_start_char,
                                piece.char_len - right_start_char,
                            ));
                        }
                    }

                    current_char = piece_end_char;
                }
            }
        }

        affected_pieces
    }

    fn replace_leaf_span(
        &mut self,
        start: LeafAddress,
        end: LeafAddress,
        replacement_leaves: Vec<PieceTreeLeaf>,
    ) {
        let mut combined_leaves = Vec::new();
        combined_leaves.extend(
            self.root.nodes[start.node_index].leaves[..start.leaf_index]
                .iter()
                .cloned(),
        );
        combined_leaves.extend(replacement_leaves);
        combined_leaves.extend(
            self.root.nodes[end.node_index].leaves[end.leaf_index + 1..]
                .iter()
                .cloned(),
        );

        let replacement_nodes = pack_leaves_into_nodes(combined_leaves);
        let inserted_nodes = replacement_nodes.len();
        self.root
            .nodes
            .splice(start.node_index..=end.node_index, replacement_nodes);
        self.rebalance_node_window(start.node_index, inserted_nodes);
    }

    fn rebalance_node_window(&mut self, inserted_at: usize, inserted_nodes: usize) {
        if self.root.nodes.is_empty() {
            self.root.recalculate();
            return;
        }

        let mut window_start = inserted_at.saturating_sub(1);
        let mut window_end = (inserted_at + inserted_nodes + 1).min(self.root.nodes.len());

        if window_start > 0
            && self.root.nodes[window_start].leaves.len() < super::MIN_LEAVES_PER_INTERNAL
        {
            window_start -= 1;
        }
        if window_end < self.root.nodes.len()
            && self.root.nodes[window_end - 1].leaves.len() < super::MIN_LEAVES_PER_INTERNAL
        {
            window_end = (window_end + 1).min(self.root.nodes.len());
        }

        let mut window_leaves = Vec::new();
        for node in &self.root.nodes[window_start..window_end] {
            window_leaves.extend(node.leaves.iter().cloned());
        }

        let rebalanced_nodes = pack_leaves_into_nodes(window_leaves);
        self.root
            .nodes
            .splice(window_start..window_end, rebalanced_nodes);
        self.root.recalculate();
    }
}
