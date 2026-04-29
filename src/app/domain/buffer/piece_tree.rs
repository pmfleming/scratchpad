mod anchor;
mod edit;
mod slice;
mod support;

pub use anchor::{AnchorBias, AnchorId};

use std::ops::Range;

use self::slice::previews_for_matches_in_contiguous_text;
use support::{
    build_chunked_pieces, build_root_from_pieces, byte_index_for_char_offset,
    byte_range_for_char_range, compact_preview, count_newlines, line_lookup_in_leaves,
    pack_pieces_into_leaves, recalculate_prefix_metrics,
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
            last.is_ascii &= piece.is_ascii;
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
    anchors: anchor::AnchorRegistry,
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
            anchors: anchor::AnchorRegistry::default(),
        }
    }

    /// Create a stable anchor at `char_offset`. The returned `AnchorId`
    /// continues to identify the same logical position across edits — the
    /// tree shifts the underlying offset on `insert`/`remove_char_range`
    /// according to `bias`.
    pub fn create_anchor(&mut self, char_offset: usize, bias: AnchorBias) -> AnchorId {
        let safe = char_offset.min(self.len_chars());
        self.anchors.create(safe, bias)
    }

    /// Release an anchor. Anchor IDs are not recycled, so it is safe to
    /// release IDs that may already be released; the call is a no-op then.
    pub fn release_anchor(&mut self, id: AnchorId) {
        self.anchors.release(id);
    }

    /// Resolve an anchor to its current `char_offset`, or `None` if it has
    /// been released.
    pub fn anchor_position(&self, id: AnchorId) -> Option<usize> {
        self.anchors.position(id)
    }

    /// Bias the anchor was created with, or `None` if released.
    pub fn anchor_bias(&self, id: AnchorId) -> Option<AnchorBias> {
        self.anchors.entry(id).map(|entry| entry.bias)
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

    pub fn char_at(&self, offset_chars: usize) -> Option<char> {
        if offset_chars >= self.len_chars() || self.root.nodes.is_empty() {
            return None;
        }

        let address = self.find_leaf_for_char_offset(offset_chars);
        let (piece, offset_in_piece) = self.piece_at_char_offset(address, offset_chars)?;
        let piece_text = self.piece_text(piece);

        if piece.is_ascii {
            piece_text
                .as_bytes()
                .get(offset_in_piece)
                .map(|byte| *byte as char)
        } else {
            piece_text.chars().nth(offset_in_piece)
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

    pub fn previews_for_matches(
        &self,
        ranges: &[Range<usize>],
        limit: usize,
    ) -> Vec<(usize, usize, String)> {
        let limited_ranges = &ranges[..ranges.len().min(limit)];
        if limited_ranges.is_empty() {
            return Vec::new();
        }

        if let Some(text) = self.borrow_range(0..self.len_chars()) {
            return previews_for_matches_in_contiguous_text(text, limited_ranges);
        }

        limited_ranges
            .iter()
            .map(|range| self.preview_for_match(range))
            .collect()
    }

    pub fn line_lookup(&self, target_line: usize) -> (usize, usize) {
        if self.len_chars() == 0 {
            return (0, 0);
        }

        let safe_line = target_line.min(self.root.metrics.newlines);
        let address = self.find_leaf_for_line(safe_line);
        line_lookup_in_leaves(self, address, safe_line)
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

    pub fn borrow_range(&self, range_chars: Range<usize>) -> Option<&str> {
        let normalized = self.normalize_char_range(range_chars);
        if normalized.is_empty() {
            return Some("");
        }

        let mut spans = self.spans_for_range(normalized.clone());
        let first = spans.next()?;
        if first.char_start != normalized.start || first.char_len != normalized.len() {
            return None;
        }
        if spans.next().is_some() {
            return None;
        }

        Some(first.text)
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

    fn piece_at_char_offset(
        &self,
        address: LeafAddress,
        offset_chars: usize,
    ) -> Option<(&Piece, usize)> {
        let leaf = self
            .root
            .nodes
            .get(address.node_index)?
            .leaves
            .get(address.leaf_index)?;
        let offset_in_leaf = offset_chars.saturating_sub(address.leaf_start_char);
        let piece_index = leaf
            .piece_start_chars
            .partition_point(|&char_start| char_start <= offset_in_leaf)
            .saturating_sub(1);
        let piece = leaf.pieces.get(piece_index)?;
        let offset_in_piece = offset_in_leaf.saturating_sub(leaf.piece_start_chars[piece_index]);
        Some((piece, offset_in_piece))
    }
}

#[cfg(test)]
mod tests;
