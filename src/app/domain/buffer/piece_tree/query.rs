use super::{PieceTreeCharPosition, PieceTreeLite};

pub(crate) fn char_position(tree: &PieceTreeLite, offset_chars: usize) -> PieceTreeCharPosition {
    let safe_offset = offset_chars.min(tree.len_chars());
    let line_index = tree.line_index_at_offset(safe_offset);
    let line_info = tree.line_info(line_index);
    PieceTreeCharPosition {
        offset_chars: safe_offset,
        line_index,
        column_index: safe_offset.saturating_sub(line_info.start_char),
    }
}
