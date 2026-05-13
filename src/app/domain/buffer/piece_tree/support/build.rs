use super::super::{
    MAX_LEAF_BYTES, MAX_LEAF_PIECES, MAX_LEAVES_PER_INTERNAL, MIN_LEAVES_PER_INTERNAL, Piece,
    PieceBuffer, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeMetrics, PieceTreeRoot,
};
use std::ops::Range;
use std::thread;

const PARALLEL_PIECE_BUILD_MIN_BYTES: usize = 4 * 1024 * 1024;
const PARALLEL_PIECE_BUILD_MAX_WORKERS: usize = 8;

pub(in crate::app::domain::buffer::piece_tree) fn build_root_from_pieces(
    pieces: Vec<Piece>,
) -> PieceTreeRoot {
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
    root.recalculate_from_node_metrics();
    root
}

pub(in crate::app::domain::buffer::piece_tree) fn pack_leaves_into_nodes(
    mut leaves: Vec<PieceTreeLeaf>,
) -> Vec<PieceTreeInternalNode> {
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
        node.recalculate_from_leaf_metrics();
        nodes.push(node);
    }
    nodes
}

pub(in crate::app::domain::buffer::piece_tree) fn build_chunked_pieces(
    buffer: PieceBuffer,
    start_byte: usize,
    text: &str,
) -> Vec<Piece> {
    if text.is_empty() {
        return Vec::new();
    }

    build_chunked_pieces_parallel(buffer, start_byte, text)
        .unwrap_or_else(|| build_chunked_pieces_serial(buffer, start_byte, text))
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

pub(in crate::app::domain::buffer::piece_tree) fn pack_pieces_into_leaves(
    pieces: Vec<Piece>,
) -> Vec<PieceTreeLeaf> {
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

        current.push_piece_for_pack(piece);
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
