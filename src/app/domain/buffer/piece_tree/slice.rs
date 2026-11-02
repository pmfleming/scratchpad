use super::{
    ByteSpan, Piece, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLite, PieceTreeSpan,
    byte_range_for_char_range,
};
use std::ops::Range;

mod previews;
pub(super) use previews::{
    previews_for_matches_in_contiguous_text, previews_for_matches_in_piece_spans,
};

// Both byte and character spans walk the same pieces, without buffering text.
pub(super) struct Pieces<'a> {
    nodes: std::slice::Iter<'a, PieceTreeInternalNode>,
    leaves: std::slice::Iter<'a, PieceTreeLeaf>,
    pieces: std::slice::Iter<'a, Piece>,
}

impl<'a> Pieces<'a> {
    pub(super) fn new(tree: &'a PieceTreeLite, node: usize, leaf: usize) -> Self {
        let mut nodes = tree.root.nodes[node..].iter();
        let leaves = nodes.next().map_or(&[][..], |node| &node.leaves[leaf..]);
        Self {
            nodes,
            leaves: leaves.iter(),
            pieces: [].iter(),
        }
    }
}

impl<'a> Iterator for Pieces<'a> {
    type Item = &'a Piece;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(piece) = self.pieces.next() {
                return Some(piece);
            }
            if let Some(leaf) = self.leaves.next() {
                self.pieces = leaf.pieces.iter();
            } else {
                self.leaves = self.nodes.next()?.leaves.iter();
            }
        }
    }
}

pub struct PieceTreeSlice<'a> {
    tree: &'a PieceTreeLite,
    range_chars: Range<usize>,
    pieces: Pieces<'a>,
    current_char: usize,
}

impl PieceTreeLite {
    #[must_use]
    pub fn spans_for_line(&self, target_line: usize) -> PieceTreeSlice<'_> {
        let line = self.line_info(target_line);
        self.spans_for_range(line.start_char..line.start_char + line.char_len)
    }

    #[must_use]
    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        let range_chars = self.normalize_char_range(range_chars);
        let address = self.find_leaf_for_char_offset(range_chars.start);
        PieceTreeSlice {
            tree: self,
            range_chars,
            pieces: Pieces::new(self, address.node_index, address.leaf_index),
            current_char: address.leaf_start_char,
        }
    }
}

impl<'a> Iterator for PieceTreeSlice<'a> {
    type Item = PieceTreeSpan<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.range_chars.is_empty() && self.current_char < self.range_chars.end {
            let piece = self.pieces.next()?;
            let piece_start_char = self.current_char;
            let piece_end_char = piece_start_char + piece.char_len;
            self.current_char = piece_end_char;

            if piece_end_char <= self.range_chars.start {
                continue;
            }

            let local_start = self.range_chars.start.saturating_sub(piece_start_char);
            let local_end = (self.range_chars.end.min(piece_end_char)) - piece_start_char;
            let text = self.tree.piece_text(piece);

            if local_start == 0 && local_end == piece.char_len {
                return Some(PieceTreeSpan {
                    text,
                    char_start: piece_start_char,
                    char_len: piece.char_len,
                    byte_span: ByteSpan {
                        buffer: piece.buffer,
                        start_byte: piece.start_byte as u64,
                        byte_len: piece.byte_len as u64,
                    },
                });
            }

            let byte_range = if piece.is_ascii {
                local_start..local_end
            } else {
                byte_range_for_char_range(&text, local_start, local_end)
            };
            let start_byte = piece.start_byte + byte_range.start;
            let byte_len = byte_range.len();
            return Some(PieceTreeSpan {
                text: text.slice(byte_range),
                char_start: piece_start_char + local_start,
                char_len: local_end - local_start,
                byte_span: ByteSpan {
                    buffer: piece.buffer,
                    start_byte: start_byte as u64,
                    byte_len: byte_len as u64,
                },
            });
        }

        None
    }
}
