mod edit;
mod slice;
mod support;

use std::ops::Range;

use support::{
    build_chunked_pieces, build_root_from_pieces, byte_index_for_char_offset,
    byte_range_for_char_range, compact_preview, count_newlines, pack_pieces_into_leaves,
    recalculate_prefix_metrics,
};

const MAX_LEAF_BYTES: usize = 256 * 1024;
const MAX_LEAF_PIECES: usize = 16;
const MAX_LEAVES_PER_INTERNAL: usize = 16;
const MIN_LEAVES_PER_INTERNAL: usize = MAX_LEAVES_PER_INTERNAL / 4;
const PREVIEW_MAX_CHARS: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceTreeCharPosition {
    pub offset_chars: usize,
    pub line_index: usize,
    pub column_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceTreeLineInfo {
    pub line_index: usize,
    pub start_char: usize,
    pub char_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceTreeSpan<'a> {
    pub text: &'a str,
    pub char_start: usize,
    pub char_len: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PieceTreeMetrics {
    pub bytes: usize,
    pub chars: usize,
    pub newlines: usize,
    pub pieces: usize,
}

impl PieceTreeMetrics {
    fn add_assign(&mut self, other: Self) {
        self.bytes += other.bytes;
        self.chars += other.chars;
        self.newlines += other.newlines;
        self.pieces += other.pieces;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieceBuffer {
    Original,
    Add,
}

#[derive(Clone, Debug)]
struct Piece {
    buffer: PieceBuffer,
    start_byte: usize,
    byte_len: usize,
    char_len: usize,
    newline_count: usize,
    is_ascii: bool,
}

impl Piece {
    fn from_slice(buffer: PieceBuffer, start_byte: usize, text: &str) -> Self {
        Self {
            buffer,
            start_byte,
            byte_len: text.len(),
            char_len: text.chars().count(),
            newline_count: count_newlines(text),
            is_ascii: text.is_ascii(),
        }
    }

    fn metrics(&self) -> PieceTreeMetrics {
        PieceTreeMetrics {
            bytes: self.byte_len,
            chars: self.char_len,
            newlines: self.newline_count,
            pieces: 1,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeLeaf {
    pieces: Vec<Piece>,
    metrics: PieceTreeMetrics,
    piece_start_chars: Vec<usize>,
    piece_start_newlines: Vec<usize>,
}

impl PieceTreeLeaf {
    fn push_piece(&mut self, piece: Piece) {
        if piece.byte_len == 0 {
            return;
        }

        if let Some(last) = self.pieces.last_mut()
            && last.buffer == piece.buffer
            && last.start_byte + last.byte_len == piece.start_byte
        {
            last.byte_len += piece.byte_len;
            last.char_len += piece.char_len;
            last.newline_count += piece.newline_count;
        } else {
            self.pieces.push(piece);
        }

        self.recalculate();
    }

    fn recalculate(&mut self) {
        self.metrics = recalculate_prefix_metrics(
            &self.pieces,
            &mut self.piece_start_chars,
            &mut self.piece_start_newlines,
            Piece::metrics,
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeInternalNode {
    leaves: Vec<PieceTreeLeaf>,
    metrics: PieceTreeMetrics,
    leaf_start_chars: Vec<usize>,
    leaf_start_newlines: Vec<usize>,
}

impl PieceTreeInternalNode {
    fn recalculate(&mut self) {
        if self.leaves.is_empty() {
            self.leaves.push(PieceTreeLeaf::default());
        }

        for leaf in &mut self.leaves {
            leaf.recalculate();
        }
        self.metrics = recalculate_prefix_metrics(
            &self.leaves,
            &mut self.leaf_start_chars,
            &mut self.leaf_start_newlines,
            |leaf| leaf.metrics,
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeRoot {
    nodes: Vec<PieceTreeInternalNode>,
    metrics: PieceTreeMetrics,
    node_start_chars: Vec<usize>,
    node_start_newlines: Vec<usize>,
}

impl PieceTreeRoot {
    fn recalculate(&mut self) {
        if self.nodes.is_empty() {
            self.nodes.push(PieceTreeInternalNode::default());
        }

        for node in &mut self.nodes {
            node.recalculate();
        }
        self.metrics = recalculate_prefix_metrics(
            &self.nodes,
            &mut self.node_start_chars,
            &mut self.node_start_newlines,
            |node| node.metrics,
        );
    }
}

#[derive(Clone, Debug)]
pub struct PieceTreeLite {
    original: String,
    add: String,
    root: PieceTreeRoot,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct LeafAddress {
    node_index: usize,
    leaf_index: usize,
    leaf_start_char: usize,
    leaf_start_newline: usize,
}

pub struct PieceTreeSlice<'a> {
    tree: &'a PieceTreeLite,
    range_chars: Range<usize>,
    node_index: usize,
    leaf_index: usize,
    piece_index: usize,
    current_char: usize,
    finished: bool,
}

impl PieceTreeLite {
    pub fn from_string(text: String) -> Self {
        let pieces = build_chunked_pieces(PieceBuffer::Original, 0, &text);
        Self {
            original: text,
            add: String::new(),
            root: build_root_from_pieces(pieces),
            generation: 0,
        }
    }

    pub fn metrics(&self) -> PieceTreeMetrics {
        self.root.metrics
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len_bytes(&self) -> usize {
        self.root.metrics.bytes
    }

    pub fn len_chars(&self) -> usize {
        self.root.metrics.chars
    }

    pub fn normalize_char_range(&self, range_chars: Range<usize>) -> Range<usize> {
        let start = range_chars.start.min(self.len_chars());
        let end = range_chars.end.min(self.len_chars());
        if start <= end { start..end } else { end..start }
    }

    pub fn char_position(&self, offset_chars: usize) -> PieceTreeCharPosition {
        let safe_offset = offset_chars.min(self.len_chars());
        let line_index = self.line_index_at_offset(safe_offset);
        let line_info = self.line_info(line_index);
        PieceTreeCharPosition {
            offset_chars: safe_offset,
            line_index,
            column_index: safe_offset.saturating_sub(line_info.start_char),
        }
    }

    pub fn line_info(&self, target_line: usize) -> PieceTreeLineInfo {
        let (start_char, char_len) = self.line_lookup(target_line);
        PieceTreeLineInfo {
            line_index: target_line.min(self.root.metrics.newlines),
            start_char,
            char_len,
        }
    }

    pub fn preview_for_match(&self, range_chars: &Range<usize>) -> (usize, usize, String) {
        let normalized = self.normalize_char_range(range_chars.clone());
        let line_index = self.line_index_at_offset(normalized.start);
        let info = self.line_info(line_index);
        let column = normalized.start.saturating_sub(info.start_char);
        let (line_text, truncated) = self.extract_range_bounded(
            info.start_char..info.start_char + info.char_len,
            PREVIEW_MAX_CHARS,
        );
        let mut preview = compact_preview(&line_text);
        if truncated && !preview.ends_with("...") {
            preview.push_str("...");
        }
        (line_index + 1, column + 1, preview)
    }

    pub fn line_lookup(&self, target_line: usize) -> (usize, usize) {
        if self.len_chars() == 0 {
            return (0, 0);
        }

        let safe_line = target_line.min(self.root.metrics.newlines);
        let address = self.find_leaf_for_line(safe_line);
        let mut current_line = address.leaf_start_newline;
        let mut line_start = address.leaf_start_char;
        let mut current_char = line_start;
        let mut current_len = 0usize;
        let mut is_first_leaf = true;

        for (node_index, node) in self.root.nodes.iter().enumerate().skip(address.node_index) {
            let leaf_start = if node_index == address.node_index {
                address.leaf_index
            } else {
                0
            };

            for leaf in node.leaves.iter().skip(leaf_start) {
                let piece_skip = if is_first_leaf {
                    is_first_leaf = false;
                    let offset_in_leaf = safe_line.saturating_sub(current_line);
                    if offset_in_leaf > 0 && !leaf.piece_start_newlines.is_empty() {
                        let pi = leaf
                            .piece_start_newlines
                            .partition_point(|&n| n < offset_in_leaf)
                            .saturating_sub(1);
                        current_line += leaf.piece_start_newlines[pi];
                        current_char += leaf.piece_start_chars[pi];
                        pi
                    } else {
                        0
                    }
                } else {
                    0
                };

                for piece in leaf.pieces.iter().skip(piece_skip) {
                    if current_line < safe_line && current_line + piece.newline_count < safe_line {
                        current_line += piece.newline_count;
                        current_char += piece.char_len;
                        continue;
                    }
                    if current_line == safe_line && piece.newline_count == 0 {
                        current_len += piece.char_len;
                        current_char += piece.char_len;
                        continue;
                    }

                    for ch in self.piece_text(piece).chars() {
                        if current_line == safe_line {
                            if ch == '\n' {
                                return (line_start, current_len);
                            }
                            current_len += 1;
                        } else if ch == '\n' {
                            current_line += 1;
                            line_start = current_char + 1;
                            current_len = 0;
                        }
                        current_char += 1;
                    }
                }
            }
        }

        (line_start, current_len)
    }

    pub fn line_index_at_offset(&self, offset_chars: usize) -> usize {
        if self.len_chars() == 0 {
            return 0;
        }

        let safe_offset = offset_chars.min(self.len_chars());
        let address = self.find_leaf_for_char_offset(safe_offset);
        let mut current_line = address.leaf_start_newline;
        let mut current_char = address.leaf_start_char;

        let leaf = &self.root.nodes[address.node_index].leaves[address.leaf_index];
        let piece_skip = if !leaf.piece_start_chars.is_empty() {
            let offset_in_leaf = safe_offset - address.leaf_start_char;
            let pi = leaf
                .piece_start_chars
                .partition_point(|&c| c <= offset_in_leaf)
                .saturating_sub(1);
            current_line += leaf.piece_start_newlines[pi];
            current_char += leaf.piece_start_chars[pi];
            pi
        } else {
            0
        };

        for piece in leaf.pieces.iter().skip(piece_skip) {
            for ch in self.piece_text(piece).chars() {
                if current_char >= safe_offset {
                    return current_line;
                }
                if ch == '\n' {
                    current_line += 1;
                }
                current_char += 1;
            }
        }

        current_line
    }

    pub fn extract_text(&self) -> String {
        // Fast path: no edits have been made, original covers the whole buffer.
        if self.add.is_empty() && self.root.metrics.bytes == self.original.len() {
            return self.original.clone();
        }
        self.extract_range(0..self.len_chars())
    }

    pub fn extract_range(&self, range_chars: Range<usize>) -> String {
        let mut result = String::new();
        for span in self.spans_for_range(range_chars) {
            result.push_str(span.text);
        }
        result
    }

    pub fn extract_range_bounded(
        &self,
        range_chars: Range<usize>,
        max_chars: usize,
    ) -> (String, bool) {
        if max_chars == 0 {
            return (
                String::new(),
                !self.normalize_char_range(range_chars).is_empty(),
            );
        }

        let mut remaining = max_chars;
        let mut result = String::new();
        let mut truncated = false;

        for span in self.spans_for_range(range_chars) {
            if remaining == 0 {
                truncated = true;
                break;
            }

            if span.char_len <= remaining {
                result.push_str(span.text);
                remaining -= span.char_len;
                continue;
            }

            let byte_end = byte_index_for_char_offset(span.text, remaining);
            result.push_str(&span.text[..byte_end]);
            truncated = true;
            break;
        }

        (result, truncated)
    }

    pub fn collect_line_bounded(&self, target_line: usize, max_chars: usize) -> (String, bool) {
        let line_info = self.line_info(target_line);
        self.extract_range_bounded(
            line_info.start_char..line_info.start_char + line_info.char_len,
            max_chars,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_LEAF_BYTES, MAX_LEAF_PIECES, MAX_LEAVES_PER_INTERNAL, MIN_LEAVES_PER_INTERNAL,
        PieceTreeLite,
    };

    #[test]
    fn repeated_inserts_split_into_multiple_balanced_nodes() {
        let mut tree = PieceTreeLite::from_string("abc".repeat(128));
        let mut expected = "abc".repeat(128);

        for _ in 0..320 {
            tree.insert(1, "x");
            insert_string_at_char(&mut expected, 1, "x");
        }

        assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
        assert!(tree.root.nodes.len() > 1);
        assert_balanced(&tree);
    }

    #[test]
    fn repeated_removals_merge_nodes_back_down() {
        let mut tree = PieceTreeLite::from_string("abc".repeat(128));
        let mut expected = "abc".repeat(128);

        for _ in 0..320 {
            tree.insert(1, "x");
            insert_string_at_char(&mut expected, 1, "x");
        }
        let expanded_node_count = tree.root.nodes.len();

        tree.remove_char_range(1..321);
        remove_string_char_range(&mut expected, 1..321);

        assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
        assert!(tree.root.nodes.len() < expanded_node_count);
        assert_balanced(&tree);
    }

    #[test]
    fn pack_avoids_runt_nodes() {
        // 18 leaves should produce 2 nodes of 9 each, not 16 + 2
        let mut tree = PieceTreeLite::from_string(String::new());
        // Build a tree that forces 18+ leaves via many insert sites
        let chunk = "x".repeat(1024);
        for i in 0..300 {
            tree.insert(i * 1024, &chunk);
        }
        for node in &tree.root.nodes {
            assert!(
                node.leaves.len() >= MIN_LEAVES_PER_INTERNAL || tree.root.nodes.len() == 1,
                "runt node with {} leaves (min {})",
                node.leaves.len(),
                MIN_LEAVES_PER_INTERNAL,
            );
        }
        assert_balanced(&tree);
    }

    fn assert_balanced(tree: &PieceTreeLite) {
        let mut computed_bytes = 0usize;
        let mut computed_chars = 0usize;
        let mut computed_newlines = 0usize;
        let mut computed_pieces = 0usize;

        if tree.root.nodes.len() > 1 {
            assert!(!tree.root.nodes.is_empty());
        }

        for node in &tree.root.nodes {
            assert!(!node.leaves.is_empty());
            assert!(node.leaves.len() <= MAX_LEAVES_PER_INTERNAL);

            for leaf in &node.leaves {
                if !leaf.pieces.is_empty() {
                    assert!(leaf.pieces.len() <= MAX_LEAF_PIECES);
                    assert!(leaf.metrics.bytes <= MAX_LEAF_BYTES);
                }

                // Verify piece-level prefix sums
                assert_eq!(leaf.piece_start_chars.len(), leaf.pieces.len());
                assert_eq!(leaf.piece_start_newlines.len(), leaf.pieces.len());
                let mut prefix_chars = 0usize;
                let mut prefix_newlines = 0usize;
                for (i, piece) in leaf.pieces.iter().enumerate() {
                    assert_eq!(leaf.piece_start_chars[i], prefix_chars);
                    assert_eq!(leaf.piece_start_newlines[i], prefix_newlines);
                    prefix_chars += piece.char_len;
                    prefix_newlines += piece.newline_count;
                }

                computed_bytes += leaf.metrics.bytes;
                computed_chars += leaf.metrics.chars;
                computed_newlines += leaf.metrics.newlines;
                computed_pieces += leaf.metrics.pieces;
            }
        }

        assert_eq!(tree.metrics().bytes, computed_bytes);
        assert_eq!(tree.metrics().chars, computed_chars);
        assert_eq!(tree.metrics().newlines, computed_newlines);
        assert_eq!(tree.metrics().pieces, computed_pieces);
    }

    fn insert_string_at_char(text: &mut String, char_offset: usize, inserted: &str) {
        let byte_offset = char_to_byte_offset(text, char_offset);
        text.insert_str(byte_offset, inserted);
    }

    fn remove_string_char_range(text: &mut String, range: std::ops::Range<usize>) {
        let start = char_to_byte_offset(text, range.start);
        let end = char_to_byte_offset(text, range.end);
        text.replace_range(start..end, "");
    }

    fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
        if char_offset == 0 {
            return 0;
        }

        text.char_indices()
            .map(|(index, _)| index)
            .nth(char_offset)
            .unwrap_or(text.len())
    }
}
