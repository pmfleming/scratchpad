use super::{
    PREVIEW_MAX_CHARS, PieceTreeLite, PieceTreeSlice, PieceTreeSpan, byte_range_for_char_range,
    compact_preview,
};
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

pub(super) fn previews_for_matches_in_contiguous_text(
    text: &str,
    ranges: &[Range<usize>],
) -> Vec<(usize, usize, String)> {
    let mut previews = Vec::with_capacity(ranges.len());
    let mut cursor = PreviewCursor::default();
    let mut cached_line_start_byte = None;
    let mut cached_preview = String::new();

    for range in ranges {
        cursor.advance_to(text, range.start);
        update_cached_line_preview(
            text,
            cursor.line_start_byte,
            &mut cached_line_start_byte,
            &mut cached_preview,
        );

        previews.push((
            cursor.line_number,
            range.start.saturating_sub(cursor.line_start_char) + 1,
            cached_preview.clone(),
        ));
    }

    previews
}

#[derive(Default)]
struct PreviewCursor {
    current_char: usize,
    current_byte: usize,
    line_number: usize,
    line_start_char: usize,
    line_start_byte: usize,
}

impl PreviewCursor {
    fn advance_to(&mut self, text: &str, target_char: usize) {
        if self.line_number == 0 {
            self.line_number = 1;
        }
        while self.current_char < target_char && self.current_byte < text.len() {
            let Some(ch) = text[self.current_byte..].chars().next() else {
                break;
            };
            self.advance_char(ch);
        }
    }

    fn advance_char(&mut self, ch: char) {
        let next_byte = self.current_byte + ch.len_utf8();
        if ch == '\n' {
            self.line_number += 1;
            self.line_start_char = self.current_char + 1;
            self.line_start_byte = next_byte;
        }
        self.current_char += 1;
        self.current_byte = next_byte;
    }
}

fn update_cached_line_preview(
    text: &str,
    line_start_byte: usize,
    cached_line_start_byte: &mut Option<usize>,
    cached_preview: &mut String,
) {
    if *cached_line_start_byte == Some(line_start_byte) {
        return;
    }

    let line_slice = match text[line_start_byte..].find('\n') {
        Some(relative_end) => &text[line_start_byte..line_start_byte + relative_end],
        None => &text[line_start_byte..],
    };
    let mut bounded = String::new();
    let mut chars = line_slice.chars();
    for _ in 0..PREVIEW_MAX_CHARS {
        let Some(ch) = chars.next() else {
            break;
        };
        bounded.push(ch);
    }
    *cached_preview = compact_preview(&bounded);
    if chars.next().is_some() && !cached_preview.ends_with("...") {
        cached_preview.push_str("...");
    }
    *cached_line_start_byte = Some(line_start_byte);
}
