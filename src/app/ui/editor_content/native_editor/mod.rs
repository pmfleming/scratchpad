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
    BufferState, EditorViewState, PublishedViewport, RenderedLayout, RevealRequest,
};
use crate::app::ui::scrolling;
use crate::app::ui::scrolling::ScrollIntent;
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_mouse_interaction, sync_view_cursor_before_render,
};
use std::fmt;
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
    pub response: egui::Response,
}

#[derive(Default)]
struct CursorPaintOutcome {
    reveal_attempted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewportRenderError {
    InvalidWrapWidth,
    MissingSourceSpan,
    EmptyViewportSlice,
}

impl fmt::Display for ViewportRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWrapWidth => write!(f, "invalid editor wrap width"),
            Self::MissingSourceSpan => write!(f, "could not extract viewport source span"),
            Self::EmptyViewportSlice => write!(f, "viewport slice has no paintable rows"),
        }
    }
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
        view.request_reveal(RevealRequest::KeepVisible);
    }

    // Publish active view's selection to the buffer so all views can show it
    publish_active_selection(buffer, view, focused);

    queue_page_navigation_intents(ui, focused, view, viewport, row_height);

    let galley_pos = rect.min;
    let display_snapshot = scrolling::DisplaySnapshot::from_galley(galley.clone(), row_height);
    let viewport_slice = viewport_slice_for_rect(&display_snapshot, galley_pos, rect, row_height);
    let visible_paint = match build_visible_paint_galley(
        ui,
        buffer,
        view,
        options,
        wrap_width,
        &display_snapshot,
        &viewport_slice,
        galley_pos,
    ) {
        Ok(paint) => {
            view.clear_render_notice();
            Some(paint)
        }
        Err(error) => {
            view.set_render_notice(crate::app::domain::EditorRenderNotice::new(format!(
                "Editor rendering degraded: {error}"
            )));
            None
        }
    };
    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui,
            &galley,
            visible_paint.as_ref(),
            galley_pos,
            rect,
            view,
            options,
            focused,
            changed,
            viewport,
        )
    } else {
        CursorPaintOutcome::default()
    };
    // Consume scroll flag once the galley is fresh (scroll was applied)
    consume_cursor_reveal(view, changed, paint_outcome.reveal_attempted);
    sync_ime_output_focus(view, focused);

    if changed {
        clear_latest_layout(view);
        view.clear_published_viewport();
    } else {
        update_visible_layout(
            VisibleLayoutInput {
                galley: &galley,
                display_snapshot: Some(display_snapshot),
                viewport_slice: Some(viewport_slice),
                galley_pos,
                rect,
                document_revision,
                row_height,
            },
            view,
        );
    }

    view.editor_has_focus = focused;

    EditorWidgetOutcome {
        changed,
        focused,
        request_editor_focus: false,
        response,
    }
}

// The old focused/unfocused window render entry points were removed in the
// scrolling rebuild. The unified renderer in `render_editor_text_edit` is now
// the only entry point; viewport slicing is done via
// `scrolling::DisplaySnapshot`/`ViewportSlice`.

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
    let display_snapshot = scrolling::DisplaySnapshot::from_galley(galley.clone(), row_height);
    let mut layout = RenderedLayout::from_galley(galley);
    layout.set_row_height(row_height);
    view.latest_display_snapshot = Some(display_snapshot);
    view.latest_layout = Some(layout);
    view.latest_layout_revision = None;
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
    visible_paint: Option<&VisiblePaintGalley>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
    focused: bool,
    changed: bool,
    viewport: Option<egui::Rect>,
) -> CursorPaintOutcome {
    if let Some(visible_paint) = visible_paint {
        paint_galley(
            ui,
            &visible_paint.galley,
            visible_paint.position,
            options.text_color,
        );
    } else {
        paint_galley(ui, galley, galley_pos, options.text_color);
    }

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
    paint_cursor(ui, rect, cursor_rect);
    publish_ime_output(ui, rect, cursor_rect, view);
    CursorPaintOutcome {
        reveal_attempted: queue_cursor_reveal_intent(
            view.reveal_request(),
            view,
            viewport,
            cursor_rect,
        ),
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
    let Some((rect, cursor_rect)) = safe_ime_rects(rect, cursor_rect) else {
        view.clear_ime_output();
        ui.output_mut(|output| output.ime = None);
        return;
    };

    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    let rect = to_global * rect;
    let cursor_rect = to_global * cursor_rect;
    let Some((rect, cursor_rect)) = safe_ime_rects(rect, cursor_rect) else {
        view.clear_ime_output();
        ui.output_mut(|output| output.ime = None);
        return;
    };

    if !view.mark_ime_output(rect, cursor_rect) {
        return;
    }

    ui.output_mut(|o| {
        o.ime = Some(egui::output::IMEOutput { rect, cursor_rect });
    });
}

fn safe_ime_rects(
    editor_rect: egui::Rect,
    cursor_rect: egui::Rect,
) -> Option<(egui::Rect, egui::Rect)> {
    if !rect_is_finite(editor_rect) || !rect_is_finite(cursor_rect) {
        return None;
    }
    if editor_rect.width() <= 0.0 || editor_rect.height() <= 0.0 {
        return None;
    }
    if !cursor_rect.intersects(editor_rect.expand(2.0)) {
        return None;
    }

    let cursor_rect = cursor_rect.intersect(editor_rect.expand(2.0));
    if cursor_rect.width() <= 0.0 || cursor_rect.height() <= 0.0 {
        return None;
    }
    Some((editor_rect, cursor_rect))
}

fn rect_is_finite(rect: egui::Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
}

// ---------------------------------------------------------------------------
// Private: layout helpers
// ---------------------------------------------------------------------------

struct VisibleLayoutInput<'a> {
    galley: &'a Arc<egui::Galley>,
    display_snapshot: Option<scrolling::DisplaySnapshot>,
    viewport_slice: Option<scrolling::ViewportSlice>,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    document_revision: u64,
    row_height: f32,
}

fn update_visible_layout(input: VisibleLayoutInput<'_>, view: &mut EditorViewState) {
    let display_snapshot = input.display_snapshot.unwrap_or_else(|| {
        scrolling::DisplaySnapshot::from_galley(input.galley.clone(), input.row_height)
    });
    let viewport_slice = input.viewport_slice.unwrap_or_else(|| {
        viewport_slice_for_rect(
            &display_snapshot,
            input.galley_pos,
            input.rect,
            input.row_height,
        )
    });
    let visible_row_range =
        (viewport_slice.rows.start as usize)..(viewport_slice.rows.end as usize);
    let mut latest_layout = RenderedLayout::from_galley(input.galley.clone());
    latest_layout.set_row_height(input.row_height);
    if let Some(line_range) = latest_layout.line_range_for_rows(visible_row_range.clone()) {
        view.publish_viewport(PublishedViewport {
            row_range: visible_row_range.clone(),
            line_range,
            layout_row_offset: 0,
        });
    }
    view.latest_display_snapshot = Some(display_snapshot);
    set_latest_layout(view, Some(latest_layout), Some(input.document_revision));
}

struct VisiblePaintGalley {
    galley: Arc<egui::Galley>,
    position: egui::Pos2,
}

#[allow(clippy::too_many_arguments)]
fn build_visible_paint_galley(
    ui: &egui::Ui,
    buffer: &BufferState,
    view: &EditorViewState,
    options: TextEditOptions<'_>,
    wrap_width: f32,
    display_snapshot: &scrolling::DisplaySnapshot,
    viewport_slice: &scrolling::ViewportSlice,
    galley_pos: egui::Pos2,
) -> Result<VisiblePaintGalley, ViewportRenderError> {
    if !wrap_width.is_finite() && !options.word_wrap {
        // Infinite width is intentional for unwrapped layout.
    } else if !wrap_width.is_finite() || wrap_width <= 0.0 {
        return Err(ViewportRenderError::InvalidWrapWidth);
    }
    let char_range = char_range_for_viewport(display_snapshot, viewport_slice)
        .map_err(|_| ViewportRenderError::EmptyViewportSlice)?;
    let text = buffer.document().text_cow();
    let text = text.as_ref();
    let byte_range = byte_range_for_char_range(text, char_range.clone())
        .ok_or(ViewportRenderError::MissingSourceSpan)?;
    let visible_text = &text[byte_range];
    let search_highlights = rebase_search_highlights(&view.search_highlights, char_range.clone());
    let selection_range = buffer
        .active_selection
        .as_ref()
        .and_then(|range| rebase_range(range, &char_range));
    let galley = highlighting::build_galley(
        ui,
        visible_text,
        options,
        &search_highlights,
        selection_range,
        wrap_width,
    );
    let first_row = scrolling::DisplayRow(viewport_slice.rows.start);
    let y_offset = display_snapshot.row_top(first_row).unwrap_or_default();
    Ok(VisiblePaintGalley {
        galley,
        position: galley_pos + egui::vec2(0.0, y_offset),
    })
}

fn viewport_slice_for_rect(
    display_snapshot: &scrolling::DisplaySnapshot,
    galley_pos: egui::Pos2,
    rect: egui::Rect,
    row_height: f32,
) -> scrolling::ViewportSlice {
    display_snapshot.viewport_slice(
        top_display_row_for_rect(galley_pos, rect, row_height),
        rect.height(),
        VISIBLE_ROW_OVERSCAN as u32,
    )
}

fn char_range_for_viewport(
    display_snapshot: &scrolling::DisplaySnapshot,
    viewport_slice: &scrolling::ViewportSlice,
) -> Result<std::ops::Range<usize>, scrolling::DisplaySnapshotError> {
    display_snapshot.char_range_for_rows(viewport_slice.rows.clone())
}

fn byte_range_for_char_range(
    text: &str,
    range: std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let mut start_byte = None;
    let mut end_byte = None;
    for (char_index, (byte_index, _)) in text.char_indices().enumerate() {
        if char_index == range.start {
            start_byte = Some(byte_index);
        }
        if char_index == range.end {
            end_byte = Some(byte_index);
            break;
        }
    }
    let total_chars = text.chars().count();
    if range.start == total_chars {
        start_byte = Some(text.len());
    }
    if range.end == total_chars {
        end_byte = Some(text.len());
    }
    Some(start_byte?..end_byte?)
}

fn rebase_search_highlights(
    highlights: &crate::app::domain::SearchHighlightState,
    char_range: std::ops::Range<usize>,
) -> crate::app::domain::SearchHighlightState {
    let mut rebased = crate::app::domain::SearchHighlightState::default();
    for (index, range) in highlights.ranges.iter().enumerate() {
        let Some(range) = rebase_range(range, &char_range) else {
            continue;
        };
        if highlights.active_range_index == Some(index) {
            rebased.active_range_index = Some(rebased.ranges.len());
        }
        rebased.ranges.push(range);
    }
    rebased
}

fn rebase_range(
    range: &std::ops::Range<usize>,
    char_range: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let start = range.start.max(char_range.start);
    let end = range.end.min(char_range.end);
    (start < end).then_some((start - char_range.start)..(end - char_range.start))
}

fn top_display_row_for_rect(galley_pos: egui::Pos2, clip_rect: egui::Rect, row_height: f32) -> f32 {
    if row_height <= 0.0 {
        return 0.0;
    }

    ((clip_rect.top() - galley_pos.y).max(0.0) / row_height).max(0.0)
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
    view.latest_display_snapshot = None;
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
    if !changed && (view.reveal_request().is_none() || reveal_attempted) {
        view.clear_reveal_request();
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

fn queue_cursor_reveal_intent(
    reveal_request: Option<RevealRequest>,
    view: &mut EditorViewState,
    viewport: Option<egui::Rect>,
    cursor_rect: egui::Rect,
) -> bool {
    let Some(reveal_request) = reveal_request else {
        return false;
    };
    if content_viewport(view, viewport).is_none() {
        return false;
    }

    let align_y = match reveal_request {
        RevealRequest::KeepVisible => {
            scrolling::ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX)
        }
        RevealRequest::Center => scrolling::ScrollAlign::Center,
    };
    let align_x = Some(scrolling::ScrollAlign::NearestWithMargin(0.0));
    view.request_intent(ScrollIntent::Reveal {
        rect: cursor_rect,
        align_y,
        align_x,
    });
    true
}

fn queue_page_navigation_intents(
    ui: &egui::Ui,
    focused: bool,
    view: &mut EditorViewState,
    viewport: Option<egui::Rect>,
    row_height: f32,
) {
    let Some(direction) = page_navigation_direction_sum(ui, focused, viewport, row_height) else {
        return;
    };

    view.request_intent(ScrollIntent::Pages(direction));
}

fn content_viewport(view: &EditorViewState, viewport: Option<egui::Rect>) -> Option<egui::Rect> {
    let _ = view;
    viewport
}

fn page_navigation_direction_sum(
    ui: &egui::Ui,
    focused: bool,
    viewport: Option<egui::Rect>,
    row_height: f32,
) -> Option<i32> {
    page_navigation_delta_size(focused, viewport, row_height)?;
    let direction = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(page_navigation_direction)
            .sum::<f32>()
    });

    (direction != 0.0).then_some(direction.signum() as i32)
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

#[cfg(test)]
mod tests {
    use super::{
        CURSOR_REVEAL_MARGIN_PX, CharCursor, CursorRange, consume_cursor_reveal,
        editor_content_height, editor_desired_size, editor_desired_width,
        queue_cursor_reveal_intent, queue_page_navigation_intents, safe_ime_rects,
        sync_view_cursor_before_render, top_display_row_for_rect,
    };
    use crate::app::domain::{EditorViewState, RevealRequest};
    use crate::app::ui::scrolling::{ScrollAlign, ScrollIntent};
    use eframe::egui;

    #[test]
    fn focused_editor_without_cursor_starts_at_document_beginning() {
        let mut view = EditorViewState::new(1, false);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(
            view.cursor_range,
            Some(CursorRange::one(CharCursor::new(0)))
        );
        assert!(view.reveal_request().is_some());
    }

    #[test]
    fn pending_cursor_range_overrides_missing_native_editor_cursor() {
        let mut view = EditorViewState::new(1, false);
        let pending = CursorRange::one(CharCursor::new(7));
        view.pending_cursor_range = Some(pending);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(view.cursor_range, Some(pending));
        assert_eq!(view.pending_cursor_range, None);
        assert!(view.reveal_request().is_some());
    }

    #[test]
    fn pending_cursor_sync_preserves_existing_reveal_mode() {
        let mut view = EditorViewState::new(1, false);
        let pending = CursorRange::one(CharCursor::new(7));
        view.pending_cursor_range = Some(pending);
        view.request_reveal(RevealRequest::KeepVisible);

        sync_view_cursor_before_render(&mut view, true);

        assert_eq!(view.cursor_range, Some(pending));
        assert_eq!(view.reveal_request(), Some(RevealRequest::KeepVisible));
    }

    #[test]
    fn stable_frame_consumes_scroll_to_cursor_request() {
        let mut view = EditorViewState::new(1, false);
        view.request_reveal(crate::app::domain::view::RevealRequest::KeepVisible);

        consume_cursor_reveal(&mut view, false, true);

        assert!(view.reveal_request().is_none());
    }

    #[test]
    fn changed_frame_keeps_scroll_to_cursor_request() {
        let mut view = EditorViewState::new(1, false);
        view.request_reveal(crate::app::domain::view::RevealRequest::KeepVisible);

        consume_cursor_reveal(&mut view, true, true);

        assert!(view.reveal_request().is_some());
    }

    #[test]
    fn stable_frame_keeps_scroll_to_cursor_until_cursor_reveal_is_attempted() {
        let mut view = EditorViewState::new(1, false);
        view.request_reveal(RevealRequest::KeepVisible);

        consume_cursor_reveal(&mut view, false, false);

        assert!(view.reveal_request().is_some());
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

    #[test]
    fn top_display_row_for_rect_uses_clip_offset() {
        assert_eq!(
            top_display_row_for_rect(
                egui::pos2(0.0, 40.0),
                egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(200.0, 80.0)),
                20.0,
            ),
            3.0
        );
    }

    #[test]
    fn safe_ime_rects_rejects_off_viewport_cursor_rect() {
        let editor_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let cursor_rect = egui::Rect::from_min_size(egui::pos2(0.0, 1.0e9), egui::vec2(2.0, 18.0));

        assert!(safe_ime_rects(editor_rect, cursor_rect).is_none());
    }

    #[test]
    fn safe_ime_rects_clips_cursor_rect_to_editor_viewport() {
        let editor_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let cursor_rect = egui::Rect::from_min_size(egui::pos2(10.0, 199.0), egui::vec2(2.0, 18.0));

        let (_, clipped_cursor) = safe_ime_rects(editor_rect, cursor_rect).expect("safe ime rect");

        assert!(clipped_cursor.max.y <= 202.0);
        assert!(clipped_cursor.min.y >= 197.0);
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
    fn keep_visible_reveal_queues_nearest_margin_intent() {
        let mut view = EditorViewState::new(1, false);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let cursor_rect = egui::Rect::from_min_size(egui::pos2(10.0, 380.0), egui::vec2(2.0, 18.0));

        assert!(queue_cursor_reveal_intent(
            Some(RevealRequest::KeepVisible),
            &mut view,
            Some(viewport),
            cursor_rect,
        ));

        let Some(ScrollIntent::Reveal {
            rect,
            align_y,
            align_x,
        }) = view.pending_intents.last()
        else {
            panic!("expected reveal intent");
        };
        assert_eq!(*rect, cursor_rect);
        assert_eq!(
            *align_y,
            ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX)
        );
        assert_eq!(*align_x, Some(ScrollAlign::NearestWithMargin(0.0)));
    }

    #[test]
    fn centered_reveal_queues_center_intent() {
        let mut view = EditorViewState::new(1, false);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let cursor_rect = egui::Rect::from_min_size(egui::pos2(10.0, 380.0), egui::vec2(2.0, 18.0));

        assert!(queue_cursor_reveal_intent(
            Some(RevealRequest::Center),
            &mut view,
            Some(viewport),
            cursor_rect,
        ));

        let Some(ScrollIntent::Reveal { align_y, .. }) = view.pending_intents.last() else {
            panic!("expected reveal intent");
        };
        assert_eq!(*align_y, ScrollAlign::Center);
    }

    #[test]
    fn page_navigation_requests_page_scroll_intent() {
        let ctx = egui::Context::default();
        let mut view = EditorViewState::new(1, false);
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

            queue_page_navigation_intents(
                ui,
                true,
                &mut view,
                Some(egui::Rect::from_min_max(
                    egui::pos2(0.0, 36.0),
                    egui::pos2(400.0, 216.0),
                )),
                18.0,
            );
        });

        assert!(matches!(
            view.pending_intents.as_slice(),
            [ScrollIntent::Pages(1)]
        ));
    }
}
