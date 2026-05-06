use super::types::{CharCursor, CursorRange};
use super::word_boundary;
use crate::app::domain::buffer::PieceTreeLite;
use eframe::egui;

fn is_wordwise_movement(modifiers: &egui::Modifiers) -> bool {
    modifiers.alt || modifiers.ctrl
}

fn collapsed_selection_target(
    cursor: &CursorRange,
    key: egui::Key,
    modifiers: &egui::Modifiers,
) -> Option<usize> {
    if cursor.is_empty()
        || (key != egui::Key::ArrowLeft && key != egui::Key::ArrowRight)
        || is_wordwise_movement(modifiers)
        || modifiers.command
    {
        return None;
    }

    let (start, end) = cursor.sorted_indices();
    Some(if key == egui::Key::ArrowLeft {
        start
    } else {
        end
    })
}

fn finalize_cursor_movement(
    cursor: &CursorRange,
    key: egui::Key,
    modifiers: &egui::Modifiers,
    new_primary: CharCursor,
) -> CursorRange {
    if modifiers.shift {
        return CursorRange {
            primary: new_primary,
            secondary: cursor.secondary,
        };
    }

    if let Some(index) = collapsed_selection_target(cursor, key, modifiers) {
        return CursorRange::one(CharCursor::new(index));
    }

    CursorRange::one(new_primary)
}

fn move_by_page_rows(
    galley: &egui::Galley,
    cursor: egui::text::CCursor,
    page_jump_rows: usize,
    downward: bool,
) -> egui::text::CCursor {
    let mut cursor = cursor;
    for _ in 0..page_jump_rows.max(1) {
        cursor = if downward {
            galley.cursor_down_one_row(&cursor, None).0
        } else {
            galley.cursor_up_one_row(&cursor, None).0
        };
    }
    cursor
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_cursor_movement(
    cursor: &CursorRange,
    key: egui::Key,
    modifiers: &egui::Modifiers,
    galley: &egui::Galley,
    page_jump_rows: usize,
    total_chars: usize,
    piece_tree: &PieceTreeLite,
    char_offset_base: usize,
    slice_chars: usize,
) -> Option<CursorRange> {
    let local_cursor = CharCursor {
        index: cursor
            .primary
            .index
            .saturating_sub(char_offset_base)
            .min(slice_chars),
        prefer_next_row: cursor.primary.prefer_next_row,
    };
    let egui_cursor = galley.clamp_cursor(&local_cursor.to_egui_ccursor());
    let new_primary = horizontal_movement_target(
        cursor.primary.index,
        char_offset_base,
        slice_chars,
        key,
        modifiers,
        galley,
        &egui_cursor,
        piece_tree,
    )
    .map(|target| clamp_char_cursor(galley, total_chars, target, char_offset_base))
    .or_else(|| {
        full_document_movement_target(
            cursor.primary.index,
            key,
            modifiers,
            page_jump_rows,
            total_chars,
            piece_tree,
        )
    })
    .or_else(|| {
        page_movement_target(key, galley, egui_cursor, page_jump_rows)
            .map(|target| clamp_char_cursor(galley, total_chars, target, char_offset_base))
    })?;
    Some(finalize_cursor_movement(
        cursor,
        key,
        modifiers,
        new_primary,
    ))
}

#[allow(clippy::too_many_arguments)]
fn horizontal_movement_target(
    current_index: usize,
    char_offset_base: usize,
    slice_chars: usize,
    key: egui::Key,
    modifiers: &egui::Modifiers,
    galley: &egui::Galley,
    egui_cursor: &egui::text::CCursor,
    piece_tree: &PieceTreeLite,
) -> Option<egui::text::CCursor> {
    match key {
        egui::Key::ArrowLeft if is_wordwise_movement(modifiers) => {
            Some(local_cursor_for_document_index(
                word_boundary::find_word_boundary_left(piece_tree, current_index),
                char_offset_base,
                slice_chars,
            ))
        }
        egui::Key::ArrowLeft => Some(galley.cursor_left_one_character(egui_cursor)),
        egui::Key::ArrowRight if is_wordwise_movement(modifiers) => {
            Some(local_cursor_for_document_index(
                word_boundary::find_word_boundary_right(piece_tree, current_index),
                char_offset_base,
                slice_chars,
            ))
        }
        egui::Key::ArrowRight => Some(galley.cursor_right_one_character(egui_cursor)),
        _ => None,
    }
}

fn local_cursor_for_document_index(
    index: usize,
    char_offset_base: usize,
    slice_chars: usize,
) -> egui::text::CCursor {
    egui::text::CCursor::new(index.saturating_sub(char_offset_base).min(slice_chars))
}

fn full_document_movement_target(
    current_index: usize,
    key: egui::Key,
    modifiers: &egui::Modifiers,
    page_jump_rows: usize,
    total_chars: usize,
    piece_tree: &PieceTreeLite,
) -> Option<CharCursor> {
    match key {
        egui::Key::ArrowUp if modifiers.command => None,
        egui::Key::ArrowUp => {
            logical_line_movement_target(current_index, -1, total_chars, piece_tree)
        }
        egui::Key::ArrowDown if modifiers.command => None,
        egui::Key::ArrowDown => {
            logical_line_movement_target(current_index, 1, total_chars, piece_tree)
        }
        egui::Key::Home if modifiers.command => Some(CharCursor::new(0)),
        egui::Key::Home => Some(CharCursor::new(current_line_start(
            current_index,
            piece_tree,
        ))),
        egui::Key::End if modifiers.command => Some(CharCursor::new(total_chars)),
        egui::Key::End => Some(CharCursor::new(current_line_content_end(
            current_index,
            total_chars,
            piece_tree,
        ))),
        egui::Key::PageUp => logical_line_movement_target(
            current_index,
            -(page_jump_rows.max(1) as isize),
            total_chars,
            piece_tree,
        ),
        egui::Key::PageDown => logical_line_movement_target(
            current_index,
            page_jump_rows.max(1) as isize,
            total_chars,
            piece_tree,
        ),
        _ => None,
    }
}

fn logical_line_movement_target(
    current_index: usize,
    line_delta: isize,
    total_chars: usize,
    piece_tree: &PieceTreeLite,
) -> Option<CharCursor> {
    if line_delta == 0 {
        return None;
    }

    let current_line = piece_tree.line_index_at_offset(current_index.min(total_chars));
    let target_line = current_line
        .saturating_add_signed(line_delta)
        .min(piece_tree.metrics().newlines);
    if target_line == current_line {
        return None;
    }

    let current_start = piece_tree.line_info(current_line).start_char;
    let target_info = piece_tree.line_info(target_line);
    let target_end = line_content_end(target_info.start_char, target_info.char_len, piece_tree);
    let column = current_index.saturating_sub(current_start);
    Some(CharCursor::new(
        target_info
            .start_char
            .saturating_add(column)
            .min(target_end)
            .min(total_chars),
    ))
}

fn current_line_start(current_index: usize, piece_tree: &PieceTreeLite) -> usize {
    piece_tree
        .line_info(piece_tree.line_index_at_offset(current_index))
        .start_char
}

fn current_line_content_end(
    current_index: usize,
    total_chars: usize,
    piece_tree: &PieceTreeLite,
) -> usize {
    let line_info = piece_tree.line_info(piece_tree.line_index_at_offset(current_index));
    line_content_end(line_info.start_char, line_info.char_len, piece_tree).min(total_chars)
}

fn line_content_end(start_char: usize, char_len: usize, piece_tree: &PieceTreeLite) -> usize {
    let mut end = start_char
        .saturating_add(char_len)
        .min(piece_tree.len_chars());
    if end > start_char && piece_tree.char_at(end - 1) == Some('\n') {
        end -= 1;
    }
    if end > start_char && piece_tree.char_at(end - 1) == Some('\r') {
        end -= 1;
    }
    end
}

fn page_movement_target(
    key: egui::Key,
    galley: &egui::Galley,
    egui_cursor: egui::text::CCursor,
    page_jump_rows: usize,
) -> Option<egui::text::CCursor> {
    match key {
        egui::Key::PageUp => Some(move_by_page_rows(
            galley,
            egui_cursor,
            page_jump_rows,
            false,
        )),
        egui::Key::PageDown => Some(move_by_page_rows(galley, egui_cursor, page_jump_rows, true)),
        _ => None,
    }
}

fn clamp_char_cursor(
    galley: &egui::Galley,
    total_chars: usize,
    cursor: egui::text::CCursor,
    char_offset_base: usize,
) -> CharCursor {
    let clamped = galley.clamp_cursor(&cursor);
    CharCursor {
        index: char_offset_base
            .saturating_add(clamped.index)
            .min(total_chars),
        prefer_next_row: clamped.prefer_next_row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(text: &str) -> PieceTreeLite {
        PieceTreeLite::from_string(text.to_owned())
    }

    #[test]
    fn arrow_down_moves_past_visible_slice_boundaries_by_document_line() {
        let tree = tree("alpha\nb\ncharlie\n");

        let target = logical_line_movement_target(1, 2, tree.len_chars(), &tree).unwrap();

        assert_eq!(target.index, 9);
    }

    #[test]
    fn arrow_down_clamps_to_shorter_target_line() {
        let tree = tree("alpha\nb\ncharlie\n");

        let target = logical_line_movement_target(4, 1, tree.len_chars(), &tree).unwrap();

        assert_eq!(target.index, 7);
    }

    #[test]
    fn page_down_uses_document_lines_not_galley_edges() {
        let tree = tree("one\ntwo\nthree\nfour\nfive\n");

        let target = full_document_movement_target(
            1,
            egui::Key::PageDown,
            &egui::Modifiers::default(),
            3,
            tree.len_chars(),
            &tree,
        )
        .unwrap();

        assert_eq!(target.index, 15);
    }

    #[test]
    fn end_stops_before_line_ending() {
        let tree = tree("alpha\r\nbeta\n");

        let target = full_document_movement_target(
            1,
            egui::Key::End,
            &egui::Modifiers::default(),
            1,
            tree.len_chars(),
            &tree,
        )
        .unwrap();

        assert_eq!(target.index, 5);
    }
}
