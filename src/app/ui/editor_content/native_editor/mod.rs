mod cursor;
mod editing;
mod highlighting;
mod interactions;
mod painting;
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
use crate::app::ui::scrolling::DisplaySnapshot;
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_mouse_interaction, sync_view_cursor_before_render,
};
use painting::{CursorPaintOutcome, consume_cursor_reveal, paint_editor, paint_galley};
use std::sync::Arc;

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
    let total_chars = buffer.current_file_length().chars;
    let mut galley_context = build_editor_galley(ui, buffer, view, options, viewport);

    let row_height = editor_row_height(ui, options.editor_font_id);
    let viewport_height = editor_viewport_height(ui, viewport);
    let total_content_height = total_editor_content_height(
        buffer.line_count.max(1),
        row_height,
        &galley_context.galley,
        viewport_height,
    );
    let (rect, response) = allocate_editor_rect(
        ui,
        &galley_context.galley,
        options,
        total_content_height,
        viewport,
    );
    let mut galley_pos = galley_origin(rect, galley_context.logical_line_base, row_height);
    request_editor_focus(ui, &response, options.request_focus);

    // The pre-input galley bakes `buffer.active_selection` into its layout.
    // If `process_editor_input` changes either the document or that
    // selection (mouse-drag, cursor move, focus gain), the painted
    // highlight would otherwise lag one frame and flicker. Capture the
    // exact value the pre-input galley used so we can rebuild only when
    // the bake actually became stale.
    let pre_active_selection = buffer.active_selection.clone();
    let input = process_editor_input(
        ui,
        buffer,
        view,
        EditorInputRequest {
            response: &response,
            galley: &galley_context.galley,
            rect,
            galley_pos,
            options,
            viewport,
            row_height,
            total_chars,
            char_offset_base: galley_context.char_offset_base,
            slice_chars: galley_context.slice_chars,
        },
    );

    let mut document_revision = buffer.document_revision();
    if input.changed || pre_active_selection != buffer.active_selection {
        document_revision = buffer.document_revision();
        galley_context = build_editor_galley(ui, buffer, view, options, viewport);
        galley_pos = galley_origin(rect, galley_context.logical_line_base, row_height);
    }

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
            galley_context.slice_chars,
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
    galley_pos: egui::Pos2,
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
        request.galley_pos,
        view,
        buffer.document().piece_tree(),
        request.char_offset_base,
    );
    let suppress_cursor_reveal = request.response.dragged_by(egui::PointerButton::Primary);
    let focused = request.response.has_focus()
        || request.response.gained_focus()
        || request.options.request_focus;
    sync_view_cursor_before_render(view, focused);
    let changed = handle_focused_keyboard_input(ui, buffer, view, &request, focused);
    request_cursor_reveal_after_input(
        buffer,
        view,
        prev_cursor,
        prev_cursor_line,
        changed,
        suppress_cursor_reveal,
    );
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
    suppress_reveal: bool,
) {
    if suppress_reveal {
        view.clear_cursor_reveal();
        return;
    }

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
    let cache_was_warm = !view.layout_cache.is_empty();
    let galley = if let Some(galley) = view.layout_cache.get(&cache_key) {
        crate::app::capacity_metrics::record_layout_cache_hit();
        galley
    } else {
        crate::app::capacity_metrics::record_layout_cache_miss();
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
        // Phase 4: warm nearby viewport slices opportunistically. We only
        // warm when the user is already scrolling around (the cache had
        // entries for this revision) and the global layout budget is not
        // saturated. This trades a one-off paint-pass cost for cache hits
        // on subsequent scroll frames in either direction.
        if cache_was_warm
            && !crate::app::memory_budget::over_budget(
                crate::app::memory_budget::BudgetCategory::Layout,
            )
        {
            warm_nearby_layout_slices(ui, buffer, view, options, effective_viewport, wrap_width);
        }
        galley
    };
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
    let overscan_lines = visible_lines.clamp(4, 24);
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

/// Phase 4: build galleys for slices just above and below the current
/// viewport and seed the layout cache. The slice is shifted by exactly one
/// visible-row block in each direction so the next-frame scroll lookup hits.
fn warm_nearby_layout_slices(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: egui::Rect,
    wrap_width: f32,
) {
    let row_height = editor_row_height(ui, options.editor_font_id);
    let visible_lines = viewport_line_capacity(viewport, row_height).unwrap_or(1) as f32;
    if visible_lines < 1.0 || row_height <= 0.0 {
        return;
    }
    let shift = visible_lines * row_height;
    let viewport_above = egui::Rect::from_min_size(
        egui::pos2(viewport.min.x, (viewport.min.y - shift).max(0.0)),
        viewport.size(),
    );
    let viewport_below = egui::Rect::from_min_size(
        egui::pos2(viewport.min.x, viewport.min.y + shift),
        viewport.size(),
    );

    for adjacent in [viewport_above, viewport_below] {
        if crate::app::memory_budget::over_budget(crate::app::memory_budget::BudgetCategory::Layout)
        {
            break;
        }
        let slice = viewport_text_slice(buffer, adjacent, row_height);
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
        let cache_key = layout_cache_key(
            buffer.document_revision(),
            slice.char_range.clone(),
            options,
            &search_highlights,
            selection.clone(),
            wrap_width,
            ui.visuals().dark_mode,
        );
        if view.layout_cache.get(&cache_key).is_some() {
            continue;
        }
        let galley = highlighting::build_galley(
            ui,
            &slice.text,
            options,
            &search_highlights,
            selection,
            wrap_width,
        );
        view.layout_cache
            .insert(cache_key, galley, slice.text.len());
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
    total_content_height: f32,
    viewport: Option<egui::Rect>,
) -> (egui::Rect, egui::Response) {
    ui.allocate_exact_size(
        editor_desired_size(
            ui,
            editor_desired_width(ui, galley, options.word_wrap, viewport),
            total_content_height,
        ),
        egui::Sense::click_and_drag(),
    )
}

fn galley_origin(rect: egui::Rect, logical_line_base: usize, row_height: f32) -> egui::Pos2 {
    rect.min + egui::vec2(0.0, logical_line_base as f32 * row_height)
}

fn editor_content_height(galley: &egui::Galley, row_height: f32) -> f32 {
    galley.rect.height().max(row_height).ceil().max(1.0)
}

fn editor_viewport_height(ui: &egui::Ui, viewport: Option<egui::Rect>) -> f32 {
    viewport
        .map(|rect| rect.height())
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or_else(|| ui.available_height().max(0.0))
}

fn editor_eof_tail_height(viewport_height: f32, row_height: f32) -> f32 {
    if viewport_height.is_finite() && row_height.is_finite() && row_height > 0.0 {
        (viewport_height - row_height).max(0.0)
    } else {
        0.0
    }
}

/// Full document content height, used to size the editor rect so the scroll
/// area can scroll across the entire document. The galley itself only covers
/// a slice (visible rows + overscan); the rest of the rect stays empty.
fn total_editor_content_height(
    line_count: usize,
    row_height: f32,
    galley: &egui::Galley,
    viewport_height: f32,
) -> f32 {
    let by_lines = (line_count as f32 * row_height).max(row_height);
    (by_lines.max(editor_content_height(galley, row_height))
        + editor_eof_tail_height(viewport_height, row_height))
    .ceil()
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
            galley.as_ref(),
            row_height,
            char_offset_base,
            logical_line_base,
            selection_range,
            &view.search_highlights.ranges,
        ));
        view.latest_display_snapshot_revision = revision;
    }
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
mod tests;
