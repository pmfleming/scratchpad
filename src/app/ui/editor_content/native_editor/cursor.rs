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
    _total_chars: usize,
    piece_tree: &PieceTreeLite,
) -> Option<CursorRange> {
    let egui_cursor = cursor.primary.to_egui_ccursor();

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
    let new_primary_char = CharCursor {
        index: new_primary.index,
        prefer_next_row: new_primary.prefer_next_row,
    };

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
            Some(CursorRange::one(CharCursor::new(start)))
        } else {
            Some(CursorRange::one(CharCursor::new(end)))
        };
    }

    Some(CursorRange::one(new_primary_char))
}
