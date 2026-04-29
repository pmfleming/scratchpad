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

use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};
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
    let mut document_revision = buffer.document_revision();
    let total_chars = buffer.current_file_length().chars;
    let mut galley = build_editor_galley(ui, buffer, view, options, viewport);

    let row_height = editor_row_height(ui, options.editor_font_id);
    let (rect, response) = allocate_editor_rect(ui, &galley, options, row_height, viewport);
    request_editor_focus(ui, &response, options.request_focus);

    let input = process_editor_input(
        ui,
        buffer,
        view,
        EditorInputRequest {
            response: &response,
            galley: &galley,
            rect,
            options,
            viewport,
            row_height,
            total_chars,
        },
    );

    if input.changed {
        document_revision = buffer.document_revision();
        galley = build_editor_galley(ui, buffer, view, options, viewport);
    }

    let galley_pos = rect.min;
    let paint_outcome = if ui.is_rect_visible(rect) {
        paint_editor(
            ui,
            &galley,
            galley_pos,
            rect,
            view,
            options,
            input.focused,
            false,
        )
    } else {
        CursorPaintOutcome::default()
    };
    consume_cursor_reveal(view, false, paint_outcome.reveal_attempted);
    sync_ime_output_focus(view, input.focused);

    store_latest_snapshot(view, &galley, row_height, false, Some(document_revision));

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
}

fn process_editor_input(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request: EditorInputRequest<'_>,
) -> EditorInputOutcome {
    let prev_cursor = view.cursor_range;
    handle_mouse_interaction(
        ui,
        request.response,
        request.galley,
        request.rect,
        view,
        buffer.document().piece_tree(),
    );
    let focused = request.response.has_focus()
        || request.response.gained_focus()
        || request.options.request_focus;
    sync_view_cursor_before_render(view, focused);
    let changed = handle_focused_keyboard_input(ui, buffer, view, &request, focused);
    if view.cursor_range != prev_cursor {
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    }
    publish_active_selection(buffer, view, focused);
    request_page_navigation_intent(ui, view, focused);
    EditorInputOutcome { focused, changed }
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
    store_latest_snapshot(view, &galley, row_height, false, None);
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
            .pos_from_cursor(cursor_range.primary.to_egui_ccursor())
            .expand(1.5);
        let cursor_rect = content_cursor_rect.translate(galley_pos.to_vec2());
        return paint_cursor_effects(ui, rect, cursor_rect, content_cursor_rect, view);
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
                ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX)
            }
            CursorRevealMode::Center => ScrollAlign::Center,
        };
        view.request_intent(ScrollIntent::Reveal {
            rect: cursor_rect_content,
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
    view: &EditorViewState,
    options: TextEditOptions<'_>,
    viewport: Option<egui::Rect>,
) -> Arc<egui::Galley> {
    let text = buffer.document().text_cow();
    highlighting::build_galley(
        ui,
        text.as_ref(),
        options,
        &view.search_highlights,
        buffer.active_selection.clone(),
        editor_wrap_width(ui, options.word_wrap, viewport),
    )
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
) {
    if changed {
        view.latest_display_snapshot = None;
        view.latest_display_snapshot_revision = None;
    } else {
        view.latest_display_snapshot =
            Some(DisplaySnapshot::from_galley(galley.clone(), row_height));
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
