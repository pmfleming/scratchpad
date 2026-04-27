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

pub(super) fn handle_mouse_interaction(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
    rect: egui::Rect,
    view: &mut EditorViewState,
    piece_tree: &crate::app::domain::buffer::PieceTreeLite,
    char_offset_base: usize,
) {
    let context = MouseInteractionContext {
        response,
        galley,
        rect,
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
        .cursor_from_pos(pointer_pos - context.rect.min);
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
    interact_pointer_pos.or_else(|| latest_pointer_pos.filter(|pos| is_dragged || rect.contains(*pos)))
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
    if ui
        .input(|input| input.pointer.hover_pos().is_some_and(|pos| rect.contains(pos)))
    {
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
        || (ui.input(|input| input.pointer.latest_pos().is_some_and(|pos| rect.contains(pos)))
            && ui.input(|input| input.pointer.button_down(egui::PointerButton::Secondary)))
}

fn is_primary_pointer_down(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect) -> bool {
    primary_pointer_tracking_active(
        ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary)),
        ui.input(|input| input.pointer.latest_pos().is_some_and(|pos| rect.contains(pos))),
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

#[cfg(test)]
mod tests {
    use super::{
        ClickSelectionContext, PointerSelection, apply_click_selection, char_cursor_with_offset,
        cursor_range_after_click, extend_selection_to_cursor, line_selection_range,
        normalize_click_count, primary_pointer_tracking_active, tracked_pointer_pos,
    };
    use crate::app::domain::{EditorViewState, buffer::PieceTreeLite};
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

    fn cursor_range_after_click_with_modifiers(
        current: Option<CursorRange>,
        clicked_cursor: CharCursor,
        modifiers: egui::Modifiers,
    ) -> CursorRange {
        let ctx = egui::Context::default();
        let mut range = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.input_mut(|input| input.modifiers = modifiers);
            range = Some(cursor_range_after_click(ui, current, clicked_cursor));
        });
        range.expect("cursor range")
    }

    fn apply_click_selection_with_count(
        text: &str,
        click_count: u32,
        char_cursor: CharCursor,
        cursor_at_pointer: egui::text::CCursor,
        char_offset_base: usize,
    ) -> Option<CursorRange> {
        let ctx = egui::Context::default();
        let piece_tree = PieceTreeLite::from_string(text.to_owned());
        let galley = galley_for(text);
        let selection_context = ClickSelectionContext {
            piece_tree: &piece_tree,
            galley: galley.as_ref(),
            char_offset_base,
        };
        let mut view = EditorViewState::new(1, false);

        let _ = ctx.run_ui(Default::default(), |ui| {
            apply_click_selection(
                ui,
                &mut view,
                selection_context,
                PointerSelection {
                    cursor_at_pointer,
                    char_cursor,
                    click_count,
                },
            );
        });

        view.cursor_range
    }

    #[test]
    fn end_of_row_clicks_do_not_promote_to_multi_click_selection() {
        let galley = galley_for("abc\ndef");
        let end_of_first_row = egui::text::CCursor::new(3);

        assert_eq!(normalize_click_count(&galley, end_of_first_row, 3), 1);
        assert_eq!(normalize_click_count(&galley, end_of_first_row, 2), 1);
    }

    #[test]
    fn interior_clicks_preserve_multi_click_count() {
        let galley = galley_for("abc\ndef");
        let interior = egui::text::CCursor::new(1);

        assert_eq!(normalize_click_count(&galley, interior, 2), 2);
        assert_eq!(normalize_click_count(&galley, interior, 3), 3);
    }

    #[test]
    fn primary_pointer_tracking_continues_for_active_drag_outside_response() {
        assert!(primary_pointer_tracking_active(true, false, true));
        assert!(primary_pointer_tracking_active(true, true, false));
        assert!(!primary_pointer_tracking_active(false, true, true));
        assert!(!primary_pointer_tracking_active(true, false, false));
    }

    #[test]
    fn tracked_pointer_pos_falls_back_to_latest_position_for_active_drag() {
        let latest = egui::pos2(24.0, 48.0);
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(32.0, 64.0));

        assert_eq!(tracked_pointer_pos(None, Some(latest), rect, true), Some(latest));
        assert_eq!(tracked_pointer_pos(Some(latest), None, rect, false), Some(latest));
        assert_eq!(tracked_pointer_pos(None, Some(latest), rect, false), Some(latest));
        assert_eq!(
            tracked_pointer_pos(None, Some(egui::pos2(48.0, 96.0)), rect, false),
            None
        );
    }

    #[test]
    fn char_cursor_with_offset_preserves_prefer_next_row() {
        let cursor = egui::text::CCursor {
            index: 5,
            prefer_next_row: true,
        };

        let adjusted = char_cursor_with_offset(cursor, 7);

        assert_eq!(adjusted.index, 12);
        assert!(adjusted.prefer_next_row);
    }

    #[test]
    fn single_click_replaces_existing_selection_without_shift() {
        let current = Some(CursorRange::two(2, 8));

        let result = cursor_range_after_click_with_modifiers(
            current,
            CharCursor::new(5),
            egui::Modifiers::default(),
        );

        assert_eq!(result, CursorRange::one(CharCursor::new(5)));
    }

    #[test]
    fn shift_click_preserves_existing_anchor() {
        let current = Some(CursorRange::two(2, 8));

        let result = cursor_range_after_click_with_modifiers(
            current,
            CharCursor::new(11),
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );

        assert_eq!(
            result,
            CursorRange {
                primary: CharCursor::new(11),
                secondary: CharCursor::new(2),
            }
        );
    }

    #[test]
    fn double_click_selects_word_boundaries() {
        let result = apply_click_selection_with_count(
            "one two three",
            2,
            CharCursor::new(5),
            egui::text::CCursor::new(5),
            0,
        );

        assert_eq!(result, Some(CursorRange::two(4, 7)));
    }

    #[test]
    fn triple_click_selects_full_line_with_window_offset() {
        let galley = galley_for("zero\none\ntwo\n");
        let cursor_at_pointer = egui::text::CCursor::new(6);
        let row_start = galley.cursor_begin_of_row(&cursor_at_pointer);
        let row_end = galley.cursor_end_of_row(&cursor_at_pointer);
        let char_offset_base = 100;

        let result = line_selection_range(
            ClickSelectionContext {
                piece_tree: &PieceTreeLite::from_string("zero\none\ntwo\n".to_owned()),
                galley: galley.as_ref(),
                char_offset_base,
            },
            cursor_at_pointer,
        );

        assert_eq!(
            result,
            CursorRange {
                primary: CharCursor {
                    index: char_offset_base + row_end.index,
                    prefer_next_row: row_end.prefer_next_row,
                },
                secondary: CharCursor {
                    index: char_offset_base + row_start.index,
                    prefer_next_row: row_start.prefer_next_row,
                },
            }
        );
    }

    #[test]
    fn dragging_extends_existing_selection_to_new_cursor() {
        let mut view = EditorViewState::new(1, false);
        view.cursor_range = Some(CursorRange::two(2, 8));

        extend_selection_to_cursor(&mut view, CharCursor::new(12));

        assert_eq!(
            view.cursor_range,
            Some(CursorRange {
                primary: CharCursor::new(12),
                secondary: CharCursor::new(2),
            })
        );
    }
}
