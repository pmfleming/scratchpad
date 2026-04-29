mod anchor;
mod edit;
mod slice;
mod support;

pub use anchor::{AnchorBias, AnchorId};

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
        let mut current_line = address.leaf_start_newline;
        let mut line_start = address.leaf_start_char;
        let mut current_char = line_start;
        let mut current_len = 0usize;
        find_line_lookup_in_leaves(
            self,
            address,
            safe_line,
            &mut current_line,
            &mut line_start,
            &mut current_char,
            &mut current_len,
        )
        .unwrap_or((line_start, current_len))
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

fn previews_for_matches_in_contiguous_text(
    text: &str,
    ranges: &[Range<usize>],
) -> Vec<(usize, usize, String)> {
    let mut previews = Vec::with_capacity(ranges.len());
    let mut current_char = 0usize;
    let mut current_byte = 0usize;
    let mut line_number = 1usize;
    let mut line_start_char = 0usize;
    let mut line_start_byte = 0usize;
    let mut cached_line_start_byte = None;
    let mut cached_preview = String::new();

    for range in ranges {
        advance_preview_cursor_to(
            text,
            range.start,
            &mut current_char,
            &mut current_byte,
            &mut line_number,
            &mut line_start_char,
            &mut line_start_byte,
        );
        update_cached_line_preview(
            text,
            line_start_byte,
            &mut cached_line_start_byte,
            &mut cached_preview,
        );

        previews.push((
            line_number,
            range.start.saturating_sub(line_start_char) + 1,
            cached_preview.clone(),
        ));
    }

    previews
}

fn first_leaf_piece_skip(
    leaf: &PieceTreeLeaf,
    is_first_leaf: &mut bool,
    safe_line: usize,
    current_line: &mut usize,
    current_char: &mut usize,
) -> usize {
    if !*is_first_leaf {
        return 0;
    }

    *is_first_leaf = false;
    let offset_in_leaf = safe_line.saturating_sub(*current_line);
    if offset_in_leaf == 0 || leaf.piece_start_newlines.is_empty() {
        return 0;
    }

    let piece_index = leaf
        .piece_start_newlines
        .partition_point(|&newline_count| newline_count < offset_in_leaf)
        .saturating_sub(1);
    *current_line += leaf.piece_start_newlines[piece_index];
    *current_char += leaf.piece_start_chars[piece_index];
    piece_index
}

fn scan_piece_for_line_lookup(
    piece_text: &str,
    safe_line: usize,
    current_line: &mut usize,
    line_start: &mut usize,
    current_char: &mut usize,
    current_len: &mut usize,
) -> Option<(usize, usize)> {
    for ch in piece_text.chars() {
        if *current_line == safe_line {
            if ch == '\n' {
                return Some((*line_start, *current_len));
            }
            *current_len += 1;
        } else if ch == '\n' {
            *current_line += 1;
            *line_start = *current_char + 1;
            *current_len = 0;
        }
        *current_char += 1;
    }
    None
}

fn find_line_lookup_in_leaves(
    tree: &PieceTreeLite,
    address: LeafAddress,
    safe_line: usize,
    current_line: &mut usize,
    line_start: &mut usize,
    current_char: &mut usize,
    current_len: &mut usize,
) -> Option<(usize, usize)> {
    let mut is_first_leaf = true;

    for (node_index, node) in tree.root.nodes.iter().enumerate().skip(address.node_index) {
        let leaf_start = if node_index == address.node_index {
            address.leaf_index
        } else {
            0
        };

        for leaf in node.leaves.iter().skip(leaf_start) {
            let piece_skip = first_leaf_piece_skip(
                leaf,
                &mut is_first_leaf,
                safe_line,
                current_line,
                current_char,
            );

            for piece in leaf.pieces.iter().skip(piece_skip) {
                if *current_line < safe_line && *current_line + piece.newline_count < safe_line {
                    *current_line += piece.newline_count;
                    *current_char += piece.char_len;
                    continue;
                }
                if *current_line == safe_line && piece.newline_count == 0 {
                    *current_len += piece.char_len;
                    *current_char += piece.char_len;
                    continue;
                }

                if let Some(line_info) = scan_piece_for_line_lookup(
                    tree.piece_text(piece),
                    safe_line,
                    current_line,
                    line_start,
                    current_char,
                    current_len,
                ) {
                    return Some(line_info);
                }
            }
        }
    }

    None
}

fn advance_preview_cursor_to(
    text: &str,
    target_char: usize,
    current_char: &mut usize,
    current_byte: &mut usize,
    line_number: &mut usize,
    line_start_char: &mut usize,
    line_start_byte: &mut usize,
) {
    while *current_char < target_char && *current_byte < text.len() {
        let Some(ch) = text[*current_byte..].chars().next() else {
            break;
        };
        let next_byte = *current_byte + ch.len_utf8();
        if ch == '\n' {
            *line_number += 1;
            *line_start_char = *current_char + 1;
            *line_start_byte = next_byte;
        }
        *current_char += 1;
        *current_byte = next_byte;
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

#[cfg(test)]
mod tests {
    use super::{
        AnchorBias, MAX_LEAF_BYTES, MAX_LEAF_PIECES, MAX_LEAVES_PER_INTERNAL,
        MIN_LEAVES_PER_INTERNAL, PieceTreeLite,
    };
    use rand::RngExt;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn anchor_follows_content_across_inserts_and_removals() {
        let mut tree = PieceTreeLite::from_string("hello world".to_owned());
        // Anchor between "hello " and "world" — index 6.
        let anchor = tree.create_anchor(6, AnchorBias::Right);

        tree.insert(0, "[prefix] ");
        assert_eq!(tree.anchor_position(anchor), Some(6 + "[prefix] ".chars().count()));

        tree.remove_char_range(0..3); // remove "[pr"
        assert_eq!(tree.anchor_position(anchor), Some(6 + "[prefix] ".chars().count() - 3));
    }

    #[test]
    fn anchor_left_bias_stays_at_split_point_under_insertion() {
        let mut tree = PieceTreeLite::from_string("ab".to_owned());
        let left = tree.create_anchor(1, AnchorBias::Left);
        let right = tree.create_anchor(1, AnchorBias::Right);

        tree.insert(1, "XYZ");

        assert_eq!(tree.anchor_position(left), Some(1));
        assert_eq!(tree.anchor_position(right), Some(4));
    }

    #[test]
    fn anchor_inside_removed_range_collapses_to_start() {
        let mut tree = PieceTreeLite::from_string("abcdefghij".to_owned());
        let anchor = tree.create_anchor(5, AnchorBias::Left);

        tree.remove_char_range(3..8);

        assert_eq!(tree.anchor_position(anchor), Some(3));
    }

    #[test]
    fn anchor_release_drops_anchor_from_tree() {
        let mut tree = PieceTreeLite::from_string("abc".to_owned());
        let anchor = tree.create_anchor(2, AnchorBias::Left);
        tree.release_anchor(anchor);
        assert_eq!(tree.anchor_position(anchor), None);
    }

    #[test]
    fn anchor_clamps_creation_offset_to_document_length() {
        let mut tree = PieceTreeLite::from_string("abc".to_owned());
        let anchor = tree.create_anchor(99, AnchorBias::Left);
        // Clamped to len_chars at creation time.
        assert_eq!(tree.anchor_position(anchor), Some(3));
    }

    #[test]
    fn anchor_survives_clone_of_tree_independently() {
        // Cloning forks the registry; subsequent edits in either side update
        // only that clone's anchors, not the other's.
        let mut tree = PieceTreeLite::from_string("abcdef".to_owned());
        let anchor = tree.create_anchor(3, AnchorBias::Left);
        let mut other = tree.clone();

        tree.insert(0, "ZZ");
        other.remove_char_range(0..2);

        assert_eq!(tree.anchor_position(anchor), Some(5));
        assert_eq!(other.anchor_position(anchor), Some(1));
    }

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

    #[test]
    fn randomized_edit_sequences_match_string_model() {
        for seed in [
            0xC0DE_0001_u64,
            0xC0DE_0002_u64,
            0xC0DE_0003_u64,
            0xC0DE_0004_u64,
        ] {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut expected = random_text(&mut rng, 96);
            let mut tree = PieceTreeLite::from_string(expected.clone());
            assert_tree_matches_string_model(&tree, &expected);

            for _step in 0..300 {
                match rng.random_range(0..4) {
                    0 => {
                        let at = rng.random_range(0..=expected.chars().count());
                        let inserted_len = rng.random_range(0..=12);
                        let inserted = random_text(&mut rng, inserted_len);
                        tree.insert(at, &inserted);
                        insert_string_at_char(&mut expected, at, &inserted);
                    }
                    1 => {
                        if expected.is_empty() {
                            continue;
                        }
                        let len = expected.chars().count();
                        let start = rng.random_range(0..len);
                        let end = rng.random_range(start + 1..=len);
                        tree.remove_char_range(start..end);
                        remove_string_char_range(&mut expected, start..end);
                    }
                    2 => {
                        let len = expected.chars().count();
                        let start = rng.random_range(0..=len);
                        let end = rng.random_range(start..=len);
                        let replacement_len = rng.random_range(0..=10);
                        let replacement = random_text(&mut rng, replacement_len);
                        tree.remove_char_range(start..end);
                        if !replacement.is_empty() {
                            tree.insert(start, &replacement);
                        }
                        replace_string_char_range(&mut expected, start..end, &replacement);
                    }
                    _ => {
                        let full = tree.extract_range(0..tree.len_chars());
                        let rebuilt = PieceTreeLite::from_string(full.clone());
                        assert_eq!(rebuilt.extract_range(0..rebuilt.len_chars()), full);
                        assert_eq!(rebuilt.metrics().bytes, tree.metrics().bytes);
                        assert_eq!(rebuilt.metrics().chars, tree.metrics().chars);
                        assert_eq!(rebuilt.metrics().newlines, tree.metrics().newlines);
                    }
                }

                assert_tree_matches_string_model(&tree, &expected);
            }
        }
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

    fn assert_tree_matches_string_model(tree: &PieceTreeLite, expected: &str) {
        assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
        assert_eq!(tree.len_chars(), expected.chars().count());
        assert_eq!(tree.len_bytes(), expected.len());
        assert_eq!(tree.metrics().chars, expected.chars().count());
        assert_eq!(tree.metrics().bytes, expected.len());
        assert_eq!(tree.metrics().newlines, expected.matches('\n').count());

        for (offset, ch) in expected.chars().enumerate() {
            assert_eq!(tree.char_at(offset), Some(ch), "char mismatch at {offset}");
        }
        assert_eq!(tree.char_at(expected.chars().count()), None);

        let lines = split_lines_without_newlines(expected);
        for (line_index, expected_line) in lines.iter().enumerate() {
            let info = tree.line_info(line_index);
            assert_eq!(info.line_index, line_index);
            assert_eq!(
                tree.extract_range(info.start_char..info.start_char + info.char_len),
                *expected_line,
                "line {line_index} mismatch"
            );
        }

        assert_balanced(tree);
    }

    #[test]
    fn batched_previews_match_individual_preview_generation() {
        let tree = PieceTreeLite::from_string("one\ntwo alpha\nthree alpha\nfour".to_owned());
        let ranges = vec![8..13, 20..25];

        let previews = tree.previews_for_matches(&ranges, ranges.len());
        let expected = ranges
            .iter()
            .map(|range| tree.preview_for_match(range))
            .collect::<Vec<_>>();

        assert_eq!(previews, expected);
    }

    #[test]
    fn batched_previews_match_individual_preview_generation_after_fragmentation() {
        let mut tree = PieceTreeLite::from_string("one\ntwo alpha\nthree alpha\nfour".to_owned());
        tree.insert(0, "zero\n");
        let ranges = vec![13..18, 25..30];

        let previews = tree.previews_for_matches(&ranges, ranges.len());
        let expected = ranges
            .iter()
            .map(|range| tree.preview_for_match(range))
            .collect::<Vec<_>>();

        assert_eq!(previews, expected);
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

    fn replace_string_char_range(
        text: &mut String,
        range: std::ops::Range<usize>,
        replacement: &str,
    ) {
        let start = char_to_byte_offset(text, range.start);
        let end = char_to_byte_offset(text, range.end);
        text.replace_range(start..end, replacement);
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

    fn random_text(rng: &mut StdRng, max_len: usize) -> String {
        const ALPHABET: &[char] = &[
            'a', 'b', 'c', 'x', 'y', 'z', '0', '1', '2', ' ', '\n', 'é', 'λ', 'β', '🙂', '界',
        ];
        let len = rng.random_range(0..=max_len);
        let mut text = String::new();
        for _ in 0..len {
            text.push(ALPHABET[rng.random_range(0..ALPHABET.len())]);
        }
        text
    }

    fn split_lines_without_newlines(text: &str) -> Vec<&str> {
        if text.is_empty() {
            return vec![""];
        }
        text.split('\n').collect()
    }
}
