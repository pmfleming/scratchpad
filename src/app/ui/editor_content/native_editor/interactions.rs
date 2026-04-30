mod keyboard;
mod mouse;

use super::{CharCursor, CursorRange};
use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};
use eframe::egui;

pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
) {
    mouse::handle_mouse_interaction(
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
    page_jump_rows: usize,
    total_chars: usize,
    char_offset_base: usize,
    slice_chars: usize,
) -> bool {
    keyboard::handle_keyboard_events(
        ui,
        buffer,
        view,
        galley,
        page_jump_rows,
        total_chars,
        char_offset_base,
        slice_chars,
    )
}

pub(super) fn sync_view_cursor_before_render(view: &mut EditorViewState, focused: bool) {
    if let Some(cursor_range) = view.pending_cursor_range.take() {
        restore_pending_cursor(view, cursor_range);
        return;
    }

    if !focused || view.cursor_range.is_some() {
        return;
    }

    view.cursor_range = Some(CursorRange::one(CharCursor::new(0)));
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);
}

fn restore_pending_cursor(view: &mut EditorViewState, cursor_range: CursorRange) {
    view.cursor_range = Some(cursor_range);
    view.request_cursor_reveal(
        view.cursor_reveal_mode()
            .unwrap_or(CursorRevealMode::Center),
    );
}
