use super::super::{CharCursor, CursorRange, word_boundary};
use crate::app::domain::EditorViewState;
use eframe::egui;

const MULTI_CLICK_MAX_DELAY: f64 = 0.4;
const MULTI_CLICK_MAX_DISTANCE: f32 = 4.0;

#[derive(Clone, Default)]
struct ClickState {
    last_click_time: f64,
    last_click_pos: egui::Pos2,
    click_count: u32,
    was_primary_pointer_down: bool,
}

#[derive(Clone, Copy)]
struct WindowClickSelection {
    cursor_at_pointer: egui::text::CCursor,
    char_cursor: CharCursor,
    char_offset_base: usize,
}

pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
) {
    handle_mouse_interaction_with_offset(ui, response, galley, rect, view, piece_tree, 0, false);
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
    handle_mouse_interaction_with_offset(
        ui,
        response,
        galley,
        rect,
        view,
        piece_tree,
        char_offset_base,
        true,
    );
}

fn extend_selection_to_cursor(view: &mut EditorViewState, char_cursor: CharCursor) {
    if let Some(existing) = &view.cursor_range {
        view.cursor_range = Some(CursorRange {
            primary: char_cursor,
            secondary: existing.secondary,
        });
    }
}

fn update_click_count(ui: &egui::Ui, pointer_pos: egui::Pos2, click_state: &mut ClickState) {
    let now = ui.input(|input| input.time);
    let is_repeat = (now - click_state.last_click_time) < MULTI_CLICK_MAX_DELAY
        && (pointer_pos - click_state.last_click_pos).length() < MULTI_CLICK_MAX_DISTANCE;

    click_state.click_count = if is_repeat {
        click_state.click_count + 1
    } else {
        1
    };
    click_state.last_click_time = now;
    click_state.last_click_pos = pointer_pos;
}

fn apply_click_selection(
    ui: &egui::Ui,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    galley: &egui::Galley,
    cursor_at_pointer: egui::text::CCursor,
    char_cursor: CharCursor,
    click_count: u32,
) {
    match click_count {
        2 => {
            let start = word_boundary::word_start(piece_tree, char_cursor.index);
            let end = word_boundary::word_end(piece_tree, char_cursor.index);
            view.cursor_range = Some(CursorRange::two(start, end));
        }
        n if n >= 3 => {
            let row_start = galley.cursor_begin_of_row(&cursor_at_pointer);
            let row_end = galley.cursor_end_of_row(&cursor_at_pointer);
            view.cursor_range = Some(CursorRange {
                primary: CharCursor {
                    index: row_end.index,
                    prefer_next_row: row_end.prefer_next_row,
                },
                secondary: CharCursor {
                    index: row_start.index,
                    prefer_next_row: row_start.prefer_next_row,
                },
            });
        }
        _ => apply_single_click(ui, view, char_cursor),
    }
}

fn apply_click_selection_window(
    ui: &egui::Ui,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    galley: &egui::Galley,
    click_count: u32,
    selection: WindowClickSelection,
) {
    match click_count {
        2 => {
            let start = word_boundary::word_start(piece_tree, selection.char_cursor.index);
            let end = word_boundary::word_end(piece_tree, selection.char_cursor.index);
            view.cursor_range = Some(CursorRange::two(start, end));
        }
        n if n >= 3 => {
            let row_start = galley.cursor_begin_of_row(&selection.cursor_at_pointer);
            let row_end = galley.cursor_end_of_row(&selection.cursor_at_pointer);
            view.cursor_range = Some(CursorRange {
                primary: CharCursor {
                    index: selection.char_offset_base + row_end.index,
                    prefer_next_row: row_end.prefer_next_row,
                },
                secondary: CharCursor {
                    index: selection.char_offset_base + row_start.index,
                    prefer_next_row: row_start.prefer_next_row,
                },
            });
        }
        _ => apply_single_click(ui, view, selection.char_cursor),
    }
}

fn apply_single_click(ui: &egui::Ui, view: &mut EditorViewState, char_cursor: CharCursor) {
    let shift = ui.input(|input| input.modifiers.shift);
    if shift {
        extend_selection_to_cursor(view, char_cursor);
        if view.cursor_range.is_none() {
            view.cursor_range = Some(CursorRange::one(char_cursor));
        }
    } else {
        view.cursor_range = Some(CursorRange::one(char_cursor));
    }
}

fn handle_mouse_interaction_with_offset(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
    windowed: bool,
) {
    update_hover_cursor(ui, response);

    let Some(pointer_pos) = response.interact_pointer_pos() else {
        return;
    };

    let cursor_at_pointer = galley.cursor_from_pos(pointer_pos - rect.min);
    let char_cursor = CharCursor {
        index: char_offset_base + cursor_at_pointer.index,
        prefer_next_row: cursor_at_pointer.prefer_next_row,
    };
    let click_id = response.id.with("click_state");
    let mut click_state = load_click_state(ui, click_id);

    if should_ignore_secondary_pointer(ui, response) {
        click_state.was_primary_pointer_down = false;
        store_click_state(ui, click_id, click_state);
        return;
    }

    let primary_pointer_down = is_primary_pointer_down(ui, response);
    if primary_pointer_down && response.dragged() {
        extend_selection_to_cursor(view, char_cursor);
    } else if primary_pointer_down && !click_state.was_primary_pointer_down {
        update_click_count(ui, pointer_pos, &mut click_state);
        apply_pointer_selection(
            ui,
            view,
            piece_tree,
            galley,
            cursor_at_pointer,
            char_cursor,
            WindowClickSelection {
                cursor_at_pointer,
                char_cursor,
                char_offset_base,
            },
            windowed,
            click_state.click_count,
        );
    }

    click_state.was_primary_pointer_down = primary_pointer_down;
    store_click_state(ui, click_id, click_state);

    if primary_pointer_down {
        response.request_focus();
    }
}

fn update_hover_cursor(ui: &mut egui::Ui, response: &egui::Response) {
    if response.hovered() {
        ui.output_mut(|output| output.mutable_text_under_cursor = true);
        ui.set_cursor_icon(egui::CursorIcon::Text);
    }
}

fn load_click_state(ui: &mut egui::Ui, click_id: egui::Id) -> ClickState {
    ui.data_mut(|data| data.get_temp(click_id))
        .unwrap_or_default()
}

fn store_click_state(ui: &mut egui::Ui, click_id: egui::Id, click_state: ClickState) {
    ui.data_mut(|data| data.insert_temp(click_id, click_state));
}

fn should_ignore_secondary_pointer(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.secondary_clicked()
        || (response.contains_pointer()
            && ui.input(|input| input.pointer.button_down(egui::PointerButton::Secondary)))
}

fn is_primary_pointer_down(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.contains_pointer()
        && ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
}

fn apply_pointer_selection(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    galley: &egui::Galley,
    cursor_at_pointer: egui::text::CCursor,
    char_cursor: CharCursor,
    window_selection: WindowClickSelection,
    windowed: bool,
    click_count: u32,
) {
    if !windowed {
        apply_click_selection(
            ui,
            view,
            piece_tree,
            galley,
            cursor_at_pointer,
            char_cursor,
            click_count,
        );
        return;
    }

    apply_click_selection_window(ui, view, piece_tree, galley, click_count, window_selection);
}
