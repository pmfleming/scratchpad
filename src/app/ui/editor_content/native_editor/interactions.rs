mod keyboard;
mod mouse;

use super::layout::DisplayTextMap;
use super::{CharCursor, CursorRange};
use crate::app::domain::buffer::PieceTreeLite;
use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};
use eframe::egui;

pub(super) struct MouseInteractionRequest<'a> {
    pub(super) response: &'a egui::Response,
    pub(super) galley: &'a egui::Galley,
    pub(super) rect: egui::Rect,
    pub(super) galley_pos: egui::Pos2,
    pub(super) piece_tree: &'a PieceTreeLite,
    pub(super) char_offset_base: usize,
    pub(super) display_map: Option<&'a DisplayTextMap>,
}

pub(super) struct KeyboardInputRequest<'a> {
    pub(super) galley: &'a egui::Galley,
    pub(super) page_jump_rows: usize,
    pub(super) total_chars: usize,
    pub(super) char_offset_base: usize,
    pub(super) slice_chars: usize,
    pub(super) display_map: Option<&'a DisplayTextMap>,
}

pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    request: MouseInteractionRequest<'_>,
) {
    mouse::handle_mouse_interaction(ui, view, request);
}

pub(super) fn handle_keyboard_events(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: KeyboardInputRequest<'_>,
) -> bool {
    keyboard::handle_keyboard_events(ui, buffer, view, request)
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

pub(super) fn page_jump_rows(viewport: Option<egui::Rect>, row_height: f32) -> usize {
    viewport
        .and_then(|viewport| viewport_line_capacity(viewport, row_height))
        .unwrap_or(1)
}

fn restore_pending_cursor(view: &mut EditorViewState, cursor_range: CursorRange) {
    view.cursor_range = Some(cursor_range);
    view.request_cursor_reveal(
        view.cursor_reveal_mode()
            .unwrap_or(CursorRevealMode::Center),
    );
}

pub(super) fn viewport_line_capacity(viewport: egui::Rect, row_height: f32) -> Option<usize> {
    if row_height <= 0.0 || viewport.max.y <= viewport.min.y {
        return None;
    }

    Some(
        ((viewport.max.y - viewport.min.y) / row_height)
            .ceil()
            .max(1.0) as usize,
    )
}
