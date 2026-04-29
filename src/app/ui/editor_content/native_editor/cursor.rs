use super::types::{CharCursor, CursorRange};
use super::word_boundary;
use crate::app::domain::buffer::PieceTreeLite;
use crate::app::ui::scrolling::{DisplayMap, DisplayRow};
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

pub(super) fn apply_cursor_movement_display_map(
    cursor: &CursorRange,
    key: egui::Key,
    modifiers: &egui::Modifiers,
    display_map: &DisplayMap,
    page_jump_rows: usize,
    total_chars: usize,
    piece_tree: &PieceTreeLite,
) -> Option<CursorRange> {
    let current = cursor.primary.index.min(total_chars);
    let new_primary = match key {
        egui::Key::ArrowLeft => {
            if is_wordwise_movement(modifiers) {
                Some(CharCursor::new(word_boundary::find_word_boundary_left(
                    piece_tree, current,
                )))
            } else {
                Some(CharCursor::new(current.saturating_sub(1)))
            }
        }
        egui::Key::ArrowRight => {
            if is_wordwise_movement(modifiers) {
                Some(CharCursor::new(word_boundary::find_word_boundary_right(
                    piece_tree, current,
                )))
            } else {
                Some(CharCursor::new((current + 1).min(total_chars)))
            }
        }
        egui::Key::ArrowUp => {
            if modifiers.command {
                Some(CharCursor::new(0))
            } else {
                move_display_rows(display_map, current, -(1_i32))
            }
        }
        egui::Key::ArrowDown => {
            if modifiers.command {
                Some(CharCursor::new(total_chars))
            } else {
                move_display_rows(display_map, current, 1)
            }
        }
        egui::Key::Home => {
            if modifiers.command {
                Some(CharCursor::new(0))
            } else {
                row_for_char(display_map, current)
                    .and_then(|row| display_map.row(row))
                    .map(|row| CharCursor::new(row.char_range.start.min(total_chars)))
            }
        }
        egui::Key::End => {
            if modifiers.command {
                Some(CharCursor::new(total_chars))
            } else {
                row_for_char(display_map, current)
                    .and_then(|row| display_map.row(row))
                    .map(|row| CharCursor::new(row.char_range.end.min(total_chars)))
            }
        }
        egui::Key::PageUp => move_display_rows(
            display_map,
            current,
            -(i32::try_from(page_jump_rows.max(1)).unwrap_or(i32::MAX)),
        ),
        egui::Key::PageDown => move_display_rows(
            display_map,
            current,
            i32::try_from(page_jump_rows.max(1)).unwrap_or(i32::MAX),
        ),
        _ => None,
    }?;

    Some(finalize_cursor_movement(
        cursor,
        key,
        modifiers,
        CharCursor {
            index: new_primary.index.min(total_chars),
            prefer_next_row: new_primary.prefer_next_row,
        },
    ))
}

fn row_for_char(display_map: &DisplayMap, char_index: usize) -> Option<DisplayRow> {
    display_map.display_row_for_char(char_index)
}

fn move_display_rows(
    display_map: &DisplayMap,
    char_index: usize,
    delta_rows: i32,
) -> Option<CharCursor> {
    let current_row = row_for_char(display_map, char_index)?;
    let current_span = display_map.row(current_row)?;
    let offset_in_row = char_index.saturating_sub(current_span.char_range.start);
    let target_row = if delta_rows < 0 {
        current_row.0.saturating_sub(delta_rows.unsigned_abs())
    } else {
        current_row
            .0
            .saturating_add(delta_rows as u32)
            .min(display_map.row_count().saturating_sub(1))
    };
    let target_span = display_map.row(DisplayRow(target_row))?;
    let target_len = target_span
        .char_range
        .end
        .saturating_sub(target_span.char_range.start);
    Some(CharCursor::new(
        target_span.char_range.start + offset_in_row.min(target_len),
    ))
}

#[cfg(test)]
mod tests {
    use super::apply_cursor_movement_display_map;
    use crate::app::domain::buffer::PieceTreeLite;
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
    use crate::app::ui::scrolling::DisplayMap;
    use eframe::egui;

    fn display_map_for(tree: &PieceTreeLite) -> DisplayMap {
        let ctx = egui::Context::default();
        let mut map = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            map = Some(
                DisplayMap::from_piece_tree(
                    ui,
                    tree,
                    1,
                    &egui::FontId::monospace(14.0),
                    false,
                    f32::INFINITY,
                    18.0,
                )
                .expect("display map"),
            );
        });
        map.expect("display map")
    }

    #[test]
    fn arrow_left_clamps_stale_cursor_before_moving() {
        let text = "abc";
        let piece_tree = PieceTreeLite::from_string(text.to_owned());
        let display_map = display_map_for(&piece_tree);
        let stale_cursor = CursorRange::one(CharCursor {
            index: 99,
            prefer_next_row: true,
        });

        let moved = apply_cursor_movement_display_map(
            &stale_cursor,
            egui::Key::ArrowLeft,
            &egui::Modifiers::default(),
            &display_map,
            10,
            piece_tree.len_chars(),
            &piece_tree,
        )
        .expect("left movement");

        assert_eq!(moved.primary.index, 2);
        assert_eq!(moved.secondary.index, 2);
    }
}
