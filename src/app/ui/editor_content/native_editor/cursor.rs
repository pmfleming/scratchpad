use super::types::{CharCursor, CursorRange};
use super::word_boundary;
use crate::app::domain::buffer::PieceTreeLite;
use crate::app::ui::editor_content::native_editor::layout::DisplayTextMap;
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

pub(super) struct CursorMovementRequest<'a> {
    pub(super) cursor: &'a CursorRange,
    pub(super) key: egui::Key,
    pub(super) modifiers: &'a egui::Modifiers,
    pub(super) galley: &'a egui::Galley,
    pub(super) page_jump_rows: usize,
    pub(super) total_chars: usize,
    pub(super) piece_tree: &'a PieceTreeLite,
    pub(super) char_offset_base: usize,
    pub(super) slice_chars: usize,
    pub(super) display_map: Option<&'a DisplayTextMap>,
}

struct HorizontalMovementContext<'a> {
    current_index: usize,
    char_offset_base: usize,
    slice_chars: usize,
    key: egui::Key,
    modifiers: &'a egui::Modifiers,
    galley: &'a egui::Galley,
    egui_cursor: &'a egui::text::CCursor,
    piece_tree: &'a PieceTreeLite,
    display_map: Option<&'a DisplayTextMap>,
}

pub(super) fn apply_cursor_movement(request: CursorMovementRequest<'_>) -> Option<CursorRange> {
    let doc_local_cursor = request
        .cursor
        .primary
        .index
        .saturating_sub(request.char_offset_base)
        .min(request.slice_chars);
    let local_cursor = CharCursor {
        index: request
            .display_map
            .map(|map| map.doc_to_display_cursor(doc_local_cursor))
            .unwrap_or(doc_local_cursor),
        prefer_next_row: request.cursor.primary.prefer_next_row,
    };
    let egui_cursor = request.galley.clamp_cursor(&local_cursor.to_egui_ccursor());
    let new_primary = horizontal_movement_target(HorizontalMovementContext {
        current_index: request.cursor.primary.index,
        char_offset_base: request.char_offset_base,
        slice_chars: request.slice_chars,
        key: request.key,
        modifiers: request.modifiers,
        galley: request.galley,
        egui_cursor: &egui_cursor,
        piece_tree: request.piece_tree,
        display_map: request.display_map,
    })
    .map(|target| {
        clamp_char_cursor(
            request.galley,
            request.total_chars,
            target,
            request.char_offset_base,
            request.display_map,
        )
    })
    .or_else(|| {
        full_document_movement_target(
            request.cursor.primary.index,
            request.key,
            request.modifiers,
            request.page_jump_rows,
            request.total_chars,
            request.piece_tree,
        )
    })
    .or_else(|| {
        page_movement_target(
            request.key,
            request.galley,
            egui_cursor,
            request.page_jump_rows,
        )
        .map(|target| {
            clamp_char_cursor(
                request.galley,
                request.total_chars,
                target,
                request.char_offset_base,
                request.display_map,
            )
        })
    })?;
    Some(finalize_cursor_movement(
        request.cursor,
        request.key,
        request.modifiers,
        new_primary,
    ))
}

fn horizontal_movement_target(
    context: HorizontalMovementContext<'_>,
) -> Option<egui::text::CCursor> {
    match context.key {
        egui::Key::ArrowLeft if is_wordwise_movement(context.modifiers) => {
            Some(local_cursor_for_document_index(
                word_boundary::find_word_boundary_left(context.piece_tree, context.current_index),
                context.char_offset_base,
                context.slice_chars,
                context.display_map,
            ))
        }
        egui::Key::ArrowLeft => Some(
            context
                .galley
                .cursor_left_one_character(context.egui_cursor),
        ),
        egui::Key::ArrowRight if is_wordwise_movement(context.modifiers) => {
            Some(local_cursor_for_document_index(
                word_boundary::find_word_boundary_right(context.piece_tree, context.current_index),
                context.char_offset_base,
                context.slice_chars,
                context.display_map,
            ))
        }
        egui::Key::ArrowRight => Some(
            context
                .galley
                .cursor_right_one_character(context.egui_cursor),
        ),
        _ => None,
    }
}

fn local_cursor_for_document_index(
    index: usize,
    char_offset_base: usize,
    slice_chars: usize,
    display_map: Option<&DisplayTextMap>,
) -> egui::text::CCursor {
    let local = index.saturating_sub(char_offset_base).min(slice_chars);
    egui::text::CCursor::new(
        display_map
            .map(|map| map.doc_to_display_cursor(local))
            .unwrap_or(local),
    )
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
    display_map: Option<&DisplayTextMap>,
) -> CharCursor {
    let clamped = galley.clamp_cursor(&cursor);
    let local_index = display_map
        .map(|map| map.display_to_doc_cursor(clamped.index))
        .unwrap_or(clamped.index);
    CharCursor {
        index: char_offset_base
            .saturating_add(local_index)
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
