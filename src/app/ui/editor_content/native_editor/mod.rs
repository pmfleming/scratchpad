mod cursor;
mod editing;
mod highlighting;
mod interactions;
mod types;
mod word_boundary;

pub use highlighting::build_layouter;
pub use types::{
    CharCursor, CursorRange, EditOperation, EditorHighlightStyle, LayouterFn, OperationRecord,
    TextEditOptions,
};

use crate::app::domain::{
    BufferState, EditorViewState, RenderedLayout, RenderedTextWindow, SearchHighlightState,
};
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_keyboard_events_unwrapped, handle_mouse_interaction,
    handle_mouse_interaction_window, sync_view_cursor_before_render,
};
use std::sync::Arc;

const VISIBLE_ROW_OVERSCAN: usize = 2;
const EDITOR_FOCUS_LOCK_FILTER: egui::EventFilter = egui::EventFilter {
    horizontal_arrows: true,
    vertical_arrows: true,
    tab: false,
    escape: false,
};

pub struct EditorWidgetOutcome {
    pub changed: bool,
    pub focused: bool,
    pub request_editor_focus: bool,
    pub response: egui::Response,
}

// ---------------------------------------------------------------------------
// Public rendering entry points
// ---------------------------------------------------------------------------

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
) -> EditorWidgetOutcome {
    let text = buffer.document().extract_text();
    let total_chars = buffer.document().piece_tree().len_chars();
    let wrap_width = editor_wrap_width(ui, options.word_wrap);

    let galley = highlighting::build_galley(
        ui,
        &text,
        options,
        &view.search_highlights,
        buffer.active_selection.clone(),
        wrap_width,
    );

    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let (rect, response) = ui.allocate_exact_size(
        editor_desired_size(ui, wrap_width, row_height, buffer.line_count.max(1)),
        egui::Sense::click_and_drag(),
    );
    request_editor_focus(ui, &response, options.request_focus);

    let prev_cursor = view.cursor_range;
    handle_mouse_interaction(
        ui,
        &response,
        &galley,
        rect,
        view,
        buffer.document().piece_tree(),
    );

    let focused = response.has_focus() || response.gained_focus() || options.request_focus;
    sync_view_cursor_before_render(view, focused);

    let changed = if focused {
        handle_keyboard_events(ui, buffer, view, &galley, total_chars)
    } else {
        false
    };

    if view.cursor_range != prev_cursor {
        view.scroll_to_cursor = true;
    }

    // Publish active view's selection to the buffer so all views can show it
    publish_active_selection(buffer, view, focused);

    let galley_pos = rect.min;
    if ui.is_rect_visible(rect) {
        paint_editor(
            ui, &galley, galley_pos, rect, view, options, focused, changed,
        );
    }

    // Consume scroll flag once the galley is fresh (scroll was applied)
    if !changed {
        view.scroll_to_cursor = false;
    }

    update_visible_layout(&galley, galley_pos, rect, buffer, view);

    if changed {
        buffer.refresh_text_metadata();
    }

    view.editor_has_focus = focused;

    EditorWidgetOutcome {
        changed,
        focused,
        request_editor_focus: false,
        response,
    }
}

pub fn render_editor_visible_text_window(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
) -> Option<EditorWidgetOutcome> {
    if options.word_wrap || options.request_focus {
        return None;
    }

    let visible_lines = previous_layout?.visible_line_range();
    if visible_lines.is_empty() {
        return None;
    }

    let visible_window = buffer.visible_line_window(visible_lines);
    Some(render_visible_text_window(
        ui,
        None,
        view,
        visible_window,
        options,
        buffer.line_count,
        buffer.active_selection.as_ref(),
    ))
}

pub fn render_editor_focused_text_window(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
) -> Option<EditorWidgetOutcome> {
    if options.word_wrap || options.request_focus {
        return None;
    }

    let visible_lines = focused_visible_line_range(buffer, view, previous_layout?)?;
    let visible_window = buffer.visible_line_window(visible_lines);
    let line_count = buffer.line_count;
    let active_selection = buffer.active_selection.clone();
    Some(render_visible_text_window(
        ui,
        Some(buffer),
        view,
        visible_window,
        options,
        line_count,
        active_selection.as_ref(),
    ))
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    text: String,
    desired_rows: usize,
    options: TextEditOptions<'_>,
) -> EditorWidgetOutcome {
    let selection_range = view
        .cursor_range
        .as_ref()
        .and_then(types::selection_char_range);

    let wrap_width = if options.word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    let galley = highlighting::build_galley(
        ui,
        &text,
        options,
        &view.search_highlights,
        selection_range,
        wrap_width,
    );

    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let desired_height = desired_rows.max(1) as f32 * row_height;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let focused = response.has_focus() || response.gained_focus();
    view.latest_layout = Some(RenderedLayout::from_galley(galley));
    view.cursor_range = None;
    view.editor_has_focus = focused;
    EditorWidgetOutcome {
        changed: false,
        focused,
        request_editor_focus: false,
        response,
    }
}

pub fn select_all_cursor(total_chars: usize) -> CursorRange {
    CursorRange::two(0, total_chars)
}

pub fn selected_text(buffer: &BufferState, cursor: CursorRange) -> Option<String> {
    let range = types::selection_char_range(&cursor)?;
    Some(buffer.document().piece_tree().extract_range(range))
}

pub fn cut_selected_text(
    buffer: &mut BufferState,
    cursor: CursorRange,
) -> Option<(CursorRange, String)> {
    (!cursor.is_empty()).then(|| editing::apply_cut(buffer, &cursor))
}

// ---------------------------------------------------------------------------
// Private: painting
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn paint_editor(
    ui: &mut egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    view: &EditorViewState,
    options: TextEditOptions<'_>,
    focused: bool,
    changed: bool,
) {
    let painter = ui.painter_at(rect.expand(1.0));

    // Paint galley — selection highlight is already baked into the LayoutJob
    paint_galley(ui, galley, galley_pos, options.text_color);

    if !focused {
        return;
    }

    if let Some(cursor_range) = &view.cursor_range
        && !changed
    {
        // Paint cursor (skip when changed — galley is stale, next frame corrects it)
        let cursor_rect = cursor_rect_at(galley, galley_pos, cursor_range.primary);
        let stroke = ui.visuals().text_cursor.stroke;
        painter.line_segment(
            [cursor_rect.center_top(), cursor_rect.center_bottom()],
            (stroke.width, stroke.color),
        );

        // Scroll to cursor only when it moved
        if view.scroll_to_cursor {
            ui.scroll_to_rect(cursor_rect, None);
        }

        // IME output
        let to_global = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();
        ui.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                rect: to_global * rect,
                cursor_rect: to_global * cursor_rect,
            });
        });
    }
}

fn paint_galley(
    ui: &egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    text_color: egui::Color32,
) {
    let offset = galley_pos - egui::vec2(galley.rect.left(), 0.0);
    ui.painter().galley(offset, galley.clone(), text_color);
}

fn cursor_rect_at(galley: &egui::Galley, galley_pos: egui::Pos2, cursor: CharCursor) -> egui::Rect {
    galley
        .pos_from_cursor(cursor.to_egui_ccursor())
        .expand(1.5)
        .translate(galley_pos.to_vec2())
}

// ---------------------------------------------------------------------------
// Private: visible text window
// ---------------------------------------------------------------------------

fn render_visible_text_window(
    ui: &mut egui::Ui,
    buffer: Option<&mut BufferState>,
    view: &mut EditorViewState,
    mut visible_window: RenderedTextWindow,
    options: TextEditOptions<'_>,
    total_line_count: usize,
    active_selection: Option<&std::ops::Range<usize>>,
) -> EditorWidgetOutcome {
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let top_padding_lines = visible_window.layout_row_offset;
    let bottom_padding_lines = total_line_count.saturating_sub(visible_window.line_range.end);

    if top_padding_lines > 0 {
        ui.add_space(row_height * top_padding_lines as f32);
    }

    // Map buffer-level selection into window-local char offsets
    let window_selection = window_selection(active_selection, &visible_window.char_range);
    let wrap_width = editor_wrap_width(ui, options.word_wrap);
    let galley = highlighting::build_galley(
        ui,
        &visible_window.text,
        options,
        &SearchHighlightState::default(),
        window_selection,
        wrap_width,
    );

    let desired_height = visible_window.line_range.len().max(1) as f32 * row_height;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        if buffer.is_some() {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        },
    );

    let (focused, request_editor_focus) = if let Some(buffer) = buffer {
        request_editor_focus(ui, &response, options.request_focus);

        let prev_cursor = view.cursor_range;
        handle_mouse_interaction_window(
            ui,
            &response,
            &galley,
            rect,
            view,
            buffer.document().piece_tree(),
            visible_window.char_range.start,
        );

        let focused = response.has_focus() || response.gained_focus() || options.request_focus;
        sync_view_cursor_before_render(view, focused);

        let changed = if focused {
            handle_keyboard_events_unwrapped(
                ui,
                buffer,
                view,
                buffer.document().piece_tree().len_chars(),
            )
        } else {
            false
        };

        if view.cursor_range != prev_cursor {
            view.scroll_to_cursor = true;
        }

        publish_active_selection(buffer, view, focused);

        if changed {
            buffer.refresh_text_metadata();
        }

        (Some((focused, changed)), false)
    } else {
        (
            None,
            apply_visible_window_click_focus(ui, &response, &galley, rect, view, &visible_window),
        )
    };

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
        if let Some((focused, changed)) = focused
            && focused
            && !changed
            && let Some(cursor_range) = view.cursor_range
            && let Some(local_cursor) = local_cursor_in_window(
                cursor_range.primary,
                visible_window.char_range.start,
                visible_window.char_range.end,
            )
        {
            paint_window_cursor(ui, &galley, rect, local_cursor, view);
        }
    }

    let mut latest_layout = Some(RenderedLayout::from_galley(galley));
    if let Some(layout) = latest_layout.as_mut() {
        layout.offset_line_numbers(visible_window.line_range.start);
        visible_window.row_range = 0..layout.row_count();
        layout.set_visible_text(visible_window);
    }
    view.latest_layout = latest_layout;
    view.editor_has_focus = focused.is_some_and(|(focused, _)| focused);

    if bottom_padding_lines > 0 {
        ui.add_space(row_height * bottom_padding_lines as f32);
    }

    EditorWidgetOutcome {
        changed: focused.is_some_and(|(_, changed)| changed),
        focused: focused.is_some_and(|(focused, _)| focused),
        request_editor_focus,
        response,
    }
}

fn apply_visible_window_click_focus(
    ui: &egui::Ui,
    response: &egui::Response,
    galley: &Arc<egui::Galley>,
    rect: egui::Rect,
    view: &mut EditorViewState,
    visible_window: &RenderedTextWindow,
) -> bool {
    if !response.clicked() {
        return false;
    }

    let Some(pointer_pos) = response.interact_pointer_pos() else {
        return true;
    };

    let clicked = galley.cursor_from_pos(pointer_pos - rect.min);
    let buffer_index =
        (visible_window.char_range.start + clicked.index).min(visible_window.char_range.end);
    let clicked_cursor = CharCursor {
        index: buffer_index,
        prefer_next_row: clicked.prefer_next_row,
    };

    let shift = ui.input(|input| input.modifiers.shift);
    let next_cursor = if shift {
        view.cursor_range
            .map(|existing| CursorRange {
                primary: clicked_cursor,
                secondary: existing.secondary,
            })
            .unwrap_or_else(|| CursorRange::one(clicked_cursor))
    } else {
        CursorRange::one(clicked_cursor)
    };

    view.cursor_range = Some(next_cursor);
    view.pending_cursor_range = Some(next_cursor);
    view.scroll_to_cursor = true;
    true
}

// ---------------------------------------------------------------------------
// Private: layout helpers
// ---------------------------------------------------------------------------

fn update_visible_layout(
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
) {
    let visible_row_range = visible_row_range_for_galley(galley, galley_pos, rect);

    let mut latest_layout = Some(RenderedLayout::from_galley(galley.clone()));
    if let (Some(layout), Some(visible_row_range)) = (latest_layout.as_mut(), visible_row_range)
        && let Some(char_range) = layout.char_range_for_rows(visible_row_range.clone())
    {
        let visible_text =
            buffer.visible_text_window(visible_row_range, char_range, layout.row_count());
        layout.set_visible_text(visible_text);
    }
    view.latest_layout = latest_layout;
}

fn editor_wrap_width(ui: &egui::Ui, word_wrap: bool) -> f32 {
    if word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    }
}

fn editor_desired_size(
    ui: &egui::Ui,
    wrap_width: f32,
    row_height: f32,
    desired_rows: usize,
) -> egui::Vec2 {
    let visible_height = ui.available_height();
    let desired_height = desired_rows as f32 * row_height + visible_height * 0.5;
    egui::vec2(
        wrap_width.min(ui.available_width()),
        desired_height.max(visible_height),
    )
}

fn request_editor_focus(ui: &mut egui::Ui, response: &egui::Response, request_focus: bool) {
    if request_focus {
        response.request_focus();
    }
    if response.has_focus() {
        ui.memory_mut(|mem| mem.set_focus_lock_filter(response.id, EDITOR_FOCUS_LOCK_FILTER));
    }
}

fn publish_active_selection(buffer: &mut BufferState, view: &EditorViewState, focused: bool) {
    if focused {
        buffer.active_selection = view
            .cursor_range
            .as_ref()
            .and_then(types::selection_char_range);
    }
}

fn window_selection(
    active_selection: Option<&std::ops::Range<usize>>,
    char_range: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    active_selection.and_then(|selection| {
        let start = selection
            .start
            .max(char_range.start)
            .saturating_sub(char_range.start);
        let end = selection
            .end
            .min(char_range.end)
            .saturating_sub(char_range.start);
        (start < end).then_some(start..end)
    })
}

fn focused_visible_line_range(
    buffer: &BufferState,
    view: &EditorViewState,
    previous_layout: &RenderedLayout,
) -> Option<std::ops::Range<usize>> {
    let previous = previous_layout.visible_line_range();
    if previous.is_empty() {
        return None;
    }

    let window_len = previous.len().max(1);
    let max_line = buffer.line_count.max(1);
    let mut start = previous.start.min(max_line.saturating_sub(1));
    let end = (start + window_len).min(max_line);
    if end <= start {
        return None;
    }

    let cursor = view.pending_cursor_range.or(view.cursor_range)?;
    let cursor_line = buffer
        .document()
        .piece_tree()
        .char_position(cursor.primary.index)
        .line_index;
    let overscan = 2usize;

    if cursor_line < start.saturating_add(overscan) {
        start = cursor_line.saturating_sub(overscan);
    } else if cursor_line + overscan >= end {
        start = cursor_line
            .saturating_add(overscan + 1)
            .saturating_sub(window_len)
            .min(max_line.saturating_sub(window_len.max(1)));
    }

    Some(start..(start + window_len).min(max_line))
}

fn local_cursor_in_window(
    cursor: CharCursor,
    char_start: usize,
    char_end: usize,
) -> Option<CharCursor> {
    (cursor.index >= char_start && cursor.index <= char_end).then_some(CharCursor {
        index: cursor.index.saturating_sub(char_start),
        prefer_next_row: cursor.prefer_next_row,
    })
}

fn paint_window_cursor(
    ui: &mut egui::Ui,
    galley: &Arc<egui::Galley>,
    rect: egui::Rect,
    local_cursor: CharCursor,
    view: &EditorViewState,
) {
    let painter = ui.painter_at(rect.expand(1.0));
    let cursor_rect = cursor_rect_at(galley, rect.min, local_cursor);
    let stroke = ui.visuals().text_cursor.stroke;
    painter.line_segment(
        [cursor_rect.center_top(), cursor_rect.center_bottom()],
        (stroke.width, stroke.color),
    );

    if view.scroll_to_cursor {
        ui.scroll_to_rect(cursor_rect, None);
    }

    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    ui.output_mut(|o| {
        o.ime = Some(egui::output::IMEOutput {
            rect: to_global * rect,
            cursor_rect: to_global * cursor_rect,
        });
    });
}

fn visible_row_range_for_galley(
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    clip_rect: egui::Rect,
) -> Option<std::ops::Range<usize>> {
    let first_visible = galley
        .rows
        .iter()
        .position(|row| galley_pos.y + row.max_y() >= clip_rect.top())?;
    let last_visible = galley
        .rows
        .iter()
        .rposition(|row| galley_pos.y + row.min_y() <= clip_rect.bottom())
        .unwrap_or(first_visible);
    let start = first_visible.saturating_sub(VISIBLE_ROW_OVERSCAN);
    let end = (last_visible + 1 + VISIBLE_ROW_OVERSCAN).min(galley.rows.len());
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::{CharCursor, CursorRange, sync_view_cursor_before_render};
    use crate::app::domain::EditorViewState;

    #[test]
    fn focused_editor_without_cursor_starts_at_document_beginning() {
        let mut view = EditorViewState::new(1, false);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(
            view.cursor_range,
            Some(CursorRange::one(CharCursor::new(0)))
        );
        assert!(view.scroll_to_cursor);
    }

    #[test]
    fn pending_cursor_range_overrides_missing_native_editor_cursor() {
        let mut view = EditorViewState::new(1, false);
        let pending = CursorRange::one(CharCursor::new(7));
        view.pending_cursor_range = Some(pending);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(view.cursor_range, Some(pending));
        assert_eq!(view.pending_cursor_range, None);
        assert!(view.scroll_to_cursor);
    }
}
