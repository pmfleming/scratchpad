mod highlighting;
mod windowing;

use crate::app::domain::{
    BufferState, EditorViewState, RenderedLayout, RenderedTextWindow, SearchHighlightState,
};
use crate::app::ui::widget_ids;
use eframe::egui;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use highlighting::{
    build_layouter, layout_job_with_highlights, tracked_layouter, windowed_char_range,
    windowed_search_highlights,
};
use windowing::{
    repaint_visible_window_overlay, visible_row_range_for_galley, visible_window_y_offset,
};

type TextLayouter = Box<dyn FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>>;
type LayoutCapture = Rc<RefCell<Option<Arc<egui::Galley>>>>;

#[derive(Clone, Copy)]
pub struct EditorHighlightStyle {
    background: egui::Color32,
    text: egui::Color32,
}

impl EditorHighlightStyle {
    pub fn new(background: egui::Color32, text: egui::Color32) -> Self {
        Self { background, text }
    }

    fn passive_background(self) -> egui::Color32 {
        self.background
    }

    fn active_background(self, dark_mode: bool) -> egui::Color32 {
        if dark_mode {
            highlighting::blend_colors(self.background, egui::Color32::BLACK, 0.18)
        } else {
            highlighting::blend_colors(self.background, egui::Color32::BLACK, 0.28)
        }
    }

    fn text_color(self) -> egui::Color32 {
        self.text
    }
}

#[derive(Clone, Copy)]
pub struct TextEditOptions<'a> {
    pub request_focus: bool,
    pub word_wrap: bool,
    pub editor_font_id: &'a egui::FontId,
    pub text_color: egui::Color32,
    pub highlight_style: EditorHighlightStyle,
}

impl<'a> TextEditOptions<'a> {
    pub fn new(
        request_focus: bool,
        word_wrap: bool,
        editor_font_id: &'a egui::FontId,
        text_color: egui::Color32,
        highlight_style: EditorHighlightStyle,
    ) -> Self {
        Self {
            request_focus,
            word_wrap,
            editor_font_id,
            text_color,
            highlight_style,
        }
    }
}

struct TextEditOutcome {
    changed: bool,
    focused: bool,
    latest_layout: Option<RenderedLayout>,
    visible_row_range: Option<std::ops::Range<usize>>,
    galley_pos: egui::Pos2,
    text_clip_rect: egui::Rect,
    cursor_range: Option<egui::text::CCursorRange>,
}

const VISIBLE_ROW_OVERSCAN: usize = 2;

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
) -> (bool, bool) {
    let line_count = buffer.line_count;
    let mut text = buffer.document().extract_text();
    let mut outcome = render_text_edit_widget(
        ui,
        &mut text,
        view,
        line_count,
        options,
        true,
        options.word_wrap,
    );
    let mut latest_layout = outcome.latest_layout.take();
    if let (Some(layout), Some(visible_row_range)) =
        (latest_layout.as_mut(), outcome.visible_row_range.clone())
        && let Some(char_range) = layout.char_range_for_rows(visible_row_range.clone())
    {
        let visible_text =
            buffer.visible_text_window(visible_row_range, char_range, layout.row_count());
        layout.set_visible_text(visible_text);
    }
    view.latest_layout = latest_layout;
    view.cursor_range = outcome
        .cursor_range
        .map(super::native_editor::CursorRange::from_egui);
    if !options.word_wrap
        && let Some(visible_text) = view
            .latest_layout
            .as_ref()
            .and_then(|layout| layout.visible_text.as_ref())
    {
        repaint_visible_window_overlay(
            ui,
            &outcome,
            visible_text,
            &view.search_highlights,
            options,
        );
    }
    (outcome.changed, outcome.focused)
}

pub fn render_editor_visible_text_window(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
) -> Option<(bool, bool)> {
    if options.word_wrap || options.request_focus {
        return None;
    }

    let visible_lines = previous_layout?.visible_line_range();
    if visible_lines.is_empty() {
        return None;
    }

    let visible_window = buffer.visible_line_window(visible_lines);
    Some(render_visible_text_window(
        ui,
        view,
        visible_window,
        options,
        buffer.line_count,
    ))
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut text: String,
    desired_rows: usize,
    options: TextEditOptions<'_>,
) -> bool {
    let outcome = render_text_edit_widget(ui, &mut text, view, desired_rows, options, false, false);
    view.latest_layout = outcome.latest_layout;
    view.cursor_range = outcome
        .cursor_range
        .map(super::native_editor::CursorRange::from_egui);
    outcome.focused
}

fn editor_widget_id(view_id: u64) -> egui::Id {
    widget_ids::global(("editor_text", view_id))
}

fn preview_widget_id(view_id: u64) -> egui::Id {
    widget_ids::global(("editor_text_preview", view_id))
}

#[allow(clippy::too_many_arguments)]
fn render_text_edit_widget(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    view: &mut EditorViewState,
    desired_rows: usize,
    options: TextEditOptions<'_>,
    interactive: bool,
    paint_overlay: bool,
) -> TextEditOutcome {
    render_text_edit_widget_with_id(
        ui,
        text,
        view,
        desired_rows,
        options,
        interactive,
        editor_widget_id(view.id),
        paint_overlay,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_text_edit_widget_with_id(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    view: &mut EditorViewState,
    desired_rows: usize,
    options: TextEditOptions<'_>,
    interactive: bool,
    widget_id: egui::Id,
    paint_overlay: bool,
) -> TextEditOutcome {
    sync_text_edit_state_before_render(ui.ctx(), widget_id, view);
    let selection_range =
        current_selection_range(ui.ctx(), widget_id).and_then(selection_char_range);
    let (mut tracking_layouter, layout_capture) = tracked_layouter(
        options.editor_font_id.clone(),
        options.word_wrap,
        options.text_color,
        options.highlight_style,
        view.search_highlights.clone(),
        selection_range,
    );
    let mut editor = egui::TextEdit::multiline(text)
        .id(widget_id)
        .font(options.editor_font_id.clone())
        .desired_width(desired_text_width(ui, options.word_wrap))
        .desired_rows(desired_rows)
        .frame(egui::Frame::NONE)
        .lock_focus(true)
        .layouter(&mut tracking_layouter);
    if !interactive {
        editor = editor.interactive(false);
    }

    let output = show_editor_with_selection_highlight(ui, editor, options.highlight_style);
    if paint_overlay {
        repaint_custom_highlights_over_text_edit(
            ui,
            &output,
            layout_capture.borrow().as_ref(),
            options,
        );
    }
    if options.request_focus {
        output.response.response.request_focus();
    }

    let response = &output.response.response;
    let captured_galley = layout_capture.borrow_mut().take();
    let visible_row_range = captured_galley.as_ref().and_then(|galley| {
        visible_row_range_for_galley(galley, output.galley_pos, output.text_clip_rect)
    });
    TextEditOutcome {
        changed: response.changed(),
        focused: response.has_focus() || response.gained_focus(),
        visible_row_range,
        latest_layout: captured_galley.map(RenderedLayout::from_galley),
        galley_pos: output.galley_pos,
        text_clip_rect: output.text_clip_rect,
        cursor_range: output.cursor_range,
    }
}

fn render_visible_text_window(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut visible_window: RenderedTextWindow,
    options: TextEditOptions<'_>,
    total_line_count: usize,
) -> (bool, bool) {
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));
    let top_padding_lines = visible_window.layout_row_offset;
    let bottom_padding_lines = total_line_count.saturating_sub(visible_window.line_range.end);

    if top_padding_lines > 0 {
        ui.add_space(row_height * top_padding_lines as f32);
    }

    let mut preview_view = EditorViewState {
        search_highlights: windowed_search_highlights(
            &view.search_highlights,
            &visible_window.char_range,
        ),
        latest_layout: None,
        cursor_range: None,
        pending_cursor_range: None,
        ..view.clone()
    };
    let mut text = visible_window.text.clone();
    let outcome = render_text_edit_widget_with_id(
        ui,
        &mut text,
        &mut preview_view,
        visible_window.line_range.len().max(1),
        options,
        false,
        preview_widget_id(view.id),
        false,
    );

    let mut latest_layout = outcome.latest_layout;
    if let Some(layout) = latest_layout.as_mut() {
        layout.offset_line_numbers(visible_window.line_range.start);
        visible_window.row_range = 0..layout.row_count();
        layout.set_visible_text(visible_window);
    }
    view.latest_layout = latest_layout;

    if bottom_padding_lines > 0 {
        ui.add_space(row_height * bottom_padding_lines as f32);
    }

    (false, outcome.focused)
}

fn show_editor_with_selection_highlight(
    ui: &mut egui::Ui,
    editor: egui::TextEdit<'_>,
    _highlight_style: EditorHighlightStyle,
) -> egui::text_edit::TextEditOutput {
    ui.scope(|ui| {
        ui.visuals_mut().selection = transparent_selection_style();
        editor.show(ui)
    })
    .inner
}

fn repaint_custom_highlights_over_text_edit(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    overlay_galley: Option<&Arc<egui::Galley>>,
    options: TextEditOptions<'_>,
) {
    let Some(cursor_range) = output.cursor_range else {
        return;
    };
    if selection_char_range(cursor_range).is_none() {
        return;
    }
    let Some(galley) = overlay_galley.cloned() else {
        return;
    };

    let painter = ui.painter_at(output.text_clip_rect.expand(1.0));
    painter.galley(
        output.galley_pos - egui::vec2(galley.rect.left(), 0.0),
        galley.clone(),
        options.text_color,
    );

    if output.response.response.has_focus() {
        paint_text_cursor(
            ui,
            &painter,
            &galley,
            output.galley_pos,
            cursor_range.primary,
            options.editor_font_id,
        );
    }
}

fn transparent_selection_style() -> egui::style::Selection {
    egui::style::Selection {
        bg_fill: egui::Color32::TRANSPARENT,
        stroke: egui::Stroke::NONE,
    }
}

fn sync_text_edit_state_before_render(
    ctx: &egui::Context,
    widget_id: egui::Id,
    view: &mut EditorViewState,
) {
    let mut state = egui::TextEdit::load_state(ctx, widget_id).unwrap_or_default();
    let should_restore_view_cursor = state.cursor.char_range().is_none();
    if let Some(cursor_range) = view
        .pending_cursor_range
        .take()
        .or(should_restore_view_cursor
            .then_some(view.cursor_range)
            .flatten())
    {
        state.cursor.set_char_range(Some(cursor_range.to_egui()));
    }
    egui::TextEdit::store_state(ctx, widget_id, state);
}

fn desired_text_width(ui: &egui::Ui, word_wrap: bool) -> f32 {
    if word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    }
}

fn paint_text_cursor(
    ui: &egui::Ui,
    painter: &egui::Painter,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    cursor: egui::text::CCursor,
    _font_id: &egui::FontId,
) {
    let cursor_rect = galley
        .pos_from_cursor(cursor)
        .expand(1.5)
        .translate(galley_pos.to_vec2());
    let top = cursor_rect.center_top();
    let bottom = cursor_rect.center_bottom();
    let stroke = ui.visuals().text_cursor.stroke;
    painter.line_segment([top, bottom], (stroke.width, stroke.color));
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn current_selection_range(
    ctx: &egui::Context,
    widget_id: egui::Id,
) -> Option<egui::text::CCursorRange> {
    egui::TextEdit::load_state(ctx, widget_id)?
        .cursor
        .char_range()
}

fn selection_char_range(cursor_range: egui::text::CCursorRange) -> Option<std::ops::Range<usize>> {
    let [left, right] = [cursor_range.primary.index, cursor_range.secondary.index];
    let (start, end) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    (start < end).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::{
        SearchHighlightState, selection_char_range, sync_text_edit_state_before_render,
        transparent_selection_style, visible_row_range_for_galley, visible_window_y_offset,
        windowed_char_range, windowed_search_highlights,
    };
    use crate::app::domain::{EditorViewState, RenderedTextWindow};
    use crate::app::ui::editor_content::native_editor::CursorRange;
    use eframe::egui;

    fn range(start: usize, end: usize) -> egui::text::CCursorRange {
        egui::text::CCursorRange::two(
            egui::text::CCursor::new(start),
            egui::text::CCursor::new(end),
        )
    }

    #[test]
    fn existing_text_edit_state_is_not_overwritten_by_stored_view_cursor() {
        let ctx = egui::Context::default();
        let widget_id = widget_ids::global("selection-sync");
        let live_selection = range(2, 6);
        let stale_view_selection = CursorRange::two(2, 3);

        let mut state = egui::widgets::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(live_selection));
        egui::TextEdit::store_state(&ctx, widget_id, state);

        let mut view = EditorViewState::new(1, false);
        view.cursor_range = Some(stale_view_selection);

        sync_text_edit_state_before_render(&ctx, widget_id, &mut view);

        let stored = egui::TextEdit::load_state(&ctx, widget_id).expect("text edit state");
        assert_eq!(stored.cursor.char_range(), Some(live_selection));
    }

    #[test]
    fn pending_cursor_range_overrides_existing_text_edit_state() {
        let ctx = egui::Context::default();
        let widget_id = widget_ids::global("pending-selection-sync");
        let existing_selection = range(4, 8);
        let requested_selection = CursorRange::two(10, 14);

        let mut state = egui::widgets::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(existing_selection));
        egui::TextEdit::store_state(&ctx, widget_id, state);

        let mut view = EditorViewState::new(2, false);
        view.pending_cursor_range = Some(requested_selection);

        sync_text_edit_state_before_render(&ctx, widget_id, &mut view);

        let stored = egui::TextEdit::load_state(&ctx, widget_id).expect("text edit state");
        assert_eq!(
            stored.cursor.char_range(),
            Some(requested_selection.to_egui())
        );
        assert_eq!(view.pending_cursor_range, None);
    }

    #[test]
    fn transparent_selection_style_disables_native_selection_paint() {
        let selection = transparent_selection_style();

        assert_eq!(selection.bg_fill, egui::Color32::TRANSPARENT);
        assert_eq!(selection.stroke, egui::Stroke::NONE);
    }

    #[test]
    fn empty_cursor_range_does_not_become_selection_range() {
        let cursor = egui::text::CCursor::new(5);

        assert_eq!(
            selection_char_range(egui::text::CCursorRange::two(cursor, cursor)),
            None
        );
    }

    #[test]
    fn visible_row_range_tracks_viewport_with_overscan() {
        let ctx = egui::Context::default();
        let mut visible = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = (0..8)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n");
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    400.0,
                ))
            });
            let target_row = &galley.rows[3];
            let clip_rect = target_row
                .rect()
                .translate(egui::vec2(0.0, 20.0))
                .expand2(egui::vec2(0.0, -1.0));

            visible = visible_row_range_for_galley(&galley, egui::pos2(0.0, 20.0), clip_rect);
        });

        assert_eq!(visible, Some(1..6));
    }

    #[test]
    fn windowed_search_highlights_rebases_ranges_into_visible_slice() {
        let highlights = SearchHighlightState {
            ranges: vec![2..6, 10..16, 18..20],
            active_range_index: Some(1),
        };

        let rebased = windowed_search_highlights(&highlights, &(8..18));

        assert_eq!(rebased.ranges, vec![2..8]);
        assert_eq!(rebased.active_range_index, Some(0));
    }

    #[test]
    fn windowed_search_highlights_clips_partial_matches_at_window_edges() {
        let highlights = SearchHighlightState {
            ranges: vec![4..9, 12..18],
            active_range_index: Some(0),
        };

        let rebased = windowed_search_highlights(&highlights, &(6..14));

        assert_eq!(rebased.ranges, vec![0..3, 6..8]);
        assert_eq!(rebased.active_range_index, Some(0));
    }

    #[test]
    fn windowed_char_range_clips_selection_to_visible_slice() {
        assert_eq!(windowed_char_range(Some(4..12), &(6..10)), Some(0..4));
        assert_eq!(windowed_char_range(Some(0..4), &(6..10)), None);
    }

    #[test]
    fn visible_window_y_offset_prefers_layout_offset_when_present() {
        let visible_window = RenderedTextWindow {
            row_range: 0..3,
            line_range: 40..43,
            char_range: 100..130,
            layout_row_offset: 40,
            text: "line 40\nline 41\nline 42\n".to_owned(),
            truncated_start: true,
            truncated_end: true,
        };

        assert_eq!(visible_window_y_offset(&visible_window, 18.0), 720.0);
    }

    #[test]
    fn visible_window_y_offset_uses_row_range_for_live_editor_windows() {
        let visible_window = RenderedTextWindow {
            row_range: 6..10,
            line_range: 6..10,
            char_range: 60..100,
            layout_row_offset: 0,
            text: "line 6\nline 7\nline 8\nline 9\n".to_owned(),
            truncated_start: true,
            truncated_end: true,
        };

        assert_eq!(visible_window_y_offset(&visible_window, 18.0), 108.0);
    }
}
