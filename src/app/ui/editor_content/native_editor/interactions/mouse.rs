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
struct MouseInteractionContext<'a> {
    response: &'a egui::Response,
    galley: &'a egui::Galley,
    rect: egui::Rect,
    galley_pos: egui::Pos2,
    piece_tree: &'a crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
}

#[derive(Clone, Copy)]
struct ClickSelectionContext<'a> {
    piece_tree: &'a crate::app::domain::buffer::PieceTreeLite,
    galley: &'a egui::Galley,
    char_offset_base: usize,
}

#[derive(Clone, Copy)]
struct PointerSelection {
    cursor_at_pointer: egui::text::CCursor,
    char_cursor: CharCursor,
    click_count: u32,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    galley_pos: egui::Pos2,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
) {
    let context = MouseInteractionContext {
        response,
        galley,
        rect,
        galley_pos,
        piece_tree,
        char_offset_base,
    };
    update_hover_cursor(ui, context.rect);

    let Some((pointer_pos, selection)) = pointer_selection(ui, context) else {
        return;
    };
    let selection_context = ClickSelectionContext {
        piece_tree: context.piece_tree,
        galley: context.galley,
        char_offset_base: context.char_offset_base,
    };
    let click_id = context.response.id.with("click_state");
    let mut click_state = load_click_state(ui, click_id);

    if should_ignore_secondary_pointer(ui, context.response, context.rect) {
        click_state.was_primary_pointer_down = false;
        store_click_state(ui, click_id, click_state);
        return;
    }

    let primary_pointer_down = is_primary_pointer_down(ui, context.response, context.rect);
    handle_primary_pointer(
        ui,
        view,
        context.response,
        selection_context,
        selection,
        pointer_pos,
        &mut click_state,
        primary_pointer_down,
    );

    click_state.was_primary_pointer_down = primary_pointer_down;
    store_click_state(ui, click_id, click_state);

    if primary_pointer_down {
        context.response.request_focus();
    }
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
    context: ClickSelectionContext<'_>,
    selection: PointerSelection,
) {
    let click_count = normalize_click_count(
        context.galley,
        selection.cursor_at_pointer,
        selection.click_count,
    );

    view.cursor_range = Some(match click_count {
        2 => {
            let start = word_boundary::word_start(context.piece_tree, selection.char_cursor.index);
            let end = word_boundary::word_end(context.piece_tree, selection.char_cursor.index);
            CursorRange::two(start, end)
        }
        n if n >= 3 => line_selection_range(context, selection.cursor_at_pointer),
        _ => cursor_range_after_click(ui, view.cursor_range, selection.char_cursor),
    });
}

fn line_selection_range(
    context: ClickSelectionContext<'_>,
    cursor_at_pointer: egui::text::CCursor,
) -> CursorRange {
    let row_start = context.galley.cursor_begin_of_row(&cursor_at_pointer);
    let row_end = context.galley.cursor_end_of_row(&cursor_at_pointer);
    CursorRange {
        primary: char_cursor_with_offset(row_end, context.char_offset_base),
        secondary: char_cursor_with_offset(row_start, context.char_offset_base),
    }
}

pub(super) fn cursor_range_after_click(
    ui: &egui::Ui,
    current: Option<CursorRange>,
    clicked_cursor: CharCursor,
) -> CursorRange {
    let shift_pressed = ui.input(|input| input.modifiers.shift);
    let Some(existing) = current.filter(|_| shift_pressed) else {
        return CursorRange::one(clicked_cursor);
    };

    CursorRange {
        primary: clicked_cursor,
        secondary: existing.secondary,
    }
}

fn normalize_click_count(
    galley: &egui::Galley,
    cursor_at_pointer: egui::text::CCursor,
    click_count: u32,
) -> u32 {
    let row_end = galley.cursor_end_of_row(&cursor_at_pointer);
    if cursor_at_pointer.index == row_end.index {
        1
    } else {
        click_count
    }
}

fn pointer_selection(
    ui: &egui::Ui,
    context: MouseInteractionContext<'_>,
) -> Option<(egui::Pos2, PointerSelection)> {
    let pointer_pos = tracked_pointer_pos(
        context.response.interact_pointer_pos(),
        ui.input(|input| input.pointer.latest_pos()),
        context.rect,
        context.response.dragged_by(egui::PointerButton::Primary),
    )?;
    let cursor_at_pointer = context
        .galley
        .cursor_from_pos(pointer_pos - context.galley_pos);
    let char_cursor = char_cursor_with_offset(cursor_at_pointer, context.char_offset_base);

    Some((
        pointer_pos,
        PointerSelection {
            cursor_at_pointer,
            char_cursor,
            click_count: 1,
        },
    ))
}

fn tracked_pointer_pos(
    interact_pointer_pos: Option<egui::Pos2>,
    latest_pointer_pos: Option<egui::Pos2>,
    rect: egui::Rect,
    is_dragged: bool,
) -> Option<egui::Pos2> {
    interact_pointer_pos
        .or_else(|| latest_pointer_pos.filter(|pos| is_dragged || rect.contains(*pos)))
}

fn char_cursor_with_offset(cursor: egui::text::CCursor, char_offset_base: usize) -> CharCursor {
    CharCursor {
        index: char_offset_base + cursor.index,
        prefer_next_row: cursor.prefer_next_row,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_primary_pointer(
    ui: &egui::Ui,
    view: &mut EditorViewState,
    response: &egui::Response,
    selection_context: ClickSelectionContext<'_>,
    mut selection: PointerSelection,
    pointer_pos: egui::Pos2,
    click_state: &mut ClickState,
    primary_pointer_down: bool,
) {
    if !primary_pointer_down {
        return;
    }

    if response.dragged() {
        extend_selection_to_cursor(view, selection.char_cursor);
        return;
    }

    if click_state.was_primary_pointer_down {
        return;
    }

    update_click_count(ui, pointer_pos, click_state);
    selection.click_count = click_state.click_count;
    apply_click_selection(ui, view, selection_context, selection);
}

fn update_hover_cursor(ui: &mut egui::Ui, rect: egui::Rect) {
    if ui.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| rect.contains(pos))
    }) {
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

fn should_ignore_secondary_pointer(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
) -> bool {
    response.secondary_clicked()
        || (ui.input(|input| {
            input
                .pointer
                .latest_pos()
                .is_some_and(|pos| rect.contains(pos))
        }) && ui.input(|input| input.pointer.button_down(egui::PointerButton::Secondary)))
}

fn is_primary_pointer_down(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect) -> bool {
    primary_pointer_tracking_active(
        ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary)),
        ui.input(|input| {
            input
                .pointer
                .latest_pos()
                .is_some_and(|pos| rect.contains(pos))
        }),
        response.dragged_by(egui::PointerButton::Primary),
    )
}

fn primary_pointer_tracking_active(
    primary_button_down: bool,
    contains_pointer: bool,
    is_dragged: bool,
) -> bool {
    primary_button_down && (contains_pointer || is_dragged)
}
