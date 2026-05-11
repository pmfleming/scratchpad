use super::{TextEditOptions, highlighting};
use crate::app::domain::{
    BufferState, EditorViewState, LayoutCacheKey, SearchHighlightState, SearchReplacementPreview,
    ViewId,
};
use crate::app::ui::widget_ids::{self, WidgetRole};
use eframe::egui;
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

mod display_text;
pub(super) use display_text::DisplayTextMap;
use display_text::{
    compose_display_maps, display_search_highlights, display_selection_highlight,
    display_text_slice, preview_text_slice,
};

pub(super) struct EditorGalleyContext {
    pub(super) galley: Arc<egui::Galley>,
    pub(super) char_offset_base: usize,
    pub(super) logical_line_base: usize,
    pub(super) slice_chars: usize,
    pub(super) display_map: Option<DisplayTextMap>,
}

struct ViewportTextSlice<'a> {
    text: Cow<'a, str>,
    char_range: std::ops::Range<usize>,
    start_line: usize,
}

pub(super) fn build_editor_galley(
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
        cursor_line_for_viewport_slice(buffer, view),
    );
    let preview_slice = preview_text_slice(
        &slice.text,
        slice.char_range.clone(),
        view.search_replacement_preview.as_ref(),
    );
    let display_slice = display_text_slice(&preview_slice.text, buffer.show_control_chars);
    let display_map = compose_display_maps(preview_slice.map.as_ref(), display_slice.map.as_ref());
    let search_highlights = local_search_highlights(
        &view.search_highlights,
        slice.char_range.start,
        slice.char_range.end,
    );
    let selection_highlight = buffer
        .active_selection
        .clone()
        .and_then(|range| local_range(Some(range), slice.char_range.start, slice.char_range.end));
    let display_search_highlights =
        display_search_highlights(&search_highlights, display_map.as_ref());
    let display_selection_highlight =
        display_selection_highlight(selection_highlight.clone(), display_map.as_ref());
    let wrap_width = editor_wrap_width(ui, options.word_wrap, Some(effective_viewport));
    let cache_key = layout_cache_key(LayoutCacheKeyInput {
        revision: buffer.document_revision(),
        char_range: slice.char_range.clone(),
        show_control_chars: buffer.show_control_chars,
        options,
        search_highlights: &display_search_highlights,
        selection_highlight: display_selection_highlight.clone(),
        replacement_preview_signature: replacement_preview_signature(
            view.search_replacement_preview.as_ref(),
        ),
        wrap_width,
        dark_mode: ui.visuals().dark_mode,
    });
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
            &display_slice.text,
            options,
            &display_search_highlights,
            display_selection_highlight,
            wrap_width,
        );
        view.layout_cache
            .insert(cache_key, galley.clone(), display_slice.text.len());
        if options.warm_layout_cache
            && cache_was_warm
            && !buffer.show_control_chars
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
        display_map,
    }
}

struct LayoutCacheKeyInput<'a> {
    revision: u64,
    char_range: std::ops::Range<usize>,
    show_control_chars: bool,
    options: TextEditOptions<'a>,
    search_highlights: &'a SearchHighlightState,
    selection_highlight: Option<std::ops::Range<usize>>,
    replacement_preview_signature: u64,
    wrap_width: f32,
    dark_mode: bool,
}

fn layout_cache_key(input: LayoutCacheKeyInput<'_>) -> LayoutCacheKey {
    LayoutCacheKey {
        revision: input.revision,
        char_range: input.char_range,
        font_family: input.options.editor_font_id.family.clone(),
        font_size_bits: input.options.editor_font_id.size.to_bits(),
        wrap_width_bits: input.wrap_width.to_bits(),
        word_wrap: input.options.word_wrap,
        show_control_chars: input.show_control_chars,
        right_to_left_reading_order: input.options.right_to_left_reading_order,
        text_color: input.options.text_color,
        dark_mode: input.dark_mode,
        selection_highlight: input.selection_highlight,
        search_highlights: input.search_highlights.clone(),
        replacement_preview_signature: input.replacement_preview_signature,
    }
}

fn replacement_preview_signature(preview: Option<&SearchReplacementPreview>) -> u64 {
    let Some(preview) = preview else {
        return 0;
    };
    let mut hasher = DefaultHasher::new();
    preview.hash(&mut hasher);
    hasher.finish()
}

fn viewport_text_slice<'a>(
    buffer: &'a BufferState,
    viewport: egui::Rect,
    row_height: f32,
    cursor_line: Option<usize>,
) -> ViewportTextSlice<'a> {
    let line_count = buffer.line_count.max(1);
    let top_line = if row_height > 0.0 {
        (viewport.min.y.max(0.0) / row_height).floor() as usize
    } else {
        0
    };
    let visible_lines =
        super::interactions::viewport_line_capacity(viewport, row_height).unwrap_or(1);
    let overscan_lines = visible_lines.clamp(4, 24);
    let mut start_line = top_line
        .saturating_sub(overscan_lines)
        .min(line_count.saturating_sub(1));
    let mut end_line =
        (top_line + visible_lines + overscan_lines).min(line_count.saturating_sub(1));
    if let Some(cursor_line) = cursor_line.filter(|line| *line < line_count) {
        if cursor_line < start_line {
            start_line = cursor_line.saturating_sub(overscan_lines);
        } else if cursor_line > end_line {
            start_line = cursor_line.saturating_sub(overscan_lines);
            end_line = (cursor_line + overscan_lines).min(line_count.saturating_sub(1));
        }
    }
    let tree = buffer.document().piece_tree();
    let start_char = tree.line_info(start_line).start_char;
    let end_info = tree.line_info(end_line);
    let end_char = (end_info.start_char + end_info.char_len).min(tree.len_chars());
    let char_range = start_char..end_char;
    let text = tree
        .borrow_range(char_range.clone())
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(tree.extract_range(char_range.clone())));
    ViewportTextSlice {
        text,
        char_range,
        start_line,
    }
}

fn cursor_line_index(buffer: &BufferState, view: &EditorViewState) -> Option<usize> {
    let cursor = view.cursor_range?;
    (cursor.primary.index <= buffer.current_file_length().chars).then(|| {
        buffer
            .document()
            .piece_tree()
            .line_index_at_offset(cursor.primary.index)
    })
}

fn cursor_line_for_viewport_slice(buffer: &BufferState, view: &EditorViewState) -> Option<usize> {
    view.cursor_reveal_mode()
        .and_then(|_| cursor_line_index(buffer, view))
}

fn warm_nearby_layout_slices(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    viewport: egui::Rect,
    wrap_width: f32,
) {
    let row_height = editor_row_height(ui, options.editor_font_id);
    let visible_lines =
        super::interactions::viewport_line_capacity(viewport, row_height).unwrap_or(1) as f32;
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
        let slice = viewport_text_slice(buffer, adjacent, row_height, None);
        let search_highlights = local_search_highlights(
            &view.search_highlights,
            slice.char_range.start,
            slice.char_range.end,
        );
        let cache_key = layout_cache_key(LayoutCacheKeyInput {
            revision: buffer.document_revision(),
            char_range: slice.char_range.clone(),
            show_control_chars: false,
            options,
            search_highlights: &search_highlights,
            selection_highlight: None,
            replacement_preview_signature: 0,
            wrap_width,
            dark_mode: ui.visuals().dark_mode,
        });
        if view.layout_cache.get(&cache_key).is_some() {
            continue;
        }
        let galley = highlighting::build_galley(
            ui,
            &slice.text,
            options,
            &search_highlights,
            None,
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

pub(super) fn allocate_editor_rect(
    ui: &mut egui::Ui,
    galley: &egui::Galley,
    view_id: ViewId,
    options: TextEditOptions<'_>,
    total_content_height: f32,
    viewport: Option<egui::Rect>,
) -> (egui::Rect, egui::Response) {
    let response = widget_ids::allocate_exact_interact(
        ui,
        editor_desired_size(
            ui,
            editor_desired_width(ui, galley, options.word_wrap, viewport),
            total_content_height,
        ),
        editor_interaction_id(view_id),
        egui::Sense::click_and_drag(),
        "native_editor",
    );
    (response.rect, response)
}

fn editor_interaction_id(view_id: ViewId) -> egui::Id {
    widget_ids::surface_role(("native_editor", view_id), WidgetRole::TextEdit)
}

pub(super) fn galley_origin(
    rect: egui::Rect,
    logical_line_base: usize,
    row_height: f32,
) -> egui::Pos2 {
    rect.min + egui::vec2(0.0, logical_line_base as f32 * row_height)
}

fn editor_content_height(galley: &egui::Galley, row_height: f32) -> f32 {
    galley.rect.height().max(row_height).ceil().max(1.0)
}

pub(super) fn editor_viewport_height(ui: &egui::Ui, viewport: Option<egui::Rect>) -> f32 {
    viewport
        .map(|rect| rect.height())
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or_else(|| ui.available_height().max(0.0))
}

fn editor_eof_tail_height(viewport_height: f32, row_height: f32) -> f32 {
    let _ = (viewport_height, row_height);
    0.0
}

pub(super) fn total_editor_content_height(
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

pub(super) fn editor_desired_width(
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

pub(super) fn editor_row_height(ui: &egui::Ui, font_id: &egui::FontId) -> f32 {
    ui.fonts_mut(|fonts| fonts.row_height(font_id))
}

#[cfg(test)]
mod tests;
