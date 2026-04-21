use super::types::{CharCursor, CursorRange};
use super::word_boundary;
use crate::app::domain::buffer::PieceTreeLite;
use eframe::egui;

const PAGE_JUMP_ROWS: usize = 30;

pub(super) fn apply_cursor_movement(
    cursor: &CursorRange,
    key: egui::Key,
    modifiers: &egui::Modifiers,
    galley: &egui::Galley,
    total_chars: usize,
    piece_tree: &PieceTreeLite,
) -> Option<CursorRange> {
    let egui_cursor = galley.clamp_cursor(&cursor.primary.to_egui_ccursor());

    let new_primary = match key {
        egui::Key::ArrowLeft => {
            if modifiers.alt || modifiers.ctrl {
                Some(egui::text::CCursor::new(
                    word_boundary::find_word_boundary_left(piece_tree, cursor.primary.index),
                ))
            } else {
                Some(galley.cursor_left_one_character(&egui_cursor))
            }
        }
        egui::Key::ArrowRight => {
            if modifiers.alt || modifiers.ctrl {
                Some(egui::text::CCursor::new(
                    word_boundary::find_word_boundary_right(piece_tree, cursor.primary.index),
                ))
            } else {
                Some(galley.cursor_right_one_character(&egui_cursor))
            }
        }
        egui::Key::ArrowUp => {
            if modifiers.command {
                Some(galley.begin())
            } else {
                Some(galley.cursor_up_one_row(&egui_cursor, None).0)
            }
        }
        egui::Key::ArrowDown => {
            if modifiers.command {
                Some(galley.end())
            } else {
                Some(galley.cursor_down_one_row(&egui_cursor, None).0)
            }
        }
        egui::Key::Home => {
            if modifiers.command {
                Some(galley.begin())
            } else {
                Some(galley.cursor_begin_of_row(&egui_cursor))
            }
        }
        egui::Key::End => {
            if modifiers.command {
                Some(galley.end())
            } else {
                Some(galley.cursor_end_of_row(&egui_cursor))
            }
        }
        egui::Key::PageUp => {
            let mut c = egui_cursor;
            for _ in 0..PAGE_JUMP_ROWS {
                c = galley.cursor_up_one_row(&c, None).0;
            }
            Some(c)
        }
        egui::Key::PageDown => {
            let mut c = egui_cursor;
            for _ in 0..PAGE_JUMP_ROWS {
                c = galley.cursor_down_one_row(&c, None).0;
            }
            Some(c)
        }
        _ => None,
    };

    let new_primary = new_primary?;
    let new_primary_char = clamp_char_cursor(galley, total_chars, new_primary);

    if modifiers.shift {
        return Some(CursorRange {
            primary: new_primary_char,
            secondary: cursor.secondary,
        });
    }

    // When collapsing selection on arrow keys
    if !cursor.is_empty()
        && (key == egui::Key::ArrowLeft || key == egui::Key::ArrowRight)
        && !modifiers.alt
        && !modifiers.ctrl
        && !modifiers.command
    {
        let (start, end) = cursor.sorted_indices();
        return if key == egui::Key::ArrowLeft {
            Some(CursorRange::one(clamp_char_cursor(
                galley,
                total_chars,
                egui::text::CCursor::new(start),
            )))
        } else {
            Some(CursorRange::one(clamp_char_cursor(
                galley,
                total_chars,
                egui::text::CCursor::new(end),
            )))
        };
    }

    Some(CursorRange::one(new_primary_char))
}

fn clamp_char_cursor(
    galley: &egui::Galley,
    total_chars: usize,
    cursor: egui::text::CCursor,
) -> CharCursor {
    let clamped = galley.clamp_cursor(&cursor);
    CharCursor {
        index: clamped.index.min(total_chars),
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
            piece_tree.len_chars(),
            &piece_tree,
        )
        .expect("left movement");

        assert_eq!(moved.primary.index, 2);
        assert_eq!(moved.secondary.index, 2);
    }
}
