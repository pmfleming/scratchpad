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
    cursor_range_after_click, handle_keyboard_events, handle_keyboard_events_unwrapped,
    handle_mouse_interaction, handle_mouse_interaction_window, sync_view_cursor_before_render,
};
use std::ops::Range;
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
    pub requested_scroll_offset: Option<egui::Vec2>,
    pub response: egui::Response,
}

struct VisibleWindowInputState {
    focused: bool,
    changed: bool,
}

struct VisibleWindowRenderRequest<'a> {
    total_line_count: usize,
    active_selection: Option<&'a Range<usize>>,
}

// ---------------------------------------------------------------------------
// Public rendering entry points
// ---------------------------------------------------------------------------

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> EditorWidgetOutcome {
    let document_revision = buffer.document_revision();
    let total_chars = buffer.document().piece_tree().len_chars();
    let wrap_width = editor_wrap_width(ui, options.word_wrap);

    let galley = {
        let text = buffer.document().text_cow();
        highlighting::build_galley(
            ui,
            text.as_ref(),
            options,
            &view.search_highlights,
            buffer.active_selection.clone(),
            wrap_width,
        )
    };

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
    let page_jump_rows = page_jump_rows(viewport, row_height);

    let changed = if focused {
        handle_keyboard_events(ui, buffer, view, &galley, page_jump_rows, total_chars)
    } else {
        false
    };

    if view.cursor_range != prev_cursor {
        view.scroll_to_cursor = true;
    }

    // Publish active view's selection to the buffer so all views can show it
    publish_active_selection(buffer, view, focused);

    let input_requested_scroll_offset =
        page_navigation_requested_scroll_offset(ui, focused, Some(view), viewport, row_height);

    let galley_pos = rect.min;
    let requested_scroll_offset = if ui.is_rect_visible(rect) {
        paint_editor(
            ui, &galley, galley_pos, rect, view, options, focused, changed, viewport,
        )
    } else {
        None
    }
    .or(input_requested_scroll_offset);

    // Consume scroll flag once the galley is fresh (scroll was applied)
    if !changed {
        view.scroll_to_cursor = false;
    }

    if changed {
        view.latest_layout = None;
        view.latest_layout_revision = None;
    } else {
        update_visible_layout(&galley, galley_pos, rect, buffer, view, document_revision);
    }

    view.editor_has_focus = focused;

    EditorWidgetOutcome {
        changed,
        focused,
        request_editor_focus: false,
        requested_scroll_offset,
        response,
    }
}

pub fn render_editor_visible_text_window(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Option<EditorWidgetOutcome> {
    if options.word_wrap {
        return None;
    }

    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let visible_lines = viewport_visible_line_range(viewport, row_height, buffer.line_count)
        .or_else(|| Some(previous_layout?.visible_line_range()))?;
    render_read_only_visible_lines(ui, buffer, view, visible_lines, options, viewport)
}

pub fn render_editor_focused_text_window(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Option<EditorWidgetOutcome> {
    if options.word_wrap || options.request_focus {
        return None;
    }

    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let visible_lines = if view.scroll_to_cursor {
        previous_layout
            .and_then(|layout| focused_visible_line_range(buffer, view, layout))
            .or_else(|| cursor_visible_line_range(buffer, view, viewport, row_height))?
    } else {
        viewport_visible_line_range(viewport, row_height, buffer.line_count)
            .or_else(|| {
                previous_layout.and_then(|layout| focused_visible_line_range(buffer, view, layout))
            })
            .or_else(|| previous_layout.map(RenderedLayout::visible_line_range))?
    };
    render_editable_visible_lines(ui, buffer, view, visible_lines, options, viewport)
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
    view.latest_layout_revision = None;
    view.cursor_range = None;
    view.editor_has_focus = focused;
    EditorWidgetOutcome {
        changed: false,
        focused,
        request_editor_focus: false,
        requested_scroll_offset: None,
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

fn render_read_only_visible_lines(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    visible_lines: Range<usize>,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Option<EditorWidgetOutcome> {
    render_visible_line_window(
        ui,
        None,
        view,
        visible_line_window_for_range(buffer, visible_lines)?,
        options,
        viewport,
        VisibleWindowRenderRequest {
            total_line_count: buffer.line_count,
            active_selection: buffer.active_selection.as_ref(),
        },
    )
}

fn render_editable_visible_lines(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    visible_lines: Range<usize>,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Option<EditorWidgetOutcome> {
    let visible_window = visible_line_window_for_range(buffer, visible_lines)?;
    let line_count = buffer.line_count;
    let active_selection = buffer.active_selection.clone();
    render_visible_line_window(
        ui,
        Some(buffer),
        view,
        visible_window,
        options,
        viewport,
        VisibleWindowRenderRequest {
            total_line_count: line_count,
            active_selection: active_selection.as_ref(),
        },
    )
}

fn visible_line_window_for_range(
    buffer: &BufferState,
    visible_lines: Range<usize>,
) -> Option<RenderedTextWindow> {
    (!visible_lines.is_empty()).then(|| buffer.visible_line_window(visible_lines))
}

fn render_visible_line_window(
    ui: &mut egui::Ui,
    buffer: Option<&mut BufferState>,
    view: &mut EditorViewState,
    visible_window: RenderedTextWindow,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
    request: VisibleWindowRenderRequest<'_>,
) -> Option<EditorWidgetOutcome> {
    Some(render_visible_text_window(
        ui,
        buffer,
        view,
        visible_window,
        options,
        viewport,
        request.total_line_count,
        request.active_selection,
    ))
}

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
    viewport: Option<egui::Rect>,
) -> Option<egui::Vec2> {
    // Paint galley — selection highlight is already baked into the LayoutJob
    paint_galley(ui, galley, galley_pos, options.text_color);

    if !focused {
        return None;
    }

    if let Some(cursor_range) = &view.cursor_range
        && !changed
    {
        // Paint cursor (skip when changed — galley is stale, next frame corrects it)
        let cursor_rect = cursor_rect_at(galley, galley_pos, cursor_range.primary);
        return paint_cursor_effects(ui, rect, cursor_rect, view.scroll_to_cursor, view, viewport);
    }

    None
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

fn paint_cursor_effects(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cursor_rect: egui::Rect,
    scroll_to_cursor: bool,
    view: &EditorViewState,
    viewport: Option<egui::Rect>,
) -> Option<egui::Vec2> {
    paint_cursor(ui, rect, cursor_rect);
    publish_ime_output(ui, rect, cursor_rect);
    requested_scroll_offset_for_cursor(scroll_to_cursor, view, viewport, cursor_rect)
}

fn paint_cursor(ui: &egui::Ui, rect: egui::Rect, cursor_rect: egui::Rect) {
    let painter = ui.painter_at(rect.expand(1.0));
    let stroke = ui.visuals().text_cursor.stroke;
    painter.line_segment(
        [cursor_rect.center_top(), cursor_rect.center_bottom()],
        (stroke.width, stroke.color),
    );
}

fn publish_ime_output(ui: &mut egui::Ui, rect: egui::Rect, cursor_rect: egui::Rect) {
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

// ---------------------------------------------------------------------------
// Private: visible text window
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_visible_text_window(
    ui: &mut egui::Ui,
    buffer: Option<&mut BufferState>,
    view: &mut EditorViewState,
    visible_window: RenderedTextWindow,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
    total_line_count: usize,
    active_selection: Option<&std::ops::Range<usize>>,
) -> EditorWidgetOutcome {
    let document_revision = buffer.as_ref().map(|buffer| buffer.document_revision());
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let top_padding_lines = visible_window.layout_row_offset;
    let bottom_padding_lines = total_line_count.saturating_sub(visible_window.line_range.end);
    let is_editable = buffer.is_some();

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
        if is_editable {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        },
    );

    let (input_state, request_editor_focus) = if let Some(buffer) = buffer {
        (
            Some(handle_visible_window_buffer_input(
                ui,
                &response,
                &galley,
                rect,
                buffer,
                view,
                options,
                viewport,
                visible_window.char_range.start,
            )),
            false,
        )
    } else {
        (
            None,
            apply_visible_window_click_focus(ui, &response, &galley, rect, view, &visible_window),
        )
    };
    let focused = input_state.as_ref().is_some_and(|state| state.focused);
    let changed = input_state.as_ref().is_some_and(|state| state.changed);

    let input_requested_scroll_offset =
        page_navigation_requested_scroll_offset(ui, focused, Some(view), viewport, row_height);

    let requested_scroll_offset = if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
        if let Some(input_state) = input_state.as_ref()
            && focused
            && !input_state.changed
            && let Some(cursor_range) = view.cursor_range
            && let Some(local_cursor) = local_cursor_in_window(
                cursor_range.primary,
                visible_window.char_range.start,
                visible_window.char_range.end,
            )
        {
            paint_window_cursor(ui, &galley, rect, local_cursor, view, viewport)
        } else {
            None
        }
    } else {
        None
    }
    .or(input_requested_scroll_offset);

    if changed {
        clear_latest_layout(view);
    } else {
        set_latest_layout(
            view,
            Some(layout_with_visible_window(galley, Some(visible_window))),
            document_revision,
        );
    }
    view.editor_has_focus = focused;

    if bottom_padding_lines > 0 {
        ui.add_space(row_height * bottom_padding_lines as f32);
    }
    add_editor_trailing_scroll_padding(ui);

    EditorWidgetOutcome {
        changed,
        focused,
        request_editor_focus,
        requested_scroll_offset,
        response,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_visible_window_buffer_input(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &Arc<egui::Galley>,
    rect: egui::Rect,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
    window_start: usize,
) -> VisibleWindowInputState {
    request_editor_focus(ui, response, options.request_focus);

    let prev_cursor = view.cursor_range;
    handle_mouse_interaction_window(
        ui,
        response,
        galley,
        rect,
        view,
        buffer.document().piece_tree(),
        window_start,
    );

    let focused = response.has_focus() || response.gained_focus() || options.request_focus;
    sync_view_cursor_before_render(view, focused);
    let page_jump_rows = page_jump_rows(
        viewport,
        ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id)),
    );

    let changed = if focused {
        handle_keyboard_events_unwrapped(
            ui,
            buffer,
            view,
            page_jump_rows,
            buffer.document().piece_tree().len_chars(),
        )
    } else {
        false
    };

    if view.cursor_range != prev_cursor {
        view.scroll_to_cursor = true;
    }

    publish_active_selection(buffer, view, focused);

    VisibleWindowInputState { focused, changed }
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

    let next_cursor = cursor_range_after_click(ui, view.cursor_range, clicked_cursor);

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
    document_revision: u64,
) {
    let visible_row_range = visible_row_range_for_galley(galley, galley_pos, rect);
    let mut latest_layout = RenderedLayout::from_galley(galley.clone());
    let visible_window = visible_row_range.and_then(|visible_row_range| {
        latest_layout
            .char_range_for_rows(visible_row_range.clone())
            .map(|char_range| {
                buffer.visible_text_window(visible_row_range, char_range, latest_layout.row_count())
            })
    });
    if let Some(visible_window) = visible_window {
        latest_layout.set_visible_text(visible_window);
    }
    set_latest_layout(view, Some(latest_layout), Some(document_revision));
}

fn set_latest_layout(
    view: &mut EditorViewState,
    latest_layout: Option<RenderedLayout>,
    document_revision: Option<u64>,
) {
    view.latest_layout = latest_layout;
    view.latest_layout_revision = document_revision;
}

fn clear_latest_layout(view: &mut EditorViewState) {
    set_latest_layout(view, None, None);
}

fn layout_with_visible_window(
    galley: Arc<egui::Galley>,
    visible_window: Option<RenderedTextWindow>,
) -> RenderedLayout {
    let mut layout = RenderedLayout::from_galley(galley);
    if let Some(mut visible_window) = visible_window {
        layout.offset_line_numbers(visible_window.line_range.start);
        visible_window.row_range = 0..layout.row_count();
        layout.set_visible_text(visible_window);
    }
    layout
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

fn add_editor_trailing_scroll_padding(ui: &mut egui::Ui) {
    let trailing_padding = ui.available_height() * 0.5;
    if trailing_padding > 0.0 {
        ui.add_space(trailing_padding);
    }
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
) -> Option<Range<usize>> {
    let previous = previous_layout.visible_line_range();
    if previous.is_empty() {
        return None;
    }

    let window_len = previous.len().max(1);
    let max_line = buffer.line_count.max(1);
    let mut start = previous.start.min(max_line.saturating_sub(1));
    let end = line_window_end(start, window_len, max_line);
    if end <= start {
        return None;
    }

    let cursor_line = cursor_line(buffer, view)?;
    let overscan = 2usize;

    if cursor_line < start.saturating_add(overscan) {
        start = cursor_line.saturating_sub(overscan);
    } else if cursor_line + overscan >= end {
        start = cursor_line
            .saturating_add(overscan + 1)
            .saturating_sub(window_len)
            .min(max_line.saturating_sub(window_len.max(1)));
    }

    Some(line_window_range(start, window_len, max_line))
}

fn cursor_visible_line_range(
    buffer: &BufferState,
    view: &EditorViewState,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<Range<usize>> {
    let cursor_line = cursor_line(buffer, view)?;
    let visible_len = viewport
        .and_then(|viewport| viewport_line_capacity(viewport, row_height))
        .unwrap_or(48)
        .saturating_add(VISIBLE_ROW_OVERSCAN * 2)
        .max(1);
    let line_count = buffer.line_count.max(1);
    let start = cursor_line
        .saturating_sub(visible_len / 2)
        .min(line_count.saturating_sub(1));
    Some(line_window_range(start, visible_len, line_count))
}

fn cursor_line(buffer: &BufferState, view: &EditorViewState) -> Option<usize> {
    let cursor = view.pending_cursor_range.or(view.cursor_range)?;
    Some(
        buffer
            .document()
            .piece_tree()
            .char_position(cursor.primary.index)
            .line_index,
    )
}

fn line_window_range(start: usize, len: usize, max_line: usize) -> Range<usize> {
    start..line_window_end(start, len, max_line)
}

fn line_window_end(start: usize, len: usize, max_line: usize) -> usize {
    start.saturating_add(len).min(max_line)
}

fn viewport_line_capacity(viewport: egui::Rect, row_height: f32) -> Option<usize> {
    if row_height <= 0.0 || viewport.max.y <= viewport.min.y {
        return None;
    }

    Some(
        ((viewport.max.y - viewport.min.y) / row_height)
            .ceil()
            .max(1.0) as usize,
    )
}

fn viewport_visible_line_range(
    viewport: Option<egui::Rect>,
    row_height: f32,
    total_line_count: usize,
) -> Option<Range<usize>> {
    let viewport = viewport?;
    if row_height <= 0.0
        || !viewport.min.y.is_finite()
        || !viewport.max.y.is_finite()
        || viewport.max.y <= viewport.min.y
    {
        return None;
    }

    let line_count = total_line_count.max(1);
    let first_visible = (viewport.min.y / row_height).floor().max(0.0) as usize;
    let last_visible = (viewport.max.y / row_height).ceil().max(1.0) as usize;
    let start = first_visible
        .saturating_sub(VISIBLE_ROW_OVERSCAN)
        .min(line_count.saturating_sub(1));
    let end = last_visible
        .saturating_add(VISIBLE_ROW_OVERSCAN)
        .min(line_count);

    (start < end).then_some(start..end)
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
    viewport: Option<egui::Rect>,
) -> Option<egui::Vec2> {
    let cursor_rect = cursor_rect_at(galley, rect.min, local_cursor);
    paint_cursor_effects(ui, rect, cursor_rect, view.scroll_to_cursor, view, viewport)
}

fn requested_scroll_offset_for_cursor(
    scroll_to_cursor: bool,
    view: &EditorViewState,
    viewport: Option<egui::Rect>,
    cursor_rect: egui::Rect,
) -> Option<egui::Vec2> {
    if !scroll_to_cursor {
        return None;
    }

    let viewport = viewport?;
    scroll_offset_to_keep_rect_visible(view.editor_scroll_offset(), viewport, cursor_rect)
}

fn requested_scroll_offset_for_page_navigation(
    ui: &egui::Ui,
    focused: bool,
    current_offset: egui::Vec2,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<egui::Vec2> {
    let delta = page_navigation_scroll_delta(ui, focused, viewport, row_height)?;
    Some(egui::vec2(
        current_offset.x,
        (current_offset.y + delta).max(0.0),
    ))
}

fn page_navigation_requested_scroll_offset(
    ui: &egui::Ui,
    focused: bool,
    view: Option<&EditorViewState>,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<egui::Vec2> {
    let current_offset = view?.editor_scroll_offset();
    requested_scroll_offset_for_page_navigation(ui, focused, current_offset, viewport, row_height)
}

fn page_navigation_scroll_delta(
    ui: &egui::Ui,
    focused: bool,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<f32> {
    if !focused || row_height <= 0.0 {
        return None;
    }

    let page_delta = page_jump_rows(viewport, row_height) as f32 * row_height;
    if page_delta <= 0.0 {
        return None;
    }

    let delta = ui.input(|input| {
        input.events.iter().fold(0.0, |delta, event| {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                return delta;
            };

            if modifiers.command {
                return delta;
            }

            match key {
                egui::Key::PageUp => delta - page_delta,
                egui::Key::PageDown => delta + page_delta,
                _ => delta,
            }
        })
    });

    (delta != 0.0).then_some(delta)
}

fn page_jump_rows(viewport: Option<egui::Rect>, row_height: f32) -> usize {
    viewport
        .and_then(|viewport| viewport_line_capacity(viewport, row_height))
        .unwrap_or(1)
}

fn scroll_offset_to_keep_rect_visible(
    current_offset: egui::Vec2,
    viewport: egui::Rect,
    target: egui::Rect,
) -> Option<egui::Vec2> {
    if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
        return None;
    }

    let mut desired = current_offset;

    if target.left() < viewport.left() {
        desired.x = target.left().max(0.0);
    } else if target.right() > viewport.right() {
        desired.x = (target.right() - viewport.width()).max(0.0);
    }

    if target.top() < viewport.top() {
        desired.y = target.top().max(0.0);
    } else if target.bottom() > viewport.bottom() {
        desired.y = (target.bottom() - viewport.height()).max(0.0);
    }

    (desired != current_offset).then_some(desired)
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
    use super::{
        CharCursor, CursorRange, cursor_visible_line_range,
        requested_scroll_offset_for_page_navigation, scroll_offset_to_keep_rect_visible,
        sync_view_cursor_before_render, viewport_line_capacity, viewport_visible_line_range,
    };
    use crate::app::domain::{BufferState, EditorViewState};
    use eframe::egui;

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

    #[test]
    fn viewport_line_range_tracks_scrolled_content_with_overscan() {
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 180.0), egui::pos2(400.0, 360.0));

        let visible = viewport_visible_line_range(Some(viewport), 18.0, 100);

        assert_eq!(visible, Some(8..22));
    }

    #[test]
    fn viewport_line_range_clamps_near_document_end() {
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 900.0), egui::pos2(400.0, 1100.0));

        let visible = viewport_visible_line_range(Some(viewport), 18.0, 52);

        assert_eq!(visible, Some(48..52));
    }

    #[test]
    fn cursor_visible_line_range_centers_cursor_without_previous_layout() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(40).start_char;
        view.cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 180.0));

        let visible = cursor_visible_line_range(&buffer, &view, Some(viewport), 18.0);

        assert_eq!(viewport_line_capacity(viewport, 18.0), Some(10));
        assert_eq!(visible, Some(33..47));
    }

    #[test]
    fn scroll_offset_to_keep_rect_visible_moves_only_when_cursor_exits_viewport() {
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 180.0), egui::pos2(400.0, 360.0));
        let below_view = egui::Rect::from_min_max(egui::pos2(10.0, 380.0), egui::pos2(14.0, 398.0));
        let in_view = egui::Rect::from_min_max(egui::pos2(10.0, 220.0), egui::pos2(14.0, 238.0));

        assert_eq!(
            scroll_offset_to_keep_rect_visible(egui::vec2(0.0, 180.0), viewport, below_view),
            Some(egui::vec2(0.0, 218.0))
        );
        assert_eq!(
            scroll_offset_to_keep_rect_visible(egui::vec2(0.0, 180.0), viewport, in_view),
            None
        );
    }

    #[test]
    fn page_navigation_requests_explicit_scroll_offset() {
        let ctx = egui::Context::default();
        let mut requested = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.input_mut(|input| {
                input.events.push(egui::Event::Key {
                    key: egui::Key::PageDown,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                });
            });

            requested = requested_scroll_offset_for_page_navigation(
                ui,
                true,
                egui::vec2(0.0, 36.0),
                Some(egui::Rect::from_min_max(
                    egui::pos2(0.0, 36.0),
                    egui::pos2(400.0, 216.0),
                )),
                18.0,
            );
        });

        assert_eq!(requested, Some(egui::vec2(0.0, 216.0)));
    }
}
