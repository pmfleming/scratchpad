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
    BufferState, CursorRevealMode, EditorViewState, LayoutCacheKey, SearchHighlightState,
};
use crate::app::ui::scrolling::{DisplaySnapshot, ScrollAlign, ScrollIntent};
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_mouse_interaction, sync_view_cursor_before_render,
};
use std::sync::Arc;

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
    pub response: egui::Response,
}

#[derive(Default)]
struct CursorPaintOutcome {
    reveal_attempted: bool,
}

struct EditorGalleyContext {
    galley: Arc<egui::Galley>,
    char_offset_base: usize,
    logical_line_base: usize,
    slice_chars: usize,
}

struct ViewportTextSlice {
    text: String,
    char_range: std::ops::Range<usize>,
    start_line: usize,
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
    view.resolve_anchored_ranges(buffer);
    let mut document_revision = buffer.document_revision();
    let total_chars = buffer.current_file_length().chars;
    let mut galley_context = build_editor_galley(ui, buffer, view, options, viewport);

    let row_height = editor_row_height(ui, options.editor_font_id);
    let (rect, response) =
        allocate_editor_rect(ui, &galley_context.galley, options, row_height, viewport);
    request_editor_focus(ui, &response, options.request_focus);

    let input = process_editor_input(
        ui,
        buffer,
        view,
        EditorInputRequest {
            response: &response,
            galley: &galley_context.galley,
            rect,
            options,
            viewport,
            row_height,
            total_chars,
            char_offset_base: galley_context.char_offset_base,
            slice_chars: galley_context.slice_chars,
        },
    );

    if input.changed {
        document_revision = buffer.document_revision();
        galley_context = build_editor_galley(ui, buffer, view, options, viewport);
    }

    let galley_pos = rect.min;
    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui,
            &galley_context.galley,
            galley_pos,
            rect,
            view,
            options,
            input.focused,
            false,
            galley_context.char_offset_base,
        )
    } else {
        CursorPaintOutcome::default()
    };
    consume_cursor_reveal(view, false, paint_outcome.reveal_attempted);
    sync_ime_output_focus(view, input.focused);

    store_latest_snapshot(
        view,
        &galley_context.galley,
        row_height,
        false,
        Some(document_revision),
        galley_context.char_offset_base,
        galley_context.logical_line_base,
    );

    view.editor_has_focus = input.focused;

    EditorWidgetOutcome {
        changed: input.changed,
        focused: input.focused,
        request_editor_focus: false,
        response,
    }
}

struct EditorInputOutcome {
    focused: bool,
    changed: bool,
}

struct EditorInputRequest<'a> {
    response: &'a egui::Response,
    galley: &'a egui::Galley,
    rect: egui::Rect,
    options: TextEditOptions<'a>,
    viewport: Option<egui::Rect>,
    row_height: f32,
    total_chars: usize,
    char_offset_base: usize,
    slice_chars: usize,
}

fn process_editor_input(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: EditorInputRequest<'_>,
) -> EditorInputOutcome {
    let prev_cursor = view.cursor_range;
    let prev_cursor_line = prev_cursor.and_then(|cursor| primary_line_index(buffer, cursor));
    handle_mouse_interaction(
        ui,
        request.response,
        request.galley,
        request.rect,
        view,
        buffer.document().piece_tree(),
        request.char_offset_base,
    );
    let focused = request.response.has_focus()
        || request.response.gained_focus()
        || request.options.request_focus;
    sync_view_cursor_before_render(view, focused);
    let changed = handle_focused_keyboard_input(ui, buffer, view, &request, focused);
    request_cursor_reveal_after_input(buffer, view, prev_cursor, prev_cursor_line, changed);
    publish_active_selection(buffer, view, focused);
    view.sync_cursor_anchors_from_ranges(buffer);
    request_page_navigation_intent(ui, view, focused);
    EditorInputOutcome { focused, changed }
}

fn request_cursor_reveal_after_input(
    buffer: &BufferState,
    view: &mut EditorViewState,
    prev_cursor: Option<CursorRange>,
    prev_cursor_line: Option<usize>,
    changed: bool,
) {
    if view.cursor_range == prev_cursor {
        return;
    }

    if !changed
        || edit_moved_primary_cursor_to_new_line(buffer, view.cursor_range, prev_cursor_line)
    {
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    } else {
        view.request_cursor_reveal(CursorRevealMode::KeepHorizontalVisible);
    }
}

fn edit_moved_primary_cursor_to_new_line(
    buffer: &BufferState,
    cursor: Option<CursorRange>,
    prev_cursor_line: Option<usize>,
) -> bool {
    let Some(prev_line) = prev_cursor_line else {
        return true;
    };
    primary_line_index(buffer, cursor.unwrap_or_default()).is_none_or(|line| line != prev_line)
}

fn primary_line_index(buffer: &BufferState, cursor: CursorRange) -> Option<usize> {
    (cursor.primary.index <= buffer.current_file_length().chars).then(|| {
        buffer
            .document()
            .piece_tree()
            .line_index_at_offset(cursor.primary.index)
    })
}

fn handle_focused_keyboard_input(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: &EditorInputRequest<'_>,
    focused: bool,
) -> bool {
    focused
        && handle_keyboard_events(
            ui,
            buffer,
            view,
            request.galley,
            page_jump_rows(request.viewport, request.row_height),
            request.total_chars,
            request.char_offset_base,
            request.slice_chars,
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
            editor_desired_width(ui, &galley, options.word_wrap, None),
            desired_height,
        ),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        paint_galley(ui, &galley, rect.min, options.text_color);
    }

    let focused = response.has_focus() || response.gained_focus();
    sync_ime_output_focus(view, focused);
    store_latest_snapshot(view, &galley, row_height, false, None, 0, 0);
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
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    focused: bool,
    changed: bool,
    char_offset_base: usize,
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
        let content_cursor_rect = galley
            .pos_from_cursor(local_cursor(cursor_range.primary, char_offset_base).to_egui_ccursor())
            .expand(1.5);
        let cursor_rect = content_cursor_rect.translate(galley_pos.to_vec2());
        return paint_cursor_effects(ui, rect, cursor_rect, content_cursor_rect, view);
    }

    CursorPaintOutcome::default()
}

fn local_cursor(cursor: CharCursor, char_offset_base: usize) -> CharCursor {
    CharCursor {
        index: cursor.index.saturating_sub(char_offset_base),
        prefer_next_row: cursor.prefer_next_row,
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

fn paint_cursor_effects(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cursor_rect_screen: egui::Rect,
    cursor_rect_content: egui::Rect,
    view: &mut EditorViewState,
) -> CursorPaintOutcome {
    let reveal_mode = view.cursor_reveal_mode();
    paint_cursor(ui, rect, cursor_rect_screen);
    publish_ime_output(ui, rect, cursor_rect_screen, view);
    if let Some(mode) = reveal_mode {
        let align_y = match mode {
            CursorRevealMode::KeepVisible => {
                Some(ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX))
            }
            CursorRevealMode::KeepHorizontalVisible => None,
            CursorRevealMode::Center => Some(ScrollAlign::Center),
        };
        // Collapse the cursor to a zero-height point at its vertical center so
        // the `NearestWithMargin` trigger fires symmetrically at the top and
        // bottom of the viewport. With a non-zero-height target, the bottom
        // check uses `target.max` and the top uses `target.min`, which makes
        // bottom-edge reveals trigger one row earlier than top-edge reveals.
        let reveal_rect = egui::Rect::from_min_max(
            egui::pos2(cursor_rect_content.left(), cursor_rect_content.center().y),
            egui::pos2(cursor_rect_content.right(), cursor_rect_content.center().y),
        );
        view.request_intent(ScrollIntent::Reveal {
            rect: reveal_rect,
            align_y,
            align_x: Some(ScrollAlign::NearestWithMargin(0.0)),
        });
    }
    CursorPaintOutcome {
        reveal_attempted: reveal_mode.is_some(),
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

fn build_editor_galley(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> EditorGalleyContext {
    let effective_viewport = viewport.unwrap_or_else(|| bounded_editor_viewport(ui));
    let slice = viewport_text_slice(
        buffer,
        effective_viewport,
        editor_row_height(ui, options.editor_font_id),
    );
    let selection = local_range(
        buffer.active_selection.clone(),
        slice.char_range.start,
        slice.char_range.end,
    );
    let search_highlights = local_search_highlights(
        &view.search_highlights,
        slice.char_range.start,
        slice.char_range.end,
    );
    let wrap_width = editor_wrap_width(ui, options.word_wrap, Some(effective_viewport));
    let cache_key = layout_cache_key(
        buffer.document_revision(),
        slice.char_range.clone(),
        options,
        &search_highlights,
        selection.clone(),
        wrap_width,
        ui.visuals().dark_mode,
    );
    view.layout_cache
        .retain_revision(buffer.document_revision());
    let galley = view.layout_cache.get(&cache_key).unwrap_or_else(|| {
        let galley = highlighting::build_galley(
            ui,
            &slice.text,
            options,
            &search_highlights,
            selection,
            wrap_width,
        );
        view.layout_cache
            .insert(cache_key, galley.clone(), slice.text.len());
        galley
    });
    let slice_chars = slice.char_range.end.saturating_sub(slice.char_range.start);
    EditorGalleyContext {
        galley,
        char_offset_base: slice.char_range.start,
        logical_line_base: slice.start_line,
        slice_chars,
    }
}

fn layout_cache_key(
    revision: u64,
    char_range: std::ops::Range<usize>,
    options: TextEditOptions<'_>,
    search_highlights: &SearchHighlightState,
    selection_range: Option<std::ops::Range<usize>>,
    wrap_width: f32,
    dark_mode: bool,
) -> LayoutCacheKey {
    LayoutCacheKey {
        revision,
        char_range,
        font_family: format!("{:?}", options.editor_font_id.family),
        font_size_bits: options.editor_font_id.size.to_bits(),
        wrap_width_bits: wrap_width.to_bits(),
        word_wrap: options.word_wrap,
        text_color: options.text_color,
        dark_mode,
        selection_range,
        search_highlights: search_highlights.clone(),
    }
}

fn viewport_text_slice(
    buffer: &BufferState,
    viewport: egui::Rect,
    row_height: f32,
) -> ViewportTextSlice {
    let line_count = buffer.line_count.max(1);
    let top_line = if row_height > 0.0 {
        (viewport.min.y.max(0.0) / row_height).floor() as usize
    } else {
        0
    };
    let visible_lines = viewport_line_capacity(viewport, row_height).unwrap_or(1);
    let overscan_lines = visible_lines.min(24).max(4);
    let start_line = top_line
        .saturating_sub(overscan_lines)
        .min(line_count.saturating_sub(1));
    let end_line = (top_line + visible_lines + overscan_lines).min(line_count.saturating_sub(1));
    let tree = buffer.document().piece_tree();
    let start_char = tree.line_info(start_line).start_char;
    let end_info = tree.line_info(end_line);
    let end_char = (end_info.start_char + end_info.char_len).min(tree.len_chars());
    ViewportTextSlice {
        text: tree.extract_range(start_char..end_char),
        char_range: start_char..end_char,
        start_line,
    }
}

fn bounded_editor_viewport(ui: &egui::Ui) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            ui.available_width().max(1.0),
            ui.available_height().max(1.0),
        ),
    )
}

fn local_search_highlights(
    highlights: &SearchHighlightState,
    slice_start: usize,
    slice_end: usize,
) -> SearchHighlightState {
    let mut local = SearchHighlightState::default();
    for (index, range) in highlights.ranges.iter().enumerate() {
        if let Some(range) = local_range(Some(range.clone()), slice_start, slice_end) {
            if highlights.active_range_index == Some(index) {
                local.active_range_index = Some(local.ranges.len());
            }
            local.ranges.push(range);
        }
    }
    local
}

fn local_range(
    range: Option<std::ops::Range<usize>>,
    slice_start: usize,
    slice_end: usize,
) -> Option<std::ops::Range<usize>> {
    let range = range?;
    let start = range.start.max(slice_start);
    let end = range.end.min(slice_end);
    (start < end).then_some(start.saturating_sub(slice_start)..end.saturating_sub(slice_start))
}

fn allocate_editor_rect(
    ui: &mut egui::Ui,
    galley: &egui::Galley,
    options: TextEditOptions<'_>,
    row_height: f32,
    viewport: Option<egui::Rect>,
) -> (egui::Rect, egui::Response) {
    ui.allocate_exact_size(
        editor_desired_size(
            ui,
            editor_desired_width(ui, galley, options.word_wrap, viewport),
            editor_content_height(galley, row_height),
        ),
        egui::Sense::click_and_drag(),
    )
}

fn request_page_navigation_intent(ui: &egui::Ui, view: &mut EditorViewState, focused: bool) {
    if focused && let Some(direction) = consumed_page_navigation_direction(ui) {
        view.request_intent(crate::app::ui::scrolling::ScrollIntent::Pages(direction));
    }
}

fn store_latest_snapshot(
    view: &mut EditorViewState,
    galley: &Arc<egui::Galley>,
    row_height: f32,
    changed: bool,
    revision: Option<u64>,
    char_offset_base: usize,
    logical_line_base: usize,
) {
    if changed {
        view.latest_display_snapshot = None;
        view.latest_display_snapshot_revision = None;
    } else {
        let selection_range = view
            .cursor_range
            .as_ref()
            .and_then(types::selection_char_range);
        view.latest_display_snapshot = Some(DisplaySnapshot::from_galley_with_base_and_overlays(
            galley.clone(),
            row_height,
            char_offset_base,
            logical_line_base,
            selection_range,
            &view.search_highlights.ranges,
        ));
        view.latest_display_snapshot_revision = revision;
    }
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
    // Clamp the published widget rect to the current clip rect. The editor's
    // content rect can be tens of thousands of pixels tall for large files;
    // when winit converts that to physical pixels via DPI scaling, the
    // i32 `RECT { right: x + width, bottom: y + height }` arithmetic in
    // `winit::platform_impl::windows::ime::ImeContext::set_ime_cursor_area`
    // overflows ("attempt to add with overflow"). The IME area only needs to
    // describe the visible text-edit region anyway, so intersect with the
    // current clip rect (which is bounded by the on-screen viewport) before
    // publishing.
    let clip = ui.clip_rect();
    let visible_rect = rect.intersect(clip);
    if !visible_rect.is_finite() || visible_rect.width() <= 0.0 || visible_rect.height() <= 0.0 {
        // Editor entirely off-screen (or degenerate) — nothing useful to
        // publish, and an inverted/empty rect would underflow the same i32
        // RECT arithmetic in winit's IME path.
        return;
    }
    let rect = to_global * visible_rect;
    let cursor_rect = to_global * cursor_rect;
    if !view.mark_ime_output(rect, cursor_rect) {
        return;
    }

    ui.output_mut(|o| {
        o.ime = Some(egui::output::IMEOutput { rect, cursor_rect });
    });
}

// ---------------------------------------------------------------------------
// Private: layout helpers
// ---------------------------------------------------------------------------

fn editor_wrap_width(ui: &egui::Ui, word_wrap: bool, viewport: Option<egui::Rect>) -> f32 {
    if word_wrap {
        viewport_width(ui, viewport)
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

fn editor_desired_width(
    ui: &egui::Ui,
    galley: &egui::Galley,
    word_wrap: bool,
    viewport: Option<egui::Rect>,
) -> f32 {
    if word_wrap {
        viewport_width(ui, viewport)
    } else {
        galley.rect.width().max(1.0)
    }
}

fn viewport_width(ui: &egui::Ui, viewport: Option<egui::Rect>) -> f32 {
    viewport
        .map(|rect| rect.width())
        .filter(|width| width.is_finite() && *width > 0.0)
        .unwrap_or_else(|| ui.available_width())
        .max(1.0)
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

/// Inspect this frame's input events for an unconsumed PageUp/PageDown press
/// and return the direction (-1 or +1) suitable for `ScrollIntent::Pages`.
/// Returns `None` if no page-navigation key was pressed (or if a modifier
/// such as Cmd/Ctrl was held).
fn consumed_page_navigation_direction(ui: &egui::Ui) -> Option<i32> {
    let direction = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(page_navigation_direction)
            .sum::<i32>()
    });
    (direction != 0).then_some(direction)
}

fn page_navigation_direction(event: &egui::Event) -> Option<i32> {
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
        egui::Key::PageUp => Some(-1),
        egui::Key::PageDown => Some(1),
        _ => None,
    }
}

fn page_jump_rows(viewport: Option<egui::Rect>, row_height: f32) -> usize {
    viewport
        .and_then(|viewport| viewport_line_capacity(viewport, row_height))
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{
        CharCursor, CursorRange, consume_cursor_reveal, consumed_page_navigation_direction,
        editor_content_height, editor_desired_size, editor_desired_width, editor_wrap_width,
        local_cursor, local_range, local_search_highlights, request_cursor_reveal_after_input,
        sync_view_cursor_before_render, viewport_text_slice,
    };
    use crate::app::domain::{
        BufferState, CursorRevealMode, EditorViewState, SearchHighlightState,
    };
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
    fn viewport_text_slice_extracts_visible_lines_with_overscan() {
        let text = (0..100)
            .map(|line| format!("line-{line}\n"))
            .collect::<String>();
        let buffer = BufferState::new("slice.txt".to_owned(), text, None);
        let row_height = 10.0;
        let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 500.0), egui::vec2(320.0, 40.0));

        let slice = viewport_text_slice(&buffer, viewport, row_height);

        assert!(slice.text.starts_with("line-46\n"));
        assert!(slice.text.contains("line-57\n"));
        assert!(!slice.text.contains("line-45\n"));
        assert!(slice.char_range.start > 0);
    }

    #[test]
    fn local_ranges_are_clipped_to_viewport_slice() {
        assert_eq!(local_range(Some(10..20), 5, 30), Some(5..15));
        assert_eq!(local_range(Some(0..10), 5, 30), Some(0..5));
        assert_eq!(local_range(Some(30..40), 5, 30), None);
    }

    #[test]
    fn local_search_highlights_preserve_active_visible_range() {
        let highlights = SearchHighlightState {
            ranges: vec![0..5, 10..20, 40..50],
            active_range_index: Some(1),
        };

        let local = local_search_highlights(&highlights, 8, 24);

        assert_eq!(local.ranges, vec![2..12]);
        assert_eq!(local.active_range_index, Some(0));
    }

    #[test]
    fn cursor_paint_uses_viewport_local_offset() {
        let local = local_cursor(CharCursor::new(42), 40);

        assert_eq!(local.index, 2);
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
    fn stable_frame_consumes_scroll_to_cursor_request() {
        let mut view = EditorViewState::new(1, false);
        view.request_cursor_reveal(crate::app::domain::view::CursorRevealMode::KeepVisible);

        consume_cursor_reveal(&mut view, false, true);

        assert!(view.cursor_reveal_mode().is_none());
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
    fn same_line_edit_requests_horizontal_only_cursor_reveal() {
        let buffer = BufferState::new("test.txt".to_owned(), "alpha!".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let previous = CursorRange::one(CharCursor::new(5));
        view.cursor_range = Some(CursorRange::one(CharCursor::new(6)));

        request_cursor_reveal_after_input(&buffer, &mut view, Some(previous), Some(0), true);

        assert_eq!(
            view.cursor_reveal_mode(),
            Some(CursorRevealMode::KeepHorizontalVisible)
        );
    }

    #[test]
    fn newline_edit_keeps_vertical_cursor_reveal() {
        let buffer = BufferState::new("test.txt".to_owned(), "alpha\n".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let previous = CursorRange::one(CharCursor::new(5));
        view.cursor_range = Some(CursorRange::one(CharCursor::new(6)));

        request_cursor_reveal_after_input(&buffer, &mut view, Some(previous), Some(0), true);

        assert_eq!(
            view.cursor_reveal_mode(),
            Some(CursorRevealMode::KeepVisible)
        );
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
    fn editor_wrap_width_uses_viewport_when_child_ui_is_unbounded() {
        let ctx = egui::Context::default();
        let mut width = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.set_width(f32::INFINITY);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 200.0));
            width = Some(editor_wrap_width(ui, true, Some(viewport)));
        });

        assert_eq!(width, Some(320.0));
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
            desired = Some(editor_desired_width(ui, &galley, word_wrap, None));
        });
        desired
    }

    #[test]
    fn page_navigation_emits_pages_intent_with_signed_direction() {
        let ctx = egui::Context::default();
        let mut direction = None;
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

            direction = consumed_page_navigation_direction(ui);
        });

        assert_eq!(direction, Some(1));

        let mut direction = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            ui.input_mut(|input| {
                input.events.push(egui::Event::Key {
                    key: egui::Key::PageUp,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                });
            });

            direction = consumed_page_navigation_direction(ui);
        });

        assert_eq!(direction, Some(-1));
    }
}
