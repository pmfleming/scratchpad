mod keyboard;
mod mouse;

use super::{CharCursor, CursorRange};
use crate::app::domain::{BufferState, EditorViewState, RevealRequest};
use crate::app::ui::scrolling::DisplayMap;
use eframe::egui;

pub(super) fn handle_mouse_interaction_window(
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

pub(super) fn handle_keyboard_events_display_map(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    display_map: &DisplayMap,
    page_jump_rows: usize,
    total_chars: usize,
) -> bool {
    keyboard::handle_keyboard_events_display_map(
        ui,
        buffer,
        view,
        display_map,
        page_jump_rows,
        total_chars,
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
    view.request_reveal(RevealRequest::KeepVisible);
}

fn restore_pending_cursor(view: &mut EditorViewState, cursor_range: CursorRange) {
    view.cursor_range = Some(cursor_range);
    view.request_reveal(view.reveal_request().unwrap_or(RevealRequest::Center));
}
