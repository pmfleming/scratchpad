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

use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState, RenderedLayout};
use eframe::egui;
use interactions::{
    handle_keyboard_events, handle_mouse_interaction, sync_view_cursor_before_render,
};
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

    // Page navigation: emit a `ScrollIntent::Pages` signed by the direction
    // of the consumed PageUp/PageDown event. The cursor itself was already
    // advanced by the keyboard handler above; this intent advances the
    // viewport anchor on the next frame's drain so the keyboard cursor
    // change and the viewport scroll stay in sync via the single
    // `ScrollManager` mutation path.
    if focused && let Some(direction) = consumed_page_navigation_direction(ui) {
        view.request_intent(crate::app::ui::scrolling::ScrollIntent::Pages(direction));
    }

    let galley_pos = rect.min;
    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui, &galley, galley_pos, rect, view, options, focused, changed, viewport,
        )
    } else {
        CursorPaintOutcome::default()
    };
    let requested_scroll_offset = paint_outcome.requested_scroll_offset;

    // Consume scroll flag once the galley is fresh (scroll was applied)
    consume_cursor_reveal(view, changed, paint_outcome.reveal_attempted);
    sync_ime_output_focus(view, focused);

    if changed {
        clear_latest_layout(view);
        view.latest_display_snapshot = None;
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
        // Phase 3 plumbing: in addition to the legacy RenderedLayout, build a
        // wrap-aware DisplaySnapshot from the freshly painted galley and
        // stash it on the view. Viewport-first queries (cursor reveal row
        // resolution, anchor-to-row conversion for piece-backed anchors)
        // read from this. Building always: the snapshot is small and the
        // render path doesn't ship the galley elsewhere.
        view.latest_display_snapshot = Some(
            crate::app::ui::scrolling::DisplaySnapshot::from_galley(galley.clone(), row_height),
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

fn content_viewport(view: &EditorViewState, viewport: Option<egui::Rect>) -> Option<egui::Rect> {
    let _ = view;
    viewport
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
        CharCursor, CursorRange, consume_cursor_reveal, consumed_page_navigation_direction,
        editor_content_height, editor_desired_size, editor_desired_width,
        scroll_offset_to_center_rect_vertically, scroll_offset_to_keep_rect_visible,
        sync_view_cursor_before_render,
    };
    use crate::app::domain::{CursorRevealMode, EditorViewState};
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
