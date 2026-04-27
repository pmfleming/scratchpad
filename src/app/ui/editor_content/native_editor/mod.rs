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

// `render_editor_visible_text_window` and `render_editor_focused_text_window`
// were removed in the scrolling rebuild (Phase 4+5). The unified renderer in
// `render_editor_text_edit` is now the only entry point; viewport slicing is
// done via `scrolling::DisplaySnapshot`/`ViewportSlice`.

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




#[allow(clippy::too_many_arguments)]

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

#[allow(clippy::too_many_arguments)]

#[allow(clippy::too_many_arguments)]

#[allow(clippy::too_many_arguments)]





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
    if !changed && (view.cursor_reveal_mode().is_none() || reveal_attempted) {
        view.clear_cursor_reveal();
    }
}

fn sync_ime_output_focus(view: &mut EditorViewState, focused: bool) {
    if !focused {
        view.clear_ime_output();
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





fn editor_row_height(ui: &egui::Ui, font_id: &egui::FontId) -> f32 {
    ui.fonts_mut(|fonts| fonts.row_height(font_id))
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
            scroll_offset_to_keep_rect_visible(view.editor_pixel_offset(), viewport, cursor_rect)
        }
        CursorRevealMode::Center => scroll_offset_to_center_rect_vertically(
            view.editor_pixel_offset(),
            viewport,
            cursor_rect,
        ),
    }
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
    let current_offset = view?.editor_pixel_offset();
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
        assert!(view.cursor_reveal_mode().is_some());
    }

    #[test]
    fn pending_cursor_range_overrides_missing_native_editor_cursor() {
        let mut view = EditorViewState::new(1, false);
        let pending = CursorRange::one(CharCursor::new(7));
        view.pending_cursor_range = Some(pending);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(view.cursor_range, Some(pending));
        assert_eq!(view.pending_cursor_range, None);
        assert!(view.cursor_reveal_mode().is_some());
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
        view.request_cursor_reveal(crate::app::domain::view::CursorRevealMode::KeepVisible);

        consume_cursor_reveal(&mut view, false, true);

        assert!(!view.cursor_reveal_mode().is_some());
    }

    #[test]
    fn changed_frame_keeps_scroll_to_cursor_request() {
        let mut view = EditorViewState::new(1, false);
        view.request_cursor_reveal(crate::app::domain::view::CursorRevealMode::KeepVisible);

        consume_cursor_reveal(&mut view, true, true);

        assert!(view.cursor_reveal_mode().is_some());
    }

    #[test]
    fn stable_frame_keeps_scroll_to_cursor_until_cursor_reveal_is_attempted() {
        let mut view = EditorViewState::new(1, false);
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);

        consume_cursor_reveal(&mut view, false, false);

        assert!(view.cursor_reveal_mode().is_some());
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
        view.set_editor_pixel_offset(egui::vec2(0.0, 360.0));
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
        view.set_editor_pixel_offset(egui::Vec2::ZERO);
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
