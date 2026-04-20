use super::types::{CharCursor, CursorRange};
use crate::app::domain::buffer::PieceTreeLite;
use eframe::egui;

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
                Some(egui::text::CCursor::new(find_word_boundary_left(
                    piece_tree,
                    cursor.primary.index,
                )))
            } else {
                Some(galley.cursor_left_one_character(&egui_cursor))
            }
        }
        egui::Key::ArrowRight => {
            if modifiers.alt || modifiers.ctrl {
                Some(egui::text::CCursor::new(find_word_boundary_right(
                    piece_tree,
                    cursor.primary.index,
                )))
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

fn find_word_boundary_left(piece_tree: &PieceTreeLite, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let text = piece_tree.extract_range(0..index);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = chars.len();
    while pos > 0 && chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    while pos > 0 && !chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    pos
}

fn find_word_boundary_right(piece_tree: &PieceTreeLite, index: usize) -> usize {
    let total = piece_tree.len_chars();
    if index >= total {
        return total;
    }
    let text = piece_tree.extract_range(index..total);
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    while pos < chars.len() && !chars[pos].is_whitespace() {
        pos += 1;
    }
    while pos < chars.len() && chars[pos].is_whitespace() {
        pos += 1;
    }
    index + pos
}
