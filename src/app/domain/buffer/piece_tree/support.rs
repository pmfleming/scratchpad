use super::{
    LeafAddress, MAX_LEAF_BYTES, MAX_LEAF_PIECES, MAX_LEAVES_PER_INTERNAL, MIN_LEAVES_PER_INTERNAL,
    PREVIEW_MAX_CHARS, Piece, PieceBuffer, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLite,
    PieceTreeMetrics, PieceTreeRoot,
};
use memchr::memchr_iter;
use std::ops::Range;
use std::thread;

const PARALLEL_PIECE_BUILD_MIN_BYTES: usize = 4 * 1024 * 1024;
const PARALLEL_PIECE_BUILD_MAX_WORKERS: usize = 8;

pub(super) struct PieceTextMetrics {
    pub byte_len: usize,
    pub char_len: usize,
    pub newline_count: usize,
    pub is_ascii: bool,
}

pub(super) fn measure_text(text: &str) -> PieceTextMetrics {
    let bytes = text.as_bytes();
    let byte_len = bytes.len();
    if bytes.is_ascii() {
        return PieceTextMetrics {
            byte_len,
            char_len: byte_len,
            newline_count: memchr_iter(b'\n', bytes).count(),
            is_ascii: true,
        };
    }

    let mut char_len = 0usize;
    let mut newline_count = 0usize;
    for &byte in bytes {
        char_len += usize::from((byte & 0b1100_0000) != 0b1000_0000);
        newline_count += usize::from(byte == b'\n');
    }

    PieceTextMetrics {
        byte_len,
        char_len,
        newline_count,
        is_ascii: false,
    }
}

pub(super) fn recalculate_prefix_metrics<T>(
    items: &[T],
    start_chars: &mut Vec<usize>,
    start_newlines: &mut Vec<usize>,
    metrics_of: impl Fn(&T) -> PieceTreeMetrics,
) -> PieceTreeMetrics {
    let mut metrics = PieceTreeMetrics::default();
    start_chars.clear();
    start_newlines.clear();

    let mut current_chars = 0usize;
    let mut current_newlines = 0usize;
    for item in items {
        start_chars.push(current_chars);
        start_newlines.push(current_newlines);

        let item_metrics = metrics_of(item);
        metrics.add_assign(item_metrics);
        current_chars += item_metrics.chars;
        current_newlines += item_metrics.newlines;
    }

    metrics
}

pub(super) fn build_root_from_pieces(pieces: Vec<Piece>) -> PieceTreeRoot {
    build_root_from_leaves(pack_pieces_into_leaves(pieces))
}

fn build_root_from_leaves(mut leaves: Vec<PieceTreeLeaf>) -> PieceTreeRoot {
    if leaves.is_empty() {
        leaves.push(PieceTreeLeaf::default());
    }

    let mut root = PieceTreeRoot {
        nodes: pack_leaves_into_nodes(leaves),
        metrics: PieceTreeMetrics::default(),
        node_start_chars: Vec::new(),
        node_start_newlines: Vec::new(),
        anchor_count: 0,
    };
    root.recalculate();
    root
}

pub(super) fn pack_leaves_into_nodes(mut leaves: Vec<PieceTreeLeaf>) -> Vec<PieceTreeInternalNode> {
    let mut nodes = Vec::with_capacity(leaves.len().div_ceil(MAX_LEAVES_PER_INTERNAL));
    while !leaves.is_empty() {
        let remaining = leaves.len();
        let chunk_size = if remaining > MAX_LEAVES_PER_INTERNAL
            && remaining - MAX_LEAVES_PER_INTERNAL < MIN_LEAVES_PER_INTERNAL
        {
            remaining.div_ceil(2)
        } else {
            MAX_LEAVES_PER_INTERNAL.min(remaining)
        };
        let mut node = PieceTreeInternalNode {
            leaves: leaves.drain(..chunk_size).collect(),
            metrics: PieceTreeMetrics::default(),
            leaf_start_chars: Vec::new(),
            leaf_start_newlines: Vec::new(),
            anchor_count: 0,
        };
        node.recalculate();
        nodes.push(node);
    }
    nodes
}

pub(super) fn build_chunked_pieces(
    buffer: PieceBuffer,
    start_byte: usize,
    text: &str,
) -> Vec<Piece> {
    if text.is_empty() {
        return Vec::new();
    }

    if let Some(pieces) = build_chunked_pieces_parallel(buffer, start_byte, text) {
        return pieces;
    }

    build_chunked_pieces_serial(buffer, start_byte, text)
}

fn build_chunked_pieces_serial(buffer: PieceBuffer, start_byte: usize, text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut offset = 0usize;
    while offset < text.len() {
        let len = next_chunk_len(text, offset, MAX_LEAF_BYTES);
        let slice = &text[offset..offset + len];
        pieces.push(Piece::from_slice(buffer, start_byte + offset, slice));
        offset += len;
    }
    pieces
}

fn build_chunked_pieces_parallel(
    buffer: PieceBuffer,
    start_byte: usize,
    text: &str,
) -> Option<Vec<Piece>> {
    if text.len() < PARALLEL_PIECE_BUILD_MIN_BYTES {
        return None;
    }

    let ranges = piece_chunk_ranges(text);
    let workers = piece_build_worker_count(text.len()).min(ranges.len());
    if workers <= 1 || ranges.len() < workers * 2 {
        return None;
    }

    let chunk_size = ranges.len().div_ceil(workers);
    let mut per_worker = Vec::with_capacity(workers);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for range_chunk in ranges.chunks(chunk_size) {
            handles.push(scope.spawn(move || {
                range_chunk
                    .iter()
                    .map(|range| {
                        Piece::from_slice(
                            buffer,
                            start_byte + range.start,
                            &text[range.start..range.end],
                        )
                    })
                    .collect::<Vec<_>>()
            }));
        }
        for handle in handles {
            if let Ok(pieces) = handle.join() {
                per_worker.push(pieces);
            }
        }
    });

    let total = per_worker.iter().map(Vec::len).sum();
    let mut pieces = Vec::with_capacity(total);
    for mut worker_pieces in per_worker {
        pieces.append(&mut worker_pieces);
    }
    Some(pieces)
}

fn piece_chunk_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;
    while offset < text.len() {
        let len = next_chunk_len(text, offset, MAX_LEAF_BYTES);
        ranges.push(offset..offset + len);
        offset += len;
    }
    ranges
}

fn piece_build_worker_count(total_bytes: usize) -> usize {
    let by_size = (total_bytes / PARALLEL_PIECE_BUILD_MIN_BYTES).max(1);
    thread::available_parallelism()
        .map(|parallelism| {
            parallelism
                .get()
                .min(PARALLEL_PIECE_BUILD_MAX_WORKERS)
                .min(by_size)
        })
        .unwrap_or(1)
        .max(1)
}

pub(super) fn pack_pieces_into_leaves(pieces: Vec<Piece>) -> Vec<PieceTreeLeaf> {
    let mut leaves = Vec::with_capacity(pieces.len() / MAX_LEAF_PIECES + 1);
    let mut current = PieceTreeLeaf::default();

    for piece in pieces {
        if piece.byte_len == 0 {
            continue;
        }

        if should_start_new_leaf(&current, &piece) {
            current.recalculate();
            leaves.push(current);
            current = PieceTreeLeaf::default();
        }

        current.push_piece(piece);
    }

    if !current.pieces.is_empty() || leaves.is_empty() {
        current.recalculate();
        leaves.push(current);
    }

    leaves
}

fn should_start_new_leaf(current: &PieceTreeLeaf, piece: &Piece) -> bool {
    if current.pieces.is_empty() {
        return false;
    }

    current.metrics.bytes + piece.byte_len > MAX_LEAF_BYTES
        || current.pieces.len() >= MAX_LEAF_PIECES
}

fn next_chunk_len(text: &str, offset: usize, max_len: usize) -> usize {
    let candidate_end = (offset + max_len).min(text.len());
    if text.is_char_boundary(candidate_end) {
        return candidate_end - offset;
    }

    let mut end = candidate_end;
    while end > offset && !text.is_char_boundary(end) {
        end -= 1;
    }
    end - offset
}

pub(super) fn byte_range_for_char_range(
    text: &str,
    start_char: usize,
    end_char: usize,
) -> std::ops::Range<usize> {
    let start = byte_index_for_char_offset(text, start_char);
    let end = byte_index_for_char_offset(text, end_char);
    start..end
}

pub(super) fn byte_index_for_char_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }
    if text.is_ascii() {
        return char_offset.min(text.len());
    }

    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_offset)
        .unwrap_or(text.len())
}

pub(super) fn compact_preview(line_text: &str) -> String {
    let trimmed = line_text.trim();
    let Some((end_byte, _)) = trimmed.char_indices().nth(PREVIEW_MAX_CHARS) else {
        return trimmed.to_owned();
    };
    format!("{}...", &trimmed[..end_byte])
}

pub(super) fn line_lookup_in_leaves(
    tree: &PieceTreeLite,
    address: LeafAddress,
    safe_line: usize,
) -> (usize, usize) {
    let mut cursor = LineLookupCursor::new(address);

    for (node_index, node) in tree.root.nodes.iter().enumerate().skip(address.node_index) {
        let leaf_start = if node_index == address.node_index {
            address.leaf_index
        } else {
            0
        };

        if leaf_start == 0 {
            if skip_node_before_line(node, safe_line, &mut cursor) {
                continue;
            }
            if append_node_to_target_line(node, safe_line, &mut cursor) {
                continue;
            }
        }

        for leaf in node.leaves.iter().skip(leaf_start) {
            if skip_leaf_before_line(leaf, safe_line, &mut cursor) {
                continue;
            }
            if append_leaf_to_target_line(leaf, safe_line, &mut cursor) {
                continue;
            }
            if let Some(line_info) = scan_leaf_for_line_lookup(tree, leaf, safe_line, &mut cursor) {
                return line_info;
            }
        }
    }

    cursor.line_info()
}

struct LineLookupCursor {
    current_line: usize,
    line_start: usize,
    current_char: usize,
    current_len: usize,
}

impl LineLookupCursor {
    fn new(address: LeafAddress) -> Self {
        Self {
            current_line: address.leaf_start_newline,
            line_start: address.leaf_start_char,
            current_char: address.leaf_start_char,
            current_len: 0,
        }
    }

    fn line_info(&self) -> (usize, usize) {
        (self.line_start, self.current_len)
    }
}

fn skip_node_before_line(
    node: &PieceTreeInternalNode,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> bool {
    if cursor.current_line < safe_line && cursor.current_line + node.metrics.newlines < safe_line {
        cursor.current_line += node.metrics.newlines;
        cursor.current_char += node.metrics.chars;
        true
    } else {
        false
    }
}

fn append_node_to_target_line(
    node: &PieceTreeInternalNode,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> bool {
    if cursor.current_line == safe_line && node.metrics.newlines == 0 {
        cursor.current_len += node.metrics.chars;
        cursor.current_char += node.metrics.chars;
        true
    } else {
        false
    }
}

fn skip_leaf_before_line(
    leaf: &PieceTreeLeaf,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> bool {
    if cursor.current_line < safe_line && cursor.current_line + leaf.metrics.newlines < safe_line {
        cursor.current_line += leaf.metrics.newlines;
        cursor.current_char += leaf.metrics.chars;
        true
    } else {
        false
    }
}

fn append_leaf_to_target_line(
    leaf: &PieceTreeLeaf,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> bool {
    if cursor.current_line == safe_line && leaf.metrics.newlines == 0 {
        cursor.current_len += leaf.metrics.chars;
        cursor.current_char += leaf.metrics.chars;
        true
    } else {
        false
    }
}

fn scan_leaf_for_line_lookup(
    tree: &PieceTreeLite,
    leaf: &PieceTreeLeaf,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> Option<(usize, usize)> {
    for piece in &leaf.pieces {
        if skip_piece_before_line(piece, safe_line, cursor) {
            continue;
        }
        if append_piece_to_target_line(piece, safe_line, cursor) {
            continue;
        }

        if let Some(line_info) =
            scan_piece_for_line_lookup(tree.piece_text(piece), safe_line, cursor)
        {
            return Some(line_info);
        }
    }
    None
}

fn skip_piece_before_line(piece: &Piece, safe_line: usize, cursor: &mut LineLookupCursor) -> bool {
    if cursor.current_line < safe_line && cursor.current_line + piece.newline_count < safe_line {
        cursor.current_line += piece.newline_count;
        cursor.current_char += piece.char_len;
        true
    } else {
        false
    }
}

fn append_piece_to_target_line(
    piece: &Piece,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> bool {
    if cursor.current_line == safe_line && piece.newline_count == 0 {
        cursor.current_len += piece.char_len;
        cursor.current_char += piece.char_len;
        true
    } else {
        false
    }
}

fn scan_piece_for_line_lookup(
    piece_text: &str,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> Option<(usize, usize)> {
    for ch in piece_text.chars() {
        if cursor.current_line == safe_line {
            if ch == '\n' {
                return Some(cursor.line_info());
            }
            cursor.current_len += 1;
        } else if ch == '\n' {
            cursor.current_line += 1;
            cursor.line_start = cursor.current_char + 1;
            cursor.current_len = 0;
        }
        cursor.current_char += 1;
    }
    None
}
