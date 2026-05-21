mod anchor;
mod edit;
mod metrics;
pub(super) mod preview;
pub(crate) mod query;
mod runtime;
mod slice;
mod storage;
mod support;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use super::history::PIECE_PROVENANCE_ENTRY_LIMIT;
pub(crate) use super::history::{ByteSpan, PieceProvenance, PieceSource};
pub use anchor::{AnchorBias, AnchorId, AnchorOwner, AnchorOwnerKind};

use std::ops::Range;

use anchor::{LeafAnchor, LeafId, PieceTreeAnchorState};
use metrics::sum_node_metrics;
use runtime::PieceTreeRuntime;
use storage::PieceTreeStorage;
use support::{
    build_chunked_pieces, build_root_from_pieces, byte_index_for_char_offset,
    byte_range_for_char_range, line_lookup_in_leaves, measure_text, pack_pieces_into_leaves,
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
    pub byte_span: ByteSpan,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PieceTreeMetrics {
    pub bytes: usize,
    pub chars: usize,
    pub newlines: usize,
    pub pieces: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PieceBuffer {
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
        let metrics = measure_text(text);
        Self {
            buffer,
            start_byte,
            byte_len: metrics.byte_len,
            char_len: metrics.char_len,
            newline_count: metrics.newline_count,
            is_ascii: metrics.is_ascii,
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
    leaf_id: LeafId,
    pieces: Vec<Piece>,
    metrics: PieceTreeMetrics,
    piece_start_chars: Vec<usize>,
    piece_start_newlines: Vec<usize>,
    anchors: Vec<LeafAnchor>,
}

impl PieceTreeLeaf {
    fn push_piece_for_pack(&mut self, piece: Piece) {
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
            self.metrics.bytes += piece.byte_len;
            self.metrics.chars += piece.char_len;
            self.metrics.newlines += piece.newline_count;
        } else {
            self.metrics.add_assign(piece.metrics());
            self.pieces.push(piece);
        }
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
    anchor_count: usize,
}

impl PieceTreeInternalNode {
    fn recalculate(&mut self) {
        if self.leaves.is_empty() {
            self.leaves.push(PieceTreeLeaf::default());
        }

        for leaf in &mut self.leaves {
            leaf.recalculate();
        }
        self.recalculate_from_leaf_metrics();
    }

    fn recalculate_from_leaf_metrics(&mut self) {
        self.metrics = recalculate_prefix_metrics(
            &self.leaves,
            &mut self.leaf_start_chars,
            &mut self.leaf_start_newlines,
            |leaf| leaf.metrics,
        );
        self.anchor_count = self.leaves.iter().map(|leaf| leaf.anchors.len()).sum();
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeRoot {
    nodes: Vec<PieceTreeInternalNode>,
    metrics: PieceTreeMetrics,
    node_start_chars: Vec<usize>,
    node_start_newlines: Vec<usize>,
    anchor_count: usize,
}

impl PieceTreeRoot {
    fn recalculate(&mut self) {
        if self.nodes.is_empty() {
            self.nodes.push(PieceTreeInternalNode::default());
        }

        for node in &mut self.nodes {
            node.recalculate();
        }
        self.recalculate_from_node_metrics();
    }

    fn recalculate_from_node_metrics(&mut self) {
        self.metrics = recalculate_prefix_metrics(
            &self.nodes,
            &mut self.node_start_chars,
            &mut self.node_start_newlines,
            |node| node.metrics,
        );
        self.anchor_count = self.nodes.iter().map(|node| node.anchor_count).sum();
    }

    fn replace_recalculated_nodes(
        &mut self,
        range: Range<usize>,
        replacement_nodes: Vec<PieceTreeInternalNode>,
    ) {
        let old_metrics = sum_node_metrics(&self.nodes[range.clone()]);
        let old_anchor_count = self.nodes[range.clone()]
            .iter()
            .map(|node| node.anchor_count)
            .sum::<usize>();
        let new_metrics = sum_node_metrics(&replacement_nodes);
        let new_anchor_count = replacement_nodes
            .iter()
            .map(|node| node.anchor_count)
            .sum::<usize>();

        self.nodes.splice(range, replacement_nodes);
        if self.nodes.is_empty() {
            self.nodes.push(PieceTreeInternalNode::default());
            self.nodes[0].recalculate();
        }

        self.metrics.saturating_sub_assign(old_metrics);
        self.metrics.add_assign(new_metrics);
        self.anchor_count = self
            .anchor_count
            .saturating_sub(old_anchor_count)
            .saturating_add(new_anchor_count);
        self.refresh_node_prefixes();
    }

    fn refresh_node_prefixes(&mut self) {
        self.node_start_chars.clear();
        self.node_start_newlines.clear();

        let mut current_chars = 0usize;
        let mut current_newlines = 0usize;
        for node in &self.nodes {
            self.node_start_chars.push(current_chars);
            self.node_start_newlines.push(current_newlines);
            current_chars += node.metrics.chars;
            current_newlines += node.metrics.newlines;
        }
    }
}

#[derive(Clone, Debug)]
pub struct PieceTreeLite {
    storage: PieceTreeStorage,
    root: PieceTreeRoot,
    runtime: PieceTreeRuntime,
    anchor_state: PieceTreeAnchorState,
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
    #[must_use]
    pub fn from_string(text: String) -> Self {
        let pieces = build_chunked_pieces(PieceBuffer::Original, 0, &text);
        let mut tree = Self {
            storage: PieceTreeStorage::from_original(text),
            root: build_root_from_pieces(pieces),
            runtime: PieceTreeRuntime::default(),
            anchor_state: PieceTreeAnchorState::default(),
        };
        tree.assign_missing_leaf_ids();
        tree
    }

    fn line_scan_start_for_leaf(
        leaf: &PieceTreeLeaf,
        address: LeafAddress,
        safe_offset: usize,
    ) -> (usize, usize, usize) {
        if leaf.piece_start_chars.is_empty() {
            return (0, address.leaf_start_newline, address.leaf_start_char);
        }

        let offset_in_leaf = safe_offset - address.leaf_start_char;
        let piece_index = leaf
            .piece_start_chars
            .partition_point(|&char_start| char_start <= offset_in_leaf)
            .saturating_sub(1);
        (
            piece_index,
            address.leaf_start_newline + leaf.piece_start_newlines[piece_index],
            address.leaf_start_char + leaf.piece_start_chars[piece_index],
        )
    }

    fn refresh_leaf_index_after_structure_change(&mut self) {
        self.anchor_state
            .refresh_leaf_index_after_structure_change(&self.root);
    }

    #[must_use]
    pub fn metrics(&self) -> PieceTreeMetrics {
        self.root.metrics
    }

    #[must_use]
    pub fn provenance_entry_count(&self) -> usize {
        self.storage.provenance_entry_count()
    }

    #[must_use]
    pub fn previews_for_matches(
        &self,
        ranges: &[Range<usize>],
        limit: usize,
    ) -> Vec<(usize, usize, String)> {
        preview::previews_for_matches(self, ranges, limit)
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.runtime.generation()
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.root.metrics.bytes
    }

    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.root.metrics.chars
    }

    #[must_use]
    pub fn normalize_char_range(&self, range_chars: Range<usize>) -> Range<usize> {
        let start = range_chars.start.min(self.len_chars());
        let end = range_chars.end.min(self.len_chars());
        if start <= end { start..end } else { end..start }
    }

    #[must_use]
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

    #[must_use]
    pub fn line_info(&self, target_line: usize) -> PieceTreeLineInfo {
        let (start_char, char_len) = self.line_lookup(target_line);
        PieceTreeLineInfo {
            line_index: target_line.min(self.root.metrics.newlines),
            start_char,
            char_len,
        }
    }

    #[must_use]
    pub fn line_lookup(&self, target_line: usize) -> (usize, usize) {
        if self.len_chars() == 0 {
            return (0, 0);
        }

        let safe_line = target_line.min(self.root.metrics.newlines);
        let address = self.find_leaf_for_line_index(safe_line);
        line_lookup_in_leaves(self, address, safe_line)
    }

    #[must_use]
    pub fn line_index_at_offset(&self, offset_chars: usize) -> usize {
        if self.len_chars() == 0 {
            return 0;
        }

        let safe_offset = offset_chars.min(self.len_chars());
        let address = self.find_leaf_for_char_offset(safe_offset);
        let leaf = &self.root.nodes[address.node_index].leaves[address.leaf_index];
        let (piece_skip, current_line, current_char) =
            Self::line_scan_start_for_leaf(leaf, address, safe_offset);

        self.line_index_from_piece_scan(leaf, piece_skip, current_line, current_char, safe_offset)
    }

    fn line_index_from_piece_scan(
        &self,
        leaf: &PieceTreeLeaf,
        piece_skip: usize,
        mut current_line: usize,
        mut current_char: usize,
        safe_offset: usize,
    ) -> usize {
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

    #[must_use]
    pub fn extract_text(&self) -> String {
        // Fast path: no edits have been made, original covers the whole buffer.
        if self.storage.add_is_empty() && self.root.metrics.bytes == self.storage.original_len() {
            return self.storage.original_text().to_owned();
        }
        self.extract_range(0..self.len_chars())
    }

    #[must_use]
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

    #[must_use]
    pub fn extract_range(&self, range_chars: Range<usize>) -> String {
        self.extract_range_with_capacity(range_chars, 0)
    }

    #[must_use]
    pub fn extract_range_with_capacity(
        &self,
        range_chars: Range<usize>,
        capacity: usize,
    ) -> String {
        let mut result = String::with_capacity(capacity);
        for span in self.spans_for_range(range_chars) {
            result.push_str(span.text);
        }
        result
    }

    #[must_use]
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
