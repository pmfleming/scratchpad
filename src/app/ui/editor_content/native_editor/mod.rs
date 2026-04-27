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
    BufferState, CursorRevealMode, EditorViewState, RenderedLayout, RenderedTextWindow,
    SearchHighlightState, VisibleWindowLayoutKey,
};
use eframe::egui;
use interactions::{
    cursor_range_after_click, handle_keyboard_events, handle_keyboard_events_unwrapped,
    handle_mouse_interaction, handle_mouse_interaction_window, sync_view_cursor_before_render,
};
use std::ops::Range;
use std::sync::Arc;

const VISIBLE_ROW_OVERSCAN: usize = 2;
const CURSOR_REVEAL_MARGIN_PX: f32 = 24.0;
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
    document_revision: u64,
    total_line_count: usize,
    active_selection: Option<&'a Range<usize>>,
}

struct VisibleWindowLayoutState {
    search_highlights: SearchHighlightState,
    selection_range: Option<Range<usize>>,
    layout_key: VisibleWindowLayoutKey,
}

struct VisibleWindowFrame {
    row_height: f32,
    bottom_padding_lines: usize,
    is_editable: bool,
    wrap_width: f32,
}

struct VisibleWindowInputOutcome {
    input_state: Option<VisibleWindowInputState>,
    request_editor_focus: bool,
    focused: bool,
    changed: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VisibleWindowDebugSnapshot {
    pub(crate) rect: egui::Rect,
    pub(crate) line_range: Range<usize>,
    pub(crate) row_height: f32,
    pub(crate) is_editable: bool,
    pub(crate) response_clicked: bool,
    pub(crate) response_contains_pointer: bool,
    pub(crate) interact_pointer_pos: Option<egui::Pos2>,
    pub(crate) latest_pointer_pos: Option<egui::Pos2>,
    pub(crate) primary_released: bool,
}

#[cfg(test)]
fn visible_window_debug_id(view_id: crate::app::domain::ViewId) -> egui::Id {
    egui::Id::new(("visible_window_debug", view_id))
}

#[cfg(test)]
fn store_visible_window_debug_snapshot(
    ui: &egui::Ui,
    view_id: crate::app::domain::ViewId,
    snapshot: VisibleWindowDebugSnapshot,
) {
    ui.ctx()
        .data_mut(|data| data.insert_temp(visible_window_debug_id(view_id), snapshot));
}

#[cfg(test)]
pub(crate) fn load_visible_window_debug_snapshot(
    ctx: &egui::Context,
    view_id: crate::app::domain::ViewId,
) -> Option<VisibleWindowDebugSnapshot> {
    ctx.data(|data| data.get_temp(visible_window_debug_id(view_id)))
}

#[derive(Default)]
struct CursorPaintOutcome {
    requested_scroll_offset: Option<egui::Vec2>,
    reveal_attempted: bool,
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
    let total_chars = buffer.current_file_length().chars;
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

    let row_height = editor_row_height(ui, options.editor_font_id);
    let (rect, response) = ui.allocate_exact_size(
        editor_desired_size(
            ui,
            editor_desired_width(ui, &galley, options.word_wrap),
            editor_content_height(&galley, row_height),
        ),
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
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    }

    // Publish active view's selection to the buffer so all views can show it
    publish_active_selection(buffer, view, focused);

    let input_requested_scroll_offset =
        page_navigation_requested_scroll_offset(ui, focused, Some(view), viewport, row_height);

    let galley_pos = rect.min;
    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui, &galley, galley_pos, rect, view, options, focused, changed, viewport,
        )
    } else {
        CursorPaintOutcome::default()
    };
    let requested_scroll_offset = paint_outcome
        .requested_scroll_offset
        .or(input_requested_scroll_offset);

    // Consume scroll flag once the galley is fresh (scroll was applied)
    consume_cursor_reveal(view, changed, paint_outcome.reveal_attempted);
    sync_ime_output_focus(view, focused);

    if changed {
        clear_latest_layout(view);
    } else {
        update_visible_layout(
            &galley,
            galley_pos,
            rect,
            buffer,
            view,
            document_revision,
            row_height,
        );
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

    let row_height = editor_row_height(ui, options.editor_font_id);
    let visible_lines =
        visible_line_range_for_window(buffer, view, previous_layout, viewport, row_height, false)?;
    render_read_only_visible_lines(
        ui,
        buffer,
        view,
        previous_layout,
        visible_lines,
        options,
        viewport,
    )
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

    let row_height = editor_row_height(ui, options.editor_font_id);
    let visible_lines =
        visible_line_range_for_window(buffer, view, previous_layout, viewport, row_height, true)?;
    render_editable_visible_lines(
        ui,
        buffer,
        view,
        previous_layout,
        visible_lines,
        options,
        viewport,
    )
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

    let row_height = editor_row_height(ui, options.editor_font_id);
    let desired_height = desired_rows.max(1) as f32 * row_height;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            editor_desired_width(ui, &galley, options.word_wrap),
            desired_height,
        ),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let focused = response.has_focus() || response.gained_focus();
    sync_ime_output_focus(view, focused);
    let mut layout = RenderedLayout::from_galley(galley);
    layout.set_row_height(row_height);
    view.latest_layout = Some(layout);
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
    previous_layout: Option<&RenderedLayout>,
    visible_lines: Range<usize>,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Option<EditorWidgetOutcome> {
    Some(render_visible_text_window(
        ui,
        None,
        view,
        previous_layout,
        visible_line_window_for_range(buffer, visible_lines)?,
        options,
        viewport,
        VisibleWindowRenderRequest {
            document_revision: buffer.document_revision(),
            total_line_count: buffer.line_count,
            active_selection: buffer.active_selection.as_ref(),
        },
    ))
}

fn render_editable_visible_lines(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    visible_lines: Range<usize>,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Option<EditorWidgetOutcome> {
    let visible_window = visible_line_window_for_range(buffer, visible_lines)?;
    let document_revision = buffer.document_revision();
    let line_count = buffer.line_count;
    Some(render_visible_text_window(
        ui,
        Some(buffer),
        view,
        previous_layout,
        visible_window,
        options,
        viewport,
        VisibleWindowRenderRequest {
            document_revision,
            total_line_count: line_count,
            active_selection: None,
        },
    ))
}

fn visible_line_window_for_range(
    buffer: &BufferState,
    visible_lines: Range<usize>,
) -> Option<RenderedTextWindow> {
    (!visible_lines.is_empty()).then(|| buffer.visible_line_window(visible_lines))
}

#[allow(clippy::too_many_arguments)]
fn paint_editor(
    ui: &mut egui::Ui,
    galley: &Arc<egui::Galley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    focused: bool,
    changed: bool,
    viewport: Option<egui::Rect>,
) -> CursorPaintOutcome {
    // Paint galley — selection highlight is already baked into the LayoutJob
    paint_galley(ui, galley, galley_pos, options.text_color);

    if !focused {
        return CursorPaintOutcome::default();
    }

    if let Some(cursor_range) = &view.cursor_range
        && !changed
    {
        // Paint cursor (skip when changed — galley is stale, next frame corrects it)
        let cursor_rect = cursor_rect_at(galley, galley_pos, cursor_range.primary);
        return paint_cursor_effects(ui, rect, cursor_rect, view, viewport);
    }

    CursorPaintOutcome::default()
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
    view: &mut EditorViewState,
    viewport: Option<egui::Rect>,
) -> CursorPaintOutcome {
    let reveal_requested = view.cursor_reveal_mode().is_some();
    paint_cursor(ui, rect, cursor_rect);
    publish_ime_output(ui, rect, cursor_rect, view);
    CursorPaintOutcome {
        requested_scroll_offset: requested_scroll_offset_for_cursor(
            view.cursor_reveal_mode(),
            view,
            viewport,
            cursor_rect,
        ),
        reveal_attempted: reveal_requested,
    }
}

fn paint_cursor(ui: &egui::Ui, rect: egui::Rect, cursor_rect: egui::Rect) {
    let painter = ui.painter_at(rect.expand(1.0));
    let stroke = ui.visuals().text_cursor.stroke;
    painter.line_segment(
        [cursor_rect.center_top(), cursor_rect.center_bottom()],
        (stroke.width, stroke.color),
    );
}

fn publish_ime_output(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cursor_rect: egui::Rect,
    view: &mut EditorViewState,
) {
    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    let rect = to_global * rect;
    let cursor_rect = to_global * cursor_rect;
    if !view.mark_ime_output(rect, cursor_rect) {
        return;
    }

    ui.output_mut(|o| {
        o.ime = Some(egui::output::IMEOutput { rect, cursor_rect });
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
    previous_layout: Option<&RenderedLayout>,
    visible_window: RenderedTextWindow,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
    request: VisibleWindowRenderRequest<'_>,
) -> EditorWidgetOutcome {
    let mut buffer = buffer;
    let frame = visible_window_frame(ui, &visible_window, options, &request, buffer.is_some());
    add_visible_window_padding(ui, frame.row_height, visible_window.layout_row_offset);

    let mut layout_state = visible_window_layout_state(
        ui,
        buffer.as_deref(),
        view,
        &visible_window,
        options,
        request.active_selection,
        frame.wrap_width,
    );
    let mut galley = build_or_reuse_visible_window_galley(
        previous_layout,
        ui,
        &visible_window,
        options,
        &layout_state,
        frame.wrap_width,
    );

    let (rect, response) = allocate_visible_window_rect(ui, &galley, options, &frame);
    let interaction_rect = visible_window_interaction_rect(
        ui,
        viewport,
        &visible_window,
        frame.row_height,
        rect.size(),
    );
    let input = handle_visible_window_input(
        ui,
        &response,
        &galley,
        interaction_rect,
        buffer.as_deref_mut(),
        view,
        options,
        viewport,
        &visible_window,
    );
    #[cfg(test)]
    store_visible_window_debug_snapshot(
        ui,
        view.id,
        VisibleWindowDebugSnapshot {
            rect: interaction_rect,
            line_range: visible_window.line_range.clone(),
            row_height: frame.row_height,
            is_editable: frame.is_editable,
            response_clicked: response.clicked(),
            response_contains_pointer: response.contains_pointer(),
            interact_pointer_pos: response.interact_pointer_pos(),
            latest_pointer_pos: ui.input(|input| input.pointer.latest_pos()),
            primary_released: ui
                .input(|input| input.pointer.button_released(egui::PointerButton::Primary)),
        },
    );

    let input_requested_scroll_offset = page_navigation_requested_scroll_offset(
        ui,
        input.focused,
        Some(view),
        viewport,
        frame.row_height,
    );
    refresh_visible_window_galley_after_input(
        previous_layout,
        ui,
        buffer.as_deref(),
        view,
        &visible_window,
        options,
        request.active_selection,
        &frame,
        input.changed,
        &mut layout_state,
        &mut galley,
    );

    let paint_outcome = paint_visible_window(
        ui,
        &galley,
        rect,
        view,
        &visible_window.char_range,
        &input,
        options,
        viewport,
    );
    let unpainted_cursor_outcome = if paint_outcome.reveal_attempted {
        CursorPaintOutcome::default()
    } else {
        buffer
            .as_deref()
            .map(|buffer| unpainted_cursor_reveal_outcome(buffer, view, viewport, frame.row_height))
            .unwrap_or_default()
    };
    let requested_scroll_offset = paint_outcome
        .requested_scroll_offset
        .or(unpainted_cursor_outcome.requested_scroll_offset)
        .or(input_requested_scroll_offset);
    let reveal_attempted =
        paint_outcome.reveal_attempted || unpainted_cursor_outcome.reveal_attempted;

    publish_visible_window_frame(
        view,
        input.changed,
        input.focused,
        reveal_attempted,
        galley,
        visible_window,
        layout_state,
        frame.row_height,
        request.document_revision,
    );
    add_visible_window_padding(ui, frame.row_height, frame.bottom_padding_lines);

    EditorWidgetOutcome {
        changed: input.changed,
        focused: input.focused,
        request_editor_focus: input.request_editor_focus,
        requested_scroll_offset,
        response,
    }
}

fn visible_window_frame(
    ui: &egui::Ui,
    visible_window: &RenderedTextWindow,
    options: TextEditOptions<'_>,
    request: &VisibleWindowRenderRequest<'_>,
    is_editable: bool,
) -> VisibleWindowFrame {
    VisibleWindowFrame {
        row_height: editor_row_height(ui, options.editor_font_id),
        bottom_padding_lines: request
            .total_line_count
            .saturating_sub(visible_window.line_range.end),
        is_editable,
        wrap_width: editor_wrap_width(ui, options.word_wrap),
    }
}

fn add_visible_window_padding(ui: &mut egui::Ui, row_height: f32, lines: usize) {
    if lines > 0 {
        ui.add_space(row_height * lines as f32);
    }
}

fn allocate_visible_window_rect(
    ui: &mut egui::Ui,
    galley: &egui::Galley,
    options: TextEditOptions<'_>,
    frame: &VisibleWindowFrame,
) -> (egui::Rect, egui::Response) {
    ui.allocate_exact_size(
        egui::vec2(
            editor_desired_width(ui, galley, options.word_wrap),
            editor_content_height(galley, frame.row_height),
        ),
        if frame.is_editable {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_visible_window_input(
    ui: &mut egui::Ui,
    response: &egui::Response,
    galley: &Arc<egui::Galley>,
    interaction_rect: egui::Rect,
    buffer: Option<&mut BufferState>,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
    visible_window: &RenderedTextWindow,
) -> VisibleWindowInputOutcome {
    if let Some(buffer) = buffer {
        let input_state = handle_visible_window_buffer_input(
            ui,
            response,
            galley,
            interaction_rect,
            buffer,
            view,
            options,
            viewport,
            visible_window.char_range.start,
        );
        return VisibleWindowInputOutcome::editable(input_state);
    }

    VisibleWindowInputOutcome::read_only(apply_visible_window_click_focus(
        ui,
        response,
        galley,
        interaction_rect,
        view,
        visible_window,
    ))
}

impl VisibleWindowInputOutcome {
    fn editable(input_state: VisibleWindowInputState) -> Self {
        Self {
            focused: input_state.focused,
            changed: input_state.changed,
            input_state: Some(input_state),
            request_editor_focus: false,
        }
    }

    fn read_only(request_editor_focus: bool) -> Self {
        Self {
            input_state: None,
            request_editor_focus,
            focused: false,
            changed: false,
        }
    }

    fn should_paint_cursor(&self) -> bool {
        self.focused
            && self
                .input_state
                .as_ref()
                .is_some_and(|state| !state.changed)
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_visible_window_galley_after_input(
    previous_layout: Option<&RenderedLayout>,
    ui: &egui::Ui,
    buffer: Option<&BufferState>,
    view: &EditorViewState,
    visible_window: &RenderedTextWindow,
    options: TextEditOptions<'_>,
    active_selection: Option<&Range<usize>>,
    frame: &VisibleWindowFrame,
    changed: bool,
    layout_state: &mut VisibleWindowLayoutState,
    galley: &mut Arc<egui::Galley>,
) {
    if !frame.is_editable || changed {
        return;
    }

    let refreshed = visible_window_layout_state(
        ui,
        buffer,
        view,
        visible_window,
        options,
        active_selection,
        frame.wrap_width,
    );
    if refreshed.layout_key == layout_state.layout_key {
        return;
    }

    *galley = build_or_reuse_visible_window_galley(
        previous_layout,
        ui,
        visible_window,
        options,
        &refreshed,
        frame.wrap_width,
    );
    *layout_state = refreshed;
}

#[allow(clippy::too_many_arguments)]
fn paint_visible_window(
    ui: &mut egui::Ui,
    galley: &Arc<egui::Galley>,
    rect: egui::Rect,
    view: &mut EditorViewState,
    char_range: &Range<usize>,
    input: &VisibleWindowInputOutcome,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> CursorPaintOutcome {
    if !ui.is_rect_visible(rect) {
        return CursorPaintOutcome::default();
    }

    paint_galley(ui, galley, rect.min, options.text_color);
    if !input.should_paint_cursor() {
        return CursorPaintOutcome::default();
    }

    let local_cursor = match view
        .cursor_range
        .and_then(|cursor| local_cursor_in_window(cursor.primary, char_range.start, char_range.end))
    {
        Some(cursor) => cursor,
        None => return CursorPaintOutcome::default(),
    };
    paint_window_cursor(ui, galley, rect, local_cursor, view, viewport)
}

#[allow(clippy::too_many_arguments)]
fn publish_visible_window_frame(
    view: &mut EditorViewState,
    changed: bool,
    focused: bool,
    reveal_attempted: bool,
    galley: Arc<egui::Galley>,
    visible_window: RenderedTextWindow,
    layout_state: VisibleWindowLayoutState,
    row_height: f32,
    document_revision: u64,
) {
    consume_cursor_reveal(view, changed, reveal_attempted);
    sync_ime_output_focus(view, focused);

    if changed {
        clear_latest_layout(view);
    } else {
        set_latest_layout(
            view,
            Some(layout_with_visible_window(
                galley,
                Some(visible_window),
                Some(layout_state.layout_key),
                row_height,
            )),
            Some(document_revision),
        );
    }
    view.editor_has_focus = focused;
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
    if view.cursor_range != prev_cursor
        && !response.dragged()
        && ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
    {
        view.pending_cursor_range = view.cursor_range;
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    }

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
            buffer.current_file_length().chars,
        )
    } else {
        false
    };

    if view.cursor_range != prev_cursor {
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
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
    if !pointer_released_on_visible_window(ui, response, rect) {
        return false;
    }

    let Some(pointer_pos) = clicked_pointer_pos(ui, response, rect) else {
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
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    true
}

fn pointer_released_on_visible_window(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
) -> bool {
    response.clicked()
        || (ui.input(|input| input.pointer.latest_pos().is_some_and(|pos| rect.contains(pos)))
            && ui.input(|input| input.pointer.button_released(egui::PointerButton::Primary)))
}

fn clicked_pointer_pos(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
) -> Option<egui::Pos2> {
    response
        .interact_pointer_pos()
        .or_else(|| {
            ui.input(|input| input.pointer.latest_pos().filter(|pos| rect.contains(*pos)))
        })
}

fn visible_window_interaction_rect(
    ui: &egui::Ui,
    viewport: Option<egui::Rect>,
    visible_window: &RenderedTextWindow,
    row_height: f32,
    size: egui::Vec2,
) -> egui::Rect {
    let Some(viewport) = viewport else {
        return egui::Rect::from_min_size(ui.clip_rect().min, size);
    };

    let clip_rect = ui.clip_rect();
    let left = clip_rect.left() - viewport.left();
    let top = clip_rect.top() + visible_window.line_range.start as f32 * row_height - viewport.top();
    egui::Rect::from_min_size(egui::pos2(left, top), size)
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
    row_height: f32,
) {
    let visible_row_range = visible_row_range_for_galley(galley, galley_pos, rect);
    let mut latest_layout = RenderedLayout::from_galley(galley.clone());
    latest_layout.set_row_height(row_height);
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
    layout_key: Option<VisibleWindowLayoutKey>,
    row_height: f32,
) -> RenderedLayout {
    let mut layout = RenderedLayout::from_galley(galley);
    layout.set_row_height(row_height);
    if let Some(mut visible_window) = visible_window {
        layout.offset_line_numbers(visible_window.line_range.start);
        visible_window.row_range = 0..layout.row_count();
        if let Some(layout_key) = layout_key {
            layout.set_visible_text_with_cache_key(visible_window, layout_key);
        } else {
            layout.set_visible_text(visible_window);
        }
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

fn editor_desired_size(ui: &egui::Ui, desired_width: f32, desired_height: f32) -> egui::Vec2 {
    let visible_height = ui.available_height();
    egui::vec2(desired_width.max(1.0), desired_height.max(visible_height))
}

fn editor_content_height(galley: &egui::Galley, row_height: f32) -> f32 {
    galley.rect.height().max(row_height).ceil().max(1.0)
}

fn editor_desired_width(ui: &egui::Ui, galley: &egui::Galley, word_wrap: bool) -> f32 {
    if word_wrap {
        ui.available_width()
    } else {
        galley.rect.width().max(1.0)
    }
}

fn consume_cursor_reveal(view: &mut EditorViewState, changed: bool, reveal_attempted: bool) {
    if !changed && (!view.scroll_to_cursor || reveal_attempted) {
        view.clear_cursor_reveal();
    }
}

fn sync_ime_output_focus(view: &mut EditorViewState, focused: bool) {
    if !focused {
        view.clear_ime_output();
    }
}

fn visible_window_layout_state(
    ui: &egui::Ui,
    buffer: Option<&BufferState>,
    view: &EditorViewState,
    visible_window: &RenderedTextWindow,
    options: TextEditOptions<'_>,
    active_selection: Option<&Range<usize>>,
    wrap_width: f32,
) -> VisibleWindowLayoutState {
    let search_highlights = highlighting::windowed_search_highlights(
        &view.search_highlights,
        &visible_window.char_range,
    );
    let selection_range =
        visible_window_selection(buffer, active_selection, &visible_window.char_range);
    let layout_key = editor_layout_cache_key(
        ui,
        options,
        wrap_width,
        &search_highlights,
        selection_range.clone(),
    );

    VisibleWindowLayoutState {
        search_highlights,
        selection_range,
        layout_key,
    }
}

fn visible_window_selection(
    buffer: Option<&BufferState>,
    active_selection: Option<&Range<usize>>,
    visible_char_range: &Range<usize>,
) -> Option<Range<usize>> {
    let active_selection = buffer
        .and_then(|buffer| buffer.active_selection.as_ref())
        .or(active_selection);
    highlighting::windowed_char_range(active_selection.cloned(), visible_char_range)
}

fn build_or_reuse_visible_window_galley(
    previous_layout: Option<&RenderedLayout>,
    ui: &egui::Ui,
    visible_window: &RenderedTextWindow,
    options: TextEditOptions<'_>,
    layout_state: &VisibleWindowLayoutState,
    wrap_width: f32,
) -> Arc<egui::Galley> {
    previous_layout
        .filter(|layout| {
            can_reuse_visible_window_layout(layout, visible_window, &layout_state.layout_key)
        })
        .map(|layout| layout.galley().clone())
        .unwrap_or_else(|| {
            highlighting::build_galley(
                ui,
                &visible_window.text,
                options,
                &layout_state.search_highlights,
                layout_state.selection_range.clone(),
                wrap_width,
            )
        })
}

fn can_reuse_visible_window_layout(
    previous_layout: &RenderedLayout,
    visible_window: &RenderedTextWindow,
    layout_key: &VisibleWindowLayoutKey,
) -> bool {
    previous_layout.matches_visible_window_layout(visible_window, layout_key)
}

fn editor_layout_cache_key(
    ui: &egui::Ui,
    options: TextEditOptions<'_>,
    wrap_width: f32,
    search_highlights: &SearchHighlightState,
    selection_range: Option<Range<usize>>,
) -> VisibleWindowLayoutKey {
    VisibleWindowLayoutKey {
        wrap_width_bits: wrap_width.to_bits(),
        font_size_bits: options.editor_font_id.size.to_bits(),
        dark_mode: ui.visuals().dark_mode,
        text_color: options.text_color,
        highlight_background: options.highlight_style.background,
        highlight_text: options.highlight_style.text,
        selection_range,
        search_highlight_signature: search_highlights.layout_signature(),
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

fn cursor_window_selection_mode(view: &EditorViewState) -> Option<CursorRevealMode> {
    view.cursor_reveal_mode().or_else(|| {
        view.pending_cursor_range
            .is_some()
            .then_some(CursorRevealMode::Center)
    })
}

fn visible_line_range_for_window(
    buffer: &BufferState,
    view: &EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    viewport: Option<egui::Rect>,
    row_height: f32,
    is_editable: bool,
) -> Option<Range<usize>> {
    let viewport = content_viewport(view, viewport);

    if !is_editable {
        if matches!(
            cursor_window_selection_mode(view),
            Some(CursorRevealMode::Center)
        ) {
            return cursor_reveal_visible_line_range(
                buffer,
                view,
                previous_layout,
                viewport,
                row_height,
                CursorRevealMode::Center,
            );
        }

        return viewport_visible_line_range(viewport, row_height, buffer.line_count)
            .or_else(|| Some(previous_layout?.visible_line_range()));
    }

    if let Some(mode) = cursor_window_selection_mode(view) {
        return cursor_reveal_visible_line_range(
            buffer,
            view,
            previous_layout,
            viewport,
            row_height,
            mode,
        );
    }

    viewport_visible_line_range(viewport, row_height, buffer.line_count)
        .or_else(|| {
            previous_layout.and_then(|layout| {
                focused_visible_line_range(buffer, view, layout, viewport, row_height)
            })
        })
        .or_else(|| previous_layout.map(RenderedLayout::visible_line_range))
}

fn cursor_reveal_visible_line_range(
    buffer: &BufferState,
    view: &EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    viewport: Option<egui::Rect>,
    row_height: f32,
    mode: CursorRevealMode,
) -> Option<Range<usize>> {
    match mode {
        CursorRevealMode::Center => cursor_visible_line_range(buffer, view, viewport, row_height),
        CursorRevealMode::KeepVisible => previous_layout
            .and_then(|layout| {
                focused_visible_line_range(buffer, view, layout, viewport, row_height)
            })
            .or_else(|| cursor_visible_line_range(buffer, view, viewport, row_height)),
    }
}

fn focused_visible_line_range(
    buffer: &BufferState,
    view: &EditorViewState,
    previous_layout: &RenderedLayout,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<Range<usize>> {
    let previous = previous_layout.visible_line_range();
    if previous.is_empty() {
        return None;
    }

    let viewport = content_viewport(view, viewport);

    let window_len = previous.len().max(1);
    let max_line = buffer.line_count.max(1);
    let mut start = viewport_visible_line_range(viewport, row_height, max_line)
        .map(|range| range.start)
        .unwrap_or(previous.start)
        .min(max_line.saturating_sub(1));
    let end = line_window_end(start, window_len, max_line);
    if end <= start {
        return None;
    }

    let cursor_line = cursor_line(buffer, view)?;
    if let Some(current_visible) = viewport_line_span(viewport, row_height, max_line) {
        let margin_lines = cursor_reveal_margin_lines(current_visible.len(), row_height);
        if let Some(delta) = reveal_band_scroll_delta(cursor_line, current_visible, margin_lines) {
            start = start
                .saturating_add_signed(delta)
                .min(max_line.saturating_sub(window_len.max(1)));
        }
    } else {
        let overscan = 2usize;

        if cursor_line < start.saturating_add(overscan) {
            start = cursor_line.saturating_sub(overscan);
        } else if cursor_line + overscan >= end {
            start = cursor_line
                .saturating_add(overscan + 1)
                .saturating_sub(window_len)
                .min(max_line.saturating_sub(window_len.max(1)));
        }
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

fn viewport_line_span(
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
    let start = (viewport.min.y / row_height).floor().max(0.0) as usize;
    let end = (viewport.max.y / row_height).ceil().max(start as f32 + 1.0) as usize;

    (start < line_count).then_some(start..end.min(line_count))
}

fn cursor_reveal_margin_lines(visible_len: usize, row_height: f32) -> usize {
    if visible_len <= 1 || row_height <= 0.0 {
        return 0;
    }

    ((CURSOR_REVEAL_MARGIN_PX / row_height).ceil() as usize).min(visible_len.saturating_sub(1))
}

fn reveal_band_scroll_delta(
    cursor_line: usize,
    current_visible: Range<usize>,
    margin_lines: usize,
) -> Option<isize> {
    let lower_bound = current_visible.start.saturating_add(margin_lines);
    let upper_bound = current_visible.end.saturating_sub(margin_lines.max(1));

    if cursor_line < lower_bound {
        return Some(cursor_line as isize - lower_bound as isize);
    }

    (cursor_line >= upper_bound).then_some(cursor_line as isize - upper_bound as isize + 1)
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

fn editor_row_height(ui: &egui::Ui, font_id: &egui::FontId) -> f32 {
    ui.fonts_mut(|fonts| fonts.row_height(font_id))
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
    view: &mut EditorViewState,
    viewport: Option<egui::Rect>,
) -> CursorPaintOutcome {
    let cursor_rect = cursor_rect_at(galley, rect.min, local_cursor);
    paint_cursor_effects(ui, rect, cursor_rect, view, viewport)
}

fn requested_scroll_offset_for_cursor(
    reveal_mode: Option<CursorRevealMode>,
    view: &EditorViewState,
    viewport: Option<egui::Rect>,
    cursor_rect: egui::Rect,
) -> Option<egui::Vec2> {
    let reveal_mode = reveal_mode?;
    let viewport = content_viewport(view, viewport)?;
    match reveal_mode {
        CursorRevealMode::KeepVisible => {
            scroll_offset_to_keep_rect_visible(view.editor_scroll_offset(), viewport, cursor_rect)
        }
        CursorRevealMode::Center => scroll_offset_to_center_rect_vertically(
            view.editor_scroll_offset(),
            viewport,
            cursor_rect,
        ),
    }
}

fn unpainted_cursor_reveal_outcome(
    buffer: &BufferState,
    view: &EditorViewState,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> CursorPaintOutcome {
    let reveal_requested = view.cursor_reveal_mode().is_some();
    let Some(cursor_rect) = unpainted_cursor_line_rect(buffer, view, row_height) else {
        return CursorPaintOutcome::default();
    };

    CursorPaintOutcome {
        requested_scroll_offset: requested_scroll_offset_for_cursor(
            view.cursor_reveal_mode(),
            view,
            viewport,
            cursor_rect,
        ),
        reveal_attempted: reveal_requested,
    }
}

fn unpainted_cursor_line_rect(
    buffer: &BufferState,
    view: &EditorViewState,
    row_height: f32,
) -> Option<egui::Rect> {
    if row_height <= 0.0 {
        return None;
    }

    let top = cursor_line(buffer, view)? as f32 * row_height;
    Some(egui::Rect::from_min_max(
        egui::pos2(0.0, top),
        egui::pos2(1.0, top + row_height),
    ))
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

fn content_viewport(view: &EditorViewState, viewport: Option<egui::Rect>) -> Option<egui::Rect> {
    let _ = view;
    viewport
}

fn page_navigation_scroll_delta(
    ui: &egui::Ui,
    focused: bool,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<f32> {
    let page_delta = page_navigation_delta_size(focused, viewport, row_height)?;
    let direction = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(page_navigation_direction)
            .sum::<f32>()
    });

    (direction != 0.0).then_some(direction * page_delta)
}

fn page_navigation_delta_size(
    focused: bool,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<f32> {
    if !focused || row_height <= 0.0 {
        return None;
    }

    let page_delta = page_jump_rows(viewport, row_height) as f32 * row_height;
    (page_delta > 0.0).then_some(page_delta)
}

fn page_navigation_direction(event: &egui::Event) -> Option<f32> {
    let egui::Event::Key {
        key,
        pressed: true,
        modifiers,
        ..
    } = event
    else {
        return None;
    };

    if modifiers.command {
        return None;
    }

    match key {
        egui::Key::PageUp => Some(-1.0),
        egui::Key::PageDown => Some(1.0),
        _ => None,
    }
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

    let desired = egui::vec2(
        scroll_offset_to_keep_axis_visible(
            viewport.left(),
            viewport.width(),
            target.left(),
            target.right(),
            0.0,
        )
        .unwrap_or(current_offset.x),
        scroll_offset_to_keep_axis_visible(
            viewport.top(),
            viewport.height(),
            target.top(),
            target.bottom(),
            CURSOR_REVEAL_MARGIN_PX,
        )
        .unwrap_or(current_offset.y),
    );

    (desired != current_offset).then_some(desired)
}

fn scroll_offset_to_center_rect_vertically(
    current_offset: egui::Vec2,
    viewport: egui::Rect,
    target: egui::Rect,
) -> Option<egui::Vec2> {
    if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
        return None;
    }

    let desired = egui::vec2(
        scroll_offset_to_keep_axis_visible(
            viewport.left(),
            viewport.width(),
            target.left(),
            target.right(),
            0.0,
        )
        .unwrap_or(current_offset.x),
        (target.center().y - viewport.height() / 2.0).max(0.0),
    );

    (desired != current_offset).then_some(desired)
}

fn scroll_offset_to_keep_axis_visible(
    viewport_start: f32,
    viewport_size: f32,
    target_start: f32,
    target_end: f32,
    margin: f32,
) -> Option<f32> {
    let margin = margin.max(0.0).min(viewport_size / 2.0);
    let target_start = target_start - margin;
    let target_end = target_end + margin;

    if target_start < viewport_start {
        return Some(target_start.max(0.0));
    }

    let desired = (target_end - viewport_size).max(0.0);
    (desired > viewport_start).then_some(desired)
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
        CharCursor, CursorRange, consume_cursor_reveal, cursor_reveal_visible_line_range,
        cursor_visible_line_range, cursor_window_selection_mode, editor_content_height,
        editor_desired_size, editor_desired_width, focused_visible_line_range,
        requested_scroll_offset_for_page_navigation, scroll_offset_to_center_rect_vertically,
        scroll_offset_to_keep_rect_visible, sync_view_cursor_before_render,
        unpainted_cursor_reveal_outcome, viewport_line_capacity, viewport_visible_line_range,
        visible_line_range_for_window, visible_window_selection,
    };
    use crate::app::domain::RenderedTextWindow;
    use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState, RenderedLayout};
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
    fn pending_cursor_sync_preserves_existing_reveal_mode() {
        let mut view = EditorViewState::new(1, false);
        let pending = CursorRange::one(CharCursor::new(7));
        view.pending_cursor_range = Some(pending);
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(view.cursor_range, Some(pending));
        assert_eq!(
            view.cursor_reveal_mode(),
            Some(CursorRevealMode::KeepVisible)
        );
    }

    #[test]
    fn pending_cursor_selects_cursor_window_before_focus_sync() {
        let mut view = EditorViewState::new(1, false);
        view.pending_cursor_range = Some(CursorRange::one(CharCursor::new(7)));

        assert!(cursor_window_selection_mode(&view).is_some());
    }

    #[test]
    fn stable_frame_consumes_scroll_to_cursor_request() {
        let mut view = EditorViewState::new(1, false);
        view.scroll_to_cursor = true;

        consume_cursor_reveal(&mut view, false, true);

        assert!(!view.scroll_to_cursor);
    }

    #[test]
    fn changed_frame_keeps_scroll_to_cursor_request() {
        let mut view = EditorViewState::new(1, false);
        view.scroll_to_cursor = true;

        consume_cursor_reveal(&mut view, true, true);

        assert!(view.scroll_to_cursor);
    }

    #[test]
    fn stable_frame_keeps_scroll_to_cursor_until_cursor_reveal_is_attempted() {
        let mut view = EditorViewState::new(1, false);
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);

        consume_cursor_reveal(&mut view, false, false);

        assert!(view.scroll_to_cursor);
    }

    #[test]
    fn cursor_only_selection_move_prefers_live_buffer_selection_in_visible_window() {
        let mut buffer =
            BufferState::new("test.txt".to_owned(), "alpha beta gamma".to_owned(), None);
        buffer.active_selection = Some(6..10);
        let stale_request_selection = 0..5;

        let selection =
            visible_window_selection(Some(&buffer), Some(&stale_request_selection), &(4..15));

        assert_eq!(selection, Some(2..6));
    }

    #[test]
    fn editor_desired_size_does_not_add_extra_trailing_scroll_space() {
        let desired = editor_desired_size_for_test(400.0, 200.0, 400.0, 400.0);

        assert_eq!(desired, Some(egui::vec2(400.0, 400.0)));
    }

    #[test]
    fn editor_content_height_tracks_wrapped_visual_rows() {
        let height = editor_content_height_for_test(80.0, "W".repeat(200).as_str());

        assert!(height.is_some_and(|(height, row_height)| height > row_height * 2.0));
    }

    #[test]
    fn editor_desired_width_uses_wrap_point_when_wrapping() {
        let width = editor_desired_width_for_test(400.0, "W".repeat(200).as_str(), true);

        assert_eq!(width, Some(400.0));
    }

    #[test]
    fn editor_desired_width_uses_longest_line_without_wrap() {
        let width = editor_desired_width_for_test(400.0, "W".repeat(200).as_str(), false);

        assert!(width.is_some_and(|width| width > 400.0));
    }

    fn editor_desired_size_for_test(
        available_width: f32,
        available_height: f32,
        desired_width: f32,
        desired_height: f32,
    ) -> Option<egui::Vec2> {
        let ctx = egui::Context::default();
        let mut desired = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.set_width(available_width);
            ui.set_height(available_height);
            desired = Some(editor_desired_size(ui, desired_width, desired_height));
        });
        desired
    }

    fn editor_content_height_for_test(wrap_width: f32, text: &str) -> Option<(f32, f32)> {
        let ctx = egui::Context::default();
        let mut height = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let font_id = egui::FontId::monospace(14.0);
            let row_height = ui.fonts_mut(|fonts| fonts.row_height(&font_id));
            let galley = ui.ctx().fonts_mut(|fonts| {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = wrap_width;
                job.append(
                    text,
                    0.0,
                    egui::TextFormat {
                        font_id,
                        ..Default::default()
                    },
                );
                fonts.layout_job(job)
            });
            height = Some((editor_content_height(&galley, row_height), row_height));
        });
        height
    }

    fn editor_desired_width_for_test(
        available_width: f32,
        text: &str,
        word_wrap: bool,
    ) -> Option<f32> {
        let ctx = egui::Context::default();
        let mut desired = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.set_width(available_width);
            let galley = ui.ctx().fonts_mut(|fonts| {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = f32::INFINITY;
                job.append(
                    text,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(14.0),
                        ..Default::default()
                    },
                );
                fonts.layout_job(job)
            });
            desired = Some(editor_desired_width(ui, &galley, word_wrap));
        });
        desired
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
            Some(egui::vec2(0.0, 242.0))
        );
        assert_eq!(
            scroll_offset_to_keep_rect_visible(egui::vec2(0.0, 180.0), viewport, in_view),
            None
        );
    }

    #[test]
    fn scroll_offset_to_keep_rect_visible_uses_frame_viewport_when_stored_offset_is_stale() {
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 180.0), egui::pos2(400.0, 360.0));
        let below_view = egui::Rect::from_min_max(egui::pos2(10.0, 380.0), egui::pos2(14.0, 398.0));

        assert_eq!(
            scroll_offset_to_keep_rect_visible(egui::vec2(0.0, 300.0), viewport, below_view),
            Some(egui::vec2(0.0, 242.0))
        );
    }

    #[test]
    fn unpainted_cursor_reveal_scrolls_from_document_line_when_cursor_leaves_window() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(31).start_char;
        view.cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        view.set_editor_scroll_offset(egui::vec2(0.0, 360.0));
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 360.0), egui::pos2(400.0, 540.0));

        let outcome = unpainted_cursor_reveal_outcome(&buffer, &view, Some(viewport), 18.0);

        assert!(outcome.reveal_attempted);
        assert_eq!(
            outcome.requested_scroll_offset,
            Some(egui::vec2(0.0, 420.0))
        );
    }

    #[test]
    fn scroll_offset_to_keep_rect_visible_moves_back_to_leading_edge_when_cursor_precedes_viewport()
    {
        let viewport = egui::Rect::from_min_max(egui::pos2(120.0, 240.0), egui::pos2(520.0, 420.0));
        let before_view =
            egui::Rect::from_min_max(egui::pos2(80.0, 200.0), egui::pos2(96.0, 218.0));

        assert_eq!(
            scroll_offset_to_keep_rect_visible(egui::vec2(120.0, 240.0), viewport, before_view),
            Some(egui::vec2(80.0, 176.0))
        );
    }

    #[test]
    fn scroll_offset_to_center_rect_vertically_preserves_horizontal_when_cursor_is_visible() {
        let viewport = egui::Rect::from_min_max(egui::pos2(40.0, 180.0), egui::pos2(440.0, 360.0));
        let target = egui::Rect::from_min_max(egui::pos2(80.0, 500.0), egui::pos2(96.0, 518.0));

        assert_eq!(
            scroll_offset_to_center_rect_vertically(egui::vec2(40.0, 180.0), viewport, target),
            Some(egui::vec2(40.0, 419.0))
        );
    }

    #[test]
    fn center_reveal_selects_cursor_centered_window() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(40).start_char;
        view.cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 180.0));

        let visible = cursor_reveal_visible_line_range(
            &buffer,
            &view,
            None,
            Some(viewport),
            18.0,
            CursorRevealMode::Center,
        );

        assert_eq!(visible, Some(33..47));
    }

    #[test]
    fn read_only_visible_window_still_honors_pending_cursor_reveal() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(40).start_char;
        view.pending_cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 180.0));

        let visible =
            visible_line_range_for_window(&buffer, &view, None, Some(viewport), 18.0, false);

        assert_eq!(visible, Some(33..47));
    }

    #[test]
    fn read_only_visible_window_ignores_keep_visible_reveal_while_scrolling() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(34).start_char;
        view.pending_cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 900.0), egui::pos2(400.0, 1080.0));

        let visible =
            visible_line_range_for_window(&buffer, &view, None, Some(viewport), 18.0, false);

        assert_eq!(visible, Some(48..62));
    }

    #[test]
    fn focused_visible_line_range_shifts_when_cursor_enters_bottom_reveal_band() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let visible_window = buffer.visible_line_window(18..32);
        let previous_layout = visible_layout_for_test(&visible_window);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(29).start_char;
        view.cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 360.0), egui::pos2(400.0, 540.0));

        let visible =
            focused_visible_line_range(&buffer, &view, &previous_layout, Some(viewport), 18.0);

        assert_eq!(visible, Some(20..34));
    }

    #[test]
    fn focused_visible_line_range_repositions_window_when_viewport_scrolled_past_cursor() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let visible_window = buffer.visible_line_window(18..32);
        let previous_layout = visible_layout_for_test(&visible_window);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(30).start_char;
        view.cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 900.0), egui::pos2(400.0, 1080.0));

        let visible =
            focused_visible_line_range(&buffer, &view, &previous_layout, Some(viewport), 18.0);

        assert_eq!(visible, Some(26..40));
    }

    #[test]
    fn focused_visible_line_range_uses_frame_viewport_when_stored_offset_is_stale() {
        let text = (0..100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = BufferState::new("large.txt".to_owned(), text, None);
        let visible_window = buffer.visible_line_window(18..32);
        let previous_layout = visible_layout_for_test(&visible_window);
        let mut view = EditorViewState::new(buffer.id, false);
        let cursor_index = buffer.document().piece_tree().line_info(30).start_char;
        view.cursor_range = Some(CursorRange::one(CharCursor::new(cursor_index)));
        view.set_editor_scroll_offset(egui::Vec2::ZERO);
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 900.0), egui::pos2(400.0, 1080.0));

        let visible =
            focused_visible_line_range(&buffer, &view, &previous_layout, Some(viewport), 18.0);

        assert_eq!(visible, Some(26..40));
    }

    fn visible_layout_for_test(visible_window: &RenderedTextWindow) -> RenderedLayout {
        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let galley = ui.fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    visible_window.text.clone(),
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    f32::INFINITY,
                ))
            });
            let mut rendered = RenderedLayout::from_galley(galley);
            rendered.set_row_height(18.0);
            rendered.set_visible_text(visible_window.clone());
            layout = Some(rendered);
        });
        layout.expect("visible layout")
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
