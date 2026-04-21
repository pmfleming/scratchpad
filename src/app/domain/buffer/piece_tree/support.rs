use super::{
    MAX_LEAF_BYTES, MAX_LEAF_PIECES, MAX_LEAVES_PER_INTERNAL, MIN_LEAVES_PER_INTERNAL,
    PREVIEW_MAX_CHARS, Piece, PieceBuffer, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeMetrics,
    PieceTreeRoot,
};

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
    };
    root.recalculate();
    root
}

pub(super) fn pack_leaves_into_nodes(leaves: Vec<PieceTreeLeaf>) -> Vec<PieceTreeInternalNode> {
    let mut nodes = Vec::new();
    let mut index = 0usize;
    let total = leaves.len();
    while index < total {
        let remaining = total - index;
        let chunk_size = if remaining > MAX_LEAVES_PER_INTERNAL
            && remaining - MAX_LEAVES_PER_INTERNAL < MIN_LEAVES_PER_INTERNAL
        {
            remaining.div_ceil(2)
        } else {
            MAX_LEAVES_PER_INTERNAL.min(remaining)
        };
        let end = index + chunk_size;
        let mut node = PieceTreeInternalNode {
            leaves: leaves[index..end].to_vec(),
            metrics: PieceTreeMetrics::default(),
            leaf_start_chars: Vec::new(),
            leaf_start_newlines: Vec::new(),
        };
        node.recalculate();
        nodes.push(node);
        index = end;
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

pub(super) fn pack_pieces_into_leaves(pieces: Vec<Piece>) -> Vec<PieceTreeLeaf> {
    let mut leaves = Vec::new();
    let mut current = PieceTreeLeaf::default();

    for piece in pieces {
        if piece.byte_len == 0 {
            continue;
        }

        let would_exceed_bytes =
            !current.pieces.is_empty() && current.metrics.bytes + piece.byte_len > MAX_LEAF_BYTES;
        let would_exceed_pieces =
            !current.pieces.is_empty() && current.pieces.len() >= MAX_LEAF_PIECES;
        if would_exceed_bytes || would_exceed_pieces {
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

pub(super) fn count_newlines(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
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
    let trimmed_chars = trimmed.chars().collect::<Vec<_>>();
    if trimmed_chars.len() <= PREVIEW_MAX_CHARS {
        return trimmed.to_owned();
    }

    let mut preview = trimmed_chars[..PREVIEW_MAX_CHARS]
        .iter()
        .collect::<String>();
    preview.push_str("...");
    preview
}
