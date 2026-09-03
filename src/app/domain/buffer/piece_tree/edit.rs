mod add_buffer;
mod rebalance;

use super::{
    ByteSpan, LeafAddress, Piece, PieceBuffer, PieceSource, PieceTreeInternalNode, PieceTreeLeaf,
    PieceTreeLite, PieceTreeText, build_chunked_pieces, byte_range_for_char_range,
    pack_pieces_into_leaves,
};
use std::ops::Range;

/// Proof that a structural edit has advanced the document generation and must
/// finish through `commit_leaf_replacement`, which refreshes derived indexes.
#[must_use]
pub(super) struct PieceTreeEdit {
    generation: u64,
}

impl PieceTreeEdit {
    fn generation(&self) -> u64 {
        self.generation
    }
}

impl PieceTreeLite {
    pub fn insert_with_source(
        &mut self,
        offset_chars: usize,
        text: &str,
        source: PieceSource,
    ) -> usize {
        assert!(offset_chars <= self.len_chars());
        if text.is_empty() {
            return 0;
        }
        let edit = self.begin_edit();

        let add_span = self
            .storage
            .append_add_text(text, source, edit.generation());
        let add_start = add_span.start_byte;
        let inserted_pieces = build_chunked_pieces(PieceBuffer::Add, add_start, text);
        let inserted_chars = inserted_pieces.iter().map(|piece| piece.char_len).sum();

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
        self.commit_leaf_replacement(edit, address, address, replacement_leaves);
        inserted_chars
    }

    pub fn remove_char_range(&mut self, range_chars: Range<usize>) {
        assert!(range_chars.start <= range_chars.end);
        assert!(range_chars.end <= self.len_chars());
        if range_chars.is_empty() {
            return;
        }
        let edit = self.begin_edit();

        let start_address = self.find_leaf_for_char_offset(range_chars.start);
        let end_probe = range_chars.end.saturating_sub(1);
        let end_address = self.find_leaf_for_char_offset(end_probe);
        let anchors =
            self.reposition_removed_span_anchors(start_address, end_address, &range_chars);
        let affected_pieces =
            self.retained_pieces_for_removal(start_address, end_address, range_chars);
        let mut replacement_leaves = pack_pieces_into_leaves(affected_pieces);
        self.redistribute_anchors_into_leaves(&mut replacement_leaves, anchors);
        self.commit_leaf_replacement(edit, start_address, end_address, replacement_leaves);
    }

    #[must_use]
    pub fn text_for_span(&self, span: ByteSpan) -> PieceTreeText<'_> {
        self.storage.text_for_span(span)
    }

    fn begin_edit(&mut self) -> PieceTreeEdit {
        PieceTreeEdit {
            generation: self.runtime.advance_generation(),
        }
    }

    fn leaf_with_inserted_pieces(
        &self,
        leaf: &PieceTreeLeaf,
        offset_in_leaf_chars: usize,
        inserted_pieces: Vec<Piece>,
    ) -> Vec<Piece> {
        let mut result = Vec::with_capacity(leaf.pieces.len() + inserted_pieces.len() + 2);
        let boundary = leaf
            .piece_start_chars
            .partition_point(|start| *start < offset_in_leaf_chars);

        if let Some(piece_index) = boundary.checked_sub(1)
            && let Some(piece) = leaf.pieces.get(piece_index)
        {
            let inner_offset = offset_in_leaf_chars - leaf.piece_start_chars[piece_index];
            if inner_offset < piece.char_len {
                result.extend_from_slice(&leaf.pieces[..piece_index]);
                result.push(self.slice_piece_by_chars(piece, 0, inner_offset));
                result.extend(inserted_pieces);
                result.push(self.slice_piece_by_chars(
                    piece,
                    inner_offset,
                    piece.char_len - inner_offset,
                ));
                result.extend_from_slice(&leaf.pieces[piece_index + 1..]);
                return result;
            }
        }

        result.extend_from_slice(&leaf.pieces[..boundary]);
        result.extend(inserted_pieces);
        result.extend_from_slice(&leaf.pieces[boundary..]);
        result
    }

    fn slice_piece_by_chars(&self, piece: &Piece, start_char: usize, char_len: usize) -> Piece {
        let text = self.piece_text(piece);
        let byte_range = if piece.is_ascii {
            start_char..(start_char + char_len)
        } else {
            byte_range_for_char_range(&text, start_char, start_char + char_len)
        };
        Piece::from_slice(
            piece.buffer,
            piece.start_byte + byte_range.start,
            &text[byte_range],
        )
    }

    pub(super) fn piece_text<'a>(&'a self, piece: &Piece) -> PieceTreeText<'a> {
        self.storage
            .piece_text(piece.buffer, piece.start_byte, piece.byte_len)
    }

    pub(super) fn find_leaf_for_char_offset(&self, offset_chars: usize) -> LeafAddress {
        if self.root.nodes.is_empty() || self.len_chars() == 0 {
            return LeafAddress::default();
        }
        self.find_leaf_by(offset_chars, |node| &node.leaf_start_chars)
    }

    pub(super) fn find_leaf_for_line_index(&self, line_index: usize) -> LeafAddress {
        if self.root.nodes.is_empty() || self.len_chars() == 0 {
            return LeafAddress::default();
        }

        let safe_line = line_index.min(self.root.metrics.newlines);
        let node_index = self.root.node_metric_index.node_for_line(safe_line);
        let node = &self.root.nodes[node_index];
        let node_start_newline = self.root.node_metric_index.newlines_before(node_index);
        let leaf_index = node
            .leaves
            .iter()
            .enumerate()
            .find_map(|(index, leaf)| {
                let start = node_start_newline + node.leaf_start_newlines[index];
                (start + leaf.metrics.newlines >= safe_line).then_some(index)
            })
            .unwrap_or_else(|| node.leaves.len() - 1);

        LeafAddress {
            node_index,
            leaf_index,
            leaf_start_char: self.root.node_metric_index.chars_before(node_index)
                + node.leaf_start_chars[leaf_index],
            leaf_start_newline: node_start_newline + node.leaf_start_newlines[leaf_index],
        }
    }

    fn find_leaf_by(
        &self,
        target: usize,
        leaf_starts: impl Fn(&PieceTreeInternalNode) -> &[usize],
    ) -> LeafAddress {
        let node_index = self.root.node_metric_index.node_for_char(target);
        let node = &self.root.nodes[node_index];
        let node_start = self.root.node_metric_index.chars_before(node_index);
        let offset_in_node = target.saturating_sub(node_start);
        let leaf_starts_slice = leaf_starts(node);
        let leaf_index = leaf_starts_slice
            .partition_point(|start| *start <= offset_in_node)
            .saturating_sub(1)
            .min(node.leaves.len() - 1);

        LeafAddress {
            node_index,
            leaf_index,
            leaf_start_char: node_start + node.leaf_start_chars[leaf_index],
            leaf_start_newline: self.root.node_metric_index.newlines_before(node_index)
                + node.leaf_start_newlines[leaf_index],
        }
    }

    fn retained_pieces_for_removal(
        &self,
        start_address: LeafAddress,
        end_address: LeafAddress,
        range_chars: Range<usize>,
    ) -> Vec<Piece> {
        let mut retained = Vec::new();
        let mut piece_start_char = start_address.leaf_start_char;

        for node_index in start_address.node_index..=end_address.node_index {
            for leaf in self.affected_leaves_in_node(node_index, start_address, end_address) {
                for piece in &leaf.pieces {
                    self.append_retained_piece(
                        piece,
                        piece_start_char,
                        &range_chars,
                        &mut retained,
                    );
                    piece_start_char += piece.char_len;
                }
            }
        }
        retained
    }

    fn affected_leaves_in_node(
        &self,
        node_index: usize,
        start: LeafAddress,
        end: LeafAddress,
    ) -> &[PieceTreeLeaf] {
        let node = &self.root.nodes[node_index];
        let first = if node_index == start.node_index {
            start.leaf_index
        } else {
            0
        };
        let last = if node_index == end.node_index {
            end.leaf_index
        } else {
            node.leaves.len() - 1
        };
        &node.leaves[first..=last]
    }

    fn append_retained_piece(
        &self,
        piece: &Piece,
        piece_start: usize,
        removed: &Range<usize>,
        retained: &mut Vec<Piece>,
    ) {
        let piece_end = piece_start + piece.char_len;
        if removed.end <= piece_start || removed.start >= piece_end {
            retained.push(*piece);
            return;
        }

        let left_chars = removed.start.saturating_sub(piece_start);
        if left_chars > 0 {
            retained.push(self.slice_piece_by_chars(piece, 0, left_chars));
        }
        let right_start = removed.end.saturating_sub(piece_start).min(piece.char_len);
        if right_start < piece.char_len {
            retained.push(self.slice_piece_by_chars(
                piece,
                right_start,
                piece.char_len - right_start,
            ));
        }
    }
}
