mod keyboard;
mod mouse;

use super::{CharCursor, CursorRange};
use crate::app::domain::{BufferState, EditorViewState};
use eframe::egui;

pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
) {
    mouse::handle_mouse_interaction(ui, response, galley, rect, view, piece_tree);
}

pub(super) fn handle_mouse_interaction_window(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
) {
    mouse::handle_mouse_interaction_window(
        ui,
        response,
        galley,
        rect,
        view,
        piece_tree,
        char_offset_base,
    );
}

pub(super) fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    galley: &egui::Galley,
    total_chars: usize,
) -> bool {
    keyboard::handle_keyboard_events(ui, buffer, view, galley, total_chars)
}

pub(super) fn handle_keyboard_events_unwrapped(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    total_chars: usize,
) -> bool {
    keyboard::handle_keyboard_events_unwrapped(ui, buffer, view, total_chars)
}

pub(super) fn sync_view_cursor_before_render(view: &mut EditorViewState, focused: bool) {
    if let Some(cursor_range) = view.pending_cursor_range.take() {
        view.cursor_range = Some(cursor_range);
        view.scroll_to_cursor = true;
    } else if focused && view.cursor_range.is_none() {
        view.cursor_range = Some(CursorRange::one(CharCursor::new(0)));
        view.scroll_to_cursor = true;
    }
}
