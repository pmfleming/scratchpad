use super::{TextEditOptions, highlighting};
use crate::app::domain::{
    BufferState, EditorViewState, LayoutCacheKey, SearchHighlightState, ViewId,
};
use crate::app::ui::widget_ids::{self, WidgetRole};
use eframe::egui;
use std::sync::Arc;

pub(super) struct EditorGalleyContext {
    pub(super) galley: Arc<egui::Galley>,
    pub(super) char_offset_base: usize,
    pub(super) logical_line_base: usize,
    pub(super) slice_chars: usize,
    pub(super) display_map: Option<DisplayTextMap>,
}

struct ViewportTextSlice {
    text: String,
    char_range: std::ops::Range<usize>,
    start_line: usize,
}

#[derive(Clone, Debug)]
pub(super) struct DisplayTextMap {
    doc_to_display: Vec<usize>,
    display_to_doc: Vec<usize>,
}

impl DisplayTextMap {
    pub(super) fn doc_to_display_cursor(&self, cursor: usize) -> usize {
        self.doc_to_display
            .get(cursor)
            .copied()
            .unwrap_or_else(|| *self.doc_to_display.last().unwrap_or(&0))
    }

    pub(super) fn display_to_doc_cursor(&self, cursor: usize) -> usize {
        self.display_to_doc
            .get(cursor)
            .copied()
            .unwrap_or_else(|| *self.display_to_doc.last().unwrap_or(&0))
    }

    pub(super) fn display_len(&self) -> usize {
        self.display_to_doc.len().saturating_sub(1)
    }

    pub(super) fn doc_range_to_display(
        &self,
        range: std::ops::Range<usize>,
    ) -> Option<std::ops::Range<usize>> {
        let start = self.doc_to_display_cursor(range.start);
        let end = self.doc_to_display_cursor(range.end);
        (start < end).then_some(start..end)
    }
}

struct DisplayTextSlice {
    text: String,
    map: Option<DisplayTextMap>,
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
    let display_slice = display_text_slice(&slice.text, buffer.show_control_chars);
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
        display_search_highlights(&search_highlights, display_slice.map.as_ref());
    let display_selection_highlight =
        display_selection_highlight(selection_highlight.clone(), display_slice.map.as_ref());
    let wrap_width = editor_wrap_width(ui, options.word_wrap, Some(effective_viewport));
    let cache_key = layout_cache_key(LayoutCacheKeyInput {
        revision: buffer.document_revision(),
        char_range: slice.char_range.clone(),
        show_control_chars: buffer.show_control_chars,
        options,
        search_highlights: &display_search_highlights,
        selection_highlight: display_selection_highlight.clone(),
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
        display_map: display_slice.map,
    }
}

struct LayoutCacheKeyInput<'a> {
    revision: u64,
    char_range: std::ops::Range<usize>,
    show_control_chars: bool,
    options: TextEditOptions<'a>,
    search_highlights: &'a SearchHighlightState,
    selection_highlight: Option<std::ops::Range<usize>>,
    wrap_width: f32,
    dark_mode: bool,
}

fn layout_cache_key(input: LayoutCacheKeyInput<'_>) -> LayoutCacheKey {
    LayoutCacheKey {
        revision: input.revision,
        char_range: input.char_range,
        font_family: format!("{:?}", input.options.editor_font_id.family),
        font_size_bits: input.options.editor_font_id.size.to_bits(),
        wrap_width_bits: input.wrap_width.to_bits(),
        word_wrap: input.options.word_wrap,
        show_control_chars: input.show_control_chars,
        right_to_left_reading_order: input.options.right_to_left_reading_order,
        text_color: input.options.text_color,
        dark_mode: input.dark_mode,
        selection_highlight: input.selection_highlight,
        search_highlights: input.search_highlights.clone(),
    }
}

fn display_text_slice(text: &str, show_control_chars: bool) -> DisplayTextSlice {
    if !show_control_chars {
        return DisplayTextSlice {
            text: text.to_owned(),
            map: None,
        };
    }

    let doc_len = text.chars().count();
    let mut visible = String::with_capacity(text.len());
    let mut doc_to_display = Vec::with_capacity(doc_len + 1);
    let mut display_to_doc = vec![0];
    let mut display_chars = 0usize;
    let mut chars = text.chars().peekable();

    for doc_index in 0..doc_len {
        let ch = chars.next().unwrap_or_default();
        doc_to_display.push(display_chars);
        let display =
            visible_control_char(ch, chars.peek().copied()).unwrap_or_else(|| ch.to_string());
        visible.push_str(&display);
        let len = display.chars().count();
        for boundary in 1..=len {
            let doc_cursor = if boundary < len && boundary < len.div_ceil(2) {
                doc_index
            } else {
                doc_index + 1
            };
            display_to_doc.push(doc_cursor);
        }
        display_chars += len;
    }
    doc_to_display.push(display_chars);

    DisplayTextSlice {
        text: visible,
        map: Some(DisplayTextMap {
            doc_to_display,
            display_to_doc,
        }),
    }
}

fn visible_control_char(ch: char, next: Option<char>) -> Option<String> {
    let label = match ch {
        '\u{1B}' => return Some("␛".to_owned()),
        '\u{0008}' => return Some("␈".to_owned()),
        '\t' => return Some("→".to_owned()),
        '\r' if next == Some('\n') => return Some("␍".to_owned()),
        '\r' => return Some("␍".to_owned()),
        '\n' => return Some("¶\n".to_owned()),
        '\u{001E}' => "RS",
        '\u{001F}' => "US",
        '\u{200B}' => "ZWSP",
        '\u{200C}' => "ZWNJ",
        '\u{200D}' => "ZWJ",
        '\u{200E}' => "LRM",
        '\u{200F}' => "RLM",
        '\u{202A}' => "LRE",
        '\u{202B}' => "RLE",
        '\u{202C}' => "PDF",
        '\u{202D}' => "LRO",
        '\u{202E}' => "RLO",
        '\u{2060}' => "WJ",
        '\u{2061}' => "FA",
        '\u{2062}' => "IT",
        '\u{2063}' => "IS",
        '\u{2064}' => "IP",
        '\u{2066}' => "LRI",
        '\u{2067}' => "RLI",
        '\u{2068}' => "FSI",
        '\u{2069}' => "PDI",
        '\u{206A}' => "ISS",
        '\u{206B}' => "ASS",
        '\u{206C}' => "IAFS",
        '\u{206D}' => "AAFS",
        '\u{206E}' => "NADS",
        '\u{206F}' => "NODS",
        '\u{FEFF}' => "BOM",
        '\u{061C}' => "ALM",
        _ if ch.is_control() => return Some(format!("\\x{:02X}", ch as u32)),
        _ => return None,
    };
    Some(format!("<{label}>"))
}

fn display_search_highlights(
    highlights: &SearchHighlightState,
    map: Option<&DisplayTextMap>,
) -> SearchHighlightState {
    let Some(map) = map else {
        return highlights.clone();
    };
    SearchHighlightState {
        ranges: highlights
            .ranges
            .iter()
            .filter_map(|range| map.doc_range_to_display(range.clone()))
            .collect(),
        active_range_index: highlights.active_range_index,
    }
}

fn display_selection_highlight(
    selection: Option<std::ops::Range<usize>>,
    map: Option<&DisplayTextMap>,
) -> Option<std::ops::Range<usize>> {
    match (selection, map) {
        (Some(range), Some(map)) => map.doc_range_to_display(range),
        (selection, None) => selection,
        (None, Some(_)) => None,
    }
}

fn viewport_text_slice(
    buffer: &BufferState,
    viewport: egui::Rect,
    row_height: f32,
    cursor_line: Option<usize>,
) -> ViewportTextSlice {
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
    ViewportTextSlice {
        text: tree.extract_range(start_char..end_char),
        char_range: start_char..end_char,
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
mod tests {
    use super::{
        cursor_line_for_viewport_slice, display_text_slice, editor_eof_tail_height,
        editor_interaction_id,
    };
    use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

    #[test]
    fn editor_interaction_id_is_stable_per_view() {
        assert_eq!(editor_interaction_id(7), editor_interaction_id(7));
        assert_ne!(editor_interaction_id(7), editor_interaction_id(8));
    }

    #[test]
    fn eof_tail_does_not_create_blank_scroll_page() {
        assert_eq!(editor_eof_tail_height(600.0, 20.0), 0.0);
    }

    #[test]
    fn viewport_slice_ignores_offscreen_cursor_without_pending_reveal() {
        let buffer = BufferState::new("sample.txt".to_owned(), numbered_lines(200), None);
        let mut view = EditorViewState::new(buffer.id, false);
        view.cursor_range = Some(CursorRange::one(CharCursor::new(
            buffer.document().piece_tree().line_info(120).start_char,
        )));

        assert_eq!(cursor_line_for_viewport_slice(&buffer, &view), None);
    }

    #[test]
    fn viewport_slice_can_follow_cursor_for_pending_reveal() {
        let buffer = BufferState::new("sample.txt".to_owned(), numbered_lines(200), None);
        let mut view = EditorViewState::new(buffer.id, false);
        view.cursor_range = Some(CursorRange::one(CharCursor::new(
            buffer.document().piece_tree().line_info(120).start_char,
        )));
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);

        assert_eq!(cursor_line_for_viewport_slice(&buffer, &view), Some(120));
    }

    #[test]
    fn visible_control_text_maps_newline_marker_to_single_document_char() {
        let display = display_text_slice("a\nb", true);
        let map = display.map.as_ref().unwrap();

        assert_eq!(display.text, "a¶\nb");
        assert_eq!(map.doc_to_display_cursor(1), 1);
        assert_eq!(map.doc_to_display_cursor(2), 3);
        assert_eq!(map.display_to_doc_cursor(1), 1);
        assert_eq!(map.display_to_doc_cursor(2), 2);
        assert_eq!(map.display_to_doc_cursor(3), 2);
    }

    #[test]
    fn visible_unicode_controls_expand_but_select_one_document_char() {
        let display = display_text_slice("a\u{200E}b", true);
        let map = display.map.as_ref().unwrap();

        assert_eq!(display.text, "a<LRM>b");
        assert_eq!(map.doc_range_to_display(1..2), Some(1..6));
        assert_eq!(map.display_to_doc_cursor(3), 1);
        assert_eq!(map.display_to_doc_cursor(4), 2);
    }

    fn numbered_lines(count: usize) -> String {
        (0..count)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
