use super::super::{
    LINE_SAMPLE_STRIDE, LeafAddress, Piece, PieceTreeLeaf, PieceTreeLite, PieceTreeMetrics,
};

pub(in crate::app::domain::buffer::piece_tree) fn line_lookup_in_leaves(
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

        if leaf_start == 0 && cursor.advance_over(node.metrics, safe_line) {
            continue;
        }

        for leaf in node.leaves.iter().skip(leaf_start) {
            if cursor.advance_over(leaf.metrics, safe_line) {
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

    /// Nodes, leaves and pieces share the same two metric-only fast paths.
    /// A span ending on the target line must still be scanned to locate its start.
    fn advance_over(&mut self, metrics: PieceTreeMetrics, target_line: usize) -> bool {
        if self.current_line < target_line && self.current_line + metrics.newlines < target_line {
            self.current_line += metrics.newlines;
        } else if self.current_line == target_line && metrics.newlines == 0 {
            self.current_len += metrics.chars;
        } else {
            return false;
        }
        self.current_char += metrics.chars;
        true
    }
}

fn scan_leaf_for_line_lookup(
    tree: &PieceTreeLite,
    leaf: &PieceTreeLeaf,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> Option<(usize, usize)> {
    for piece in &leaf.pieces {
        if cursor.advance_over(piece.metrics(), safe_line) {
            continue;
        }

        let piece_text = tree.piece_text(piece);
        let byte_start = apply_piece_line_sample(tree, piece, &piece_text, safe_line, cursor);
        if let Some(line_info) =
            scan_piece_for_line_lookup(&piece_text[byte_start..], safe_line, cursor)
        {
            return Some(line_info);
        }
    }
    None
}

fn apply_piece_line_sample(
    tree: &PieceTreeLite,
    piece: &Piece,
    piece_text: &str,
    safe_line: usize,
    cursor: &mut LineLookupCursor,
) -> usize {
    if piece.newline_count < LINE_SAMPLE_STRIDE {
        return 0;
    }
    let lines_needed = safe_line.saturating_sub(cursor.current_line);
    let samples = tree.line_samples_for_piece(piece, piece_text);
    let sample_count = (lines_needed / LINE_SAMPLE_STRIDE).min(samples.len());
    if sample_count == 0 {
        return 0;
    }

    let sample = samples[sample_count - 1];
    cursor.current_line += sample_count * LINE_SAMPLE_STRIDE;
    cursor.current_char += sample.char_offset as usize + 1;
    cursor.line_start = cursor.current_char;
    cursor.current_len = 0;
    sample.byte_offset as usize + 1
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
