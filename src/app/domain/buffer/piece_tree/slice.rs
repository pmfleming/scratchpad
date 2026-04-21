use super::{PieceTreeLite, PieceTreeSlice, PieceTreeSpan, byte_range_for_char_range};
use std::ops::Range;

impl PieceTreeLite {
    pub fn spans_for_line(&self, target_line: usize) -> PieceTreeSlice<'_> {
        let line_info = self.line_info(target_line);
        self.spans_for_range(line_info.start_char..line_info.start_char + line_info.char_len)
    }

    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        let normalized = self.normalize_char_range(range_chars);
        if normalized.is_empty() || self.len_chars() == 0 {
            return PieceTreeSlice::empty(self, normalized);
        }

        let address = self.find_leaf_for_char_offset(normalized.start);
        PieceTreeSlice {
            tree: self,
            range_chars: normalized,
            node_index: address.node_index,
            leaf_index: address.leaf_index,
            piece_index: 0,
            current_char: address.leaf_start_char,
            finished: false,
        }
    }
}

impl<'a> PieceTreeSlice<'a> {
    fn empty(tree: &'a PieceTreeLite, range_chars: Range<usize>) -> Self {
        let current_char = range_chars.start;
        Self {
            tree,
            range_chars,
            node_index: 0,
            leaf_index: 0,
            piece_index: 0,
            current_char,
            finished: true,
        }
    }

    fn advance_piece_cursor(&mut self) {
        if self.finished || self.node_index >= self.tree.root.nodes.len() {
            self.finished = true;
            return;
        }

        let node = &self.tree.root.nodes[self.node_index];
        if self.leaf_index >= node.leaves.len() {
            self.node_index += 1;
            self.leaf_index = 0;
            self.piece_index = 0;
            if self.node_index >= self.tree.root.nodes.len() {
                self.finished = true;
            }
            return;
        }

        let leaf = &node.leaves[self.leaf_index];
        self.piece_index += 1;
        if self.piece_index >= leaf.pieces.len() {
            self.leaf_index += 1;
            self.piece_index = 0;
            if self.leaf_index >= node.leaves.len() {
                self.node_index += 1;
                self.leaf_index = 0;
                if self.node_index >= self.tree.root.nodes.len() {
                    self.finished = true;
                }
            }
        }
    }
}

impl<'a> Iterator for PieceTreeSlice<'a> {
    type Item = PieceTreeSpan<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.finished {
            let node = self.tree.root.nodes.get(self.node_index)?;
            let leaf = node.leaves.get(self.leaf_index)?;
            let piece = match leaf.pieces.get(self.piece_index) {
                Some(piece) => piece,
                None => {
                    self.advance_piece_cursor();
                    continue;
                }
            };

            let piece_start_char = self.current_char;
            let piece_end_char = piece_start_char + piece.char_len;
            self.current_char = piece_end_char;
            self.advance_piece_cursor();

            if piece_end_char <= self.range_chars.start {
                continue;
            }
            if piece_start_char >= self.range_chars.end {
                self.finished = true;
                return None;
            }

            let local_start = self.range_chars.start.saturating_sub(piece_start_char);
            let local_end = (self.range_chars.end.min(piece_end_char)) - piece_start_char;
            let text = self.tree.piece_text(piece);

            if local_start == 0 && local_end == piece.char_len {
                return Some(PieceTreeSpan {
                    text,
                    char_start: piece_start_char,
                    char_len: piece.char_len,
                });
            }

            let byte_range = if piece.is_ascii {
                local_start..local_end
            } else {
                byte_range_for_char_range(text, local_start, local_end)
            };
            return Some(PieceTreeSpan {
                text: &text[byte_range],
                char_start: piece_start_char + local_start,
                char_len: local_end - local_start,
            });
        }

        None
    }
}
