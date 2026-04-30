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
    .or_else(|| vertical_movement_target(key, modifiers, galley, &egui_cursor))
    .or_else(|| row_edge_movement_target(key, modifiers, galley, &egui_cursor))
    .or_else(|| page_movement_target(key, galley, egui_cursor, page_jump_rows))?;
    let new_primary_char = clamp_char_cursor(galley, total_chars, new_primary, char_offset_base);
    Some(finalize_cursor_movement(
        cursor,
        key,
        modifiers,
        new_primary_char,
    ))
}

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

fn vertical_movement_target(
    key: egui::Key,
    modifiers: &egui::Modifiers,
    galley: &egui::Galley,
    egui_cursor: &egui::text::CCursor,
) -> Option<egui::text::CCursor> {
    match key {
        egui::Key::ArrowUp if modifiers.command => Some(galley.begin()),
        egui::Key::ArrowUp => Some(galley.cursor_up_one_row(egui_cursor, None).0),
        egui::Key::ArrowDown if modifiers.command => Some(galley.end()),
        egui::Key::ArrowDown => Some(galley.cursor_down_one_row(egui_cursor, None).0),
        _ => None,
    }
}

fn row_edge_movement_target(
    key: egui::Key,
    modifiers: &egui::Modifiers,
    galley: &egui::Galley,
    egui_cursor: &egui::text::CCursor,
) -> Option<egui::text::CCursor> {
    match key {
        egui::Key::Home if modifiers.command => Some(galley.begin()),
        egui::Key::Home => Some(galley.cursor_begin_of_row(egui_cursor)),
        egui::Key::End if modifiers.command => Some(galley.end()),
        egui::Key::End => Some(galley.cursor_end_of_row(egui_cursor)),
        _ => None,
    }
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
    use super::apply_cursor_movement;
    use crate::app::domain::buffer::PieceTreeLite;
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
    use eframe::egui;

    fn galley_for(text: &str) -> std::sync::Arc<egui::Galley> {
        let ctx = egui::Context::default();
        let mut galley = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            galley = Some(ui.fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text.to_owned(),
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    f32::INFINITY,
                ))
            }));
        });
        galley.expect("galley")
    }

    #[test]
    fn arrow_left_clamps_stale_cursor_before_moving() {
        let text = "abc";
        let galley = galley_for(text);
        let piece_tree = PieceTreeLite::from_string(text.to_owned());
        let stale_cursor = CursorRange::one(CharCursor {
            index: 99,
            prefer_next_row: true,
        });

        let moved = apply_cursor_movement(
            &stale_cursor,
            egui::Key::ArrowLeft,
            &egui::Modifiers::default(),
            &galley,
            10,
            piece_tree.len_chars(),
            &piece_tree,
            0,
            piece_tree.len_chars(),
        )
        .expect("left movement");

        assert_eq!(moved.primary.index, 2);
        assert_eq!(moved.secondary.index, 2);
    }
}
