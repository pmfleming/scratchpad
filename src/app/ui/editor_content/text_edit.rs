use crate::app::domain::{
    BufferState, EditorViewState, RenderedLayout, SearchHighlightState, TextDocumentUndoer,
};
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

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
            blend_colors(self.background, egui::Color32::BLACK, 0.18)
        } else {
            blend_colors(self.background, egui::Color32::BLACK, 0.28)
        }
    }

    fn text_color(self) -> egui::Color32 {
        self.text
    }
}

struct HighlightLayoutStyle<'a> {
    wrap_width: f32,
    word_wrap: bool,
    font_id: &'a egui::FontId,
    text_color: egui::Color32,
    highlight: EditorHighlightStyle,
    dark_mode: bool,
}

#[derive(Clone, Copy)]
enum HighlightKind {
    Selection,
    SearchActive,
    SearchPassive,
}

#[derive(Clone)]
struct TextHighlightRange {
    range: std::ops::Range<usize>,
    kind: HighlightKind,
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
    cursor_range: Option<egui::text::CCursorRange>,
    undoer: Option<TextDocumentUndoer>,
}

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
) -> (bool, bool) {
    let line_count = buffer.line_count;
    let undoer = buffer.document().undoer();
    let outcome = render_text_edit_widget(
        ui,
        buffer.document_mut(),
        view,
        line_count,
        options,
        true,
        Some(undoer),
    );
    view.latest_layout = outcome.latest_layout;
    view.cursor_range = outcome.cursor_range;
    if let Some(undoer) = outcome.undoer {
        buffer.document_mut().set_undoer(undoer);
    }
    (outcome.changed, outcome.focused)
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut text: String,
    desired_rows: usize,
    options: TextEditOptions<'_>,
) -> bool {
    let outcome = render_text_edit_widget(ui, &mut text, view, desired_rows, options, false, None);
    view.latest_layout = outcome.latest_layout;
    view.cursor_range = outcome.cursor_range;
    outcome.focused
}

fn editor_widget_id(view_id: u64) -> egui::Id {
    egui::Id::new(("editor_text", view_id))
}

fn render_text_edit_widget(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    view: &mut EditorViewState,
    desired_rows: usize,
    options: TextEditOptions<'_>,
    interactive: bool,
    undoer: Option<TextDocumentUndoer>,
) -> TextEditOutcome {
    let view_id = view.id;
    let widget_id = editor_widget_id(view_id);
    sync_text_edit_state_before_render(ui.ctx(), widget_id, view, undoer);
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
        .id(editor_widget_id(view_id))
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
    repaint_custom_highlights_over_text_edit(
        ui,
        &output,
        layout_capture.borrow().as_ref(),
        options,
    );
    if options.request_focus {
        output.response.response.request_focus();
    }

    let response = &output.response.response;
    TextEditOutcome {
        changed: response.changed(),
        focused: response.has_focus() || response.gained_focus(),
        latest_layout: layout_capture
            .borrow_mut()
            .take()
            .map(RenderedLayout::from_galley),
        cursor_range: output.cursor_range,
        undoer: interactive.then(|| output.state.undoer()),
    }
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
    undoer: Option<TextDocumentUndoer>,
) {
    let mut state = egui::TextEdit::load_state(ctx, widget_id).unwrap_or_default();
    if let Some(undoer) = undoer {
        state.set_undoer(undoer);
    }
    let should_restore_view_cursor = state.cursor.char_range().is_none();
    if let Some(cursor_range) = view
        .pending_cursor_range
        .take()
        .or(should_restore_view_cursor
            .then_some(view.cursor_range)
            .flatten())
    {
        state.cursor.set_char_range(Some(cursor_range));
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

pub fn build_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
    text_color: egui::Color32,
    highlight_style: EditorHighlightStyle,
    search_highlights: SearchHighlightState,
    selection_range: Option<std::ops::Range<usize>>,
) -> TextLayouter {
    Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let job = layout_job_with_highlights(
                buf.as_str(),
                &search_highlights,
                selection_range.clone(),
                HighlightLayoutStyle {
                    wrap_width,
                    word_wrap,
                    font_id: &font_id,
                    text_color,
                    highlight: highlight_style,
                    dark_mode: ui.visuals().dark_mode,
                },
            );
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        },
    )
}

fn tracked_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
    text_color: egui::Color32,
    highlight_style: EditorHighlightStyle,
    search_highlights: SearchHighlightState,
    selection_range: Option<std::ops::Range<usize>>,
) -> (TextLayouter, LayoutCapture) {
    let mut layouter = build_layouter(
        font_id,
        word_wrap,
        text_color,
        highlight_style,
        search_highlights,
        selection_range,
    );
    let layout_capture = Rc::new(RefCell::new(None));
    let capture_for_layouter = Rc::clone(&layout_capture);
    let tracking_layouter = Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let galley = layouter(ui, buf, wrap_width);
            *capture_for_layouter.borrow_mut() = Some(galley.clone());
            galley
        },
    );

    (tracking_layouter, layout_capture)
}

fn layout_job_with_highlights(
    text: &str,
    search_highlights: &SearchHighlightState,
    selection_range: Option<std::ops::Range<usize>>,
    style: HighlightLayoutStyle<'_>,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if style.word_wrap {
        style.wrap_width
    } else {
        f32::INFINITY
    };

    let char_to_byte = char_to_byte_map(text);
    let text_char_len = char_to_byte.len().saturating_sub(1);
    let highlights =
        merged_highlight_ranges(search_highlights, selection_range, text_char_len);

    if highlights.is_empty() {
        append_job_segment(
            &mut job,
            text,
            style.font_id,
            style.text_color,
            egui::Color32::TRANSPARENT,
        );
        return job;
    }

    let mut boundaries = vec![0, text_char_len];
    for highlight in &highlights {
        boundaries.push(highlight.range.start);
        boundaries.push(highlight.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    for window in boundaries.windows(2) {
        let segment_start = window[0];
        let segment_end = window[1];
        if segment_start >= segment_end || segment_end > text_char_len {
            continue;
        }
        let start_byte = char_to_byte[segment_start];
        let end_byte = char_to_byte[segment_end];
        let kind = highlight_kind_for_segment(&highlights, segment_start);
        let (text_color, background) = match kind {
            Some(HighlightKind::Selection | HighlightKind::SearchActive) => (
                style.highlight.text_color(),
                style.highlight.active_background(style.dark_mode),
            ),
            Some(HighlightKind::SearchPassive) => (
                style.highlight.text_color(),
                style.highlight.passive_background(),
            ),
            None => (style.text_color, egui::Color32::TRANSPARENT),
        };
        append_job_segment(
            &mut job,
            &text[start_byte..end_byte],
            style.font_id,
            text_color,
            background,
        );
    }

    job
}

fn merged_highlight_ranges(
    search_highlights: &SearchHighlightState,
    selection_range: Option<std::ops::Range<usize>>,
    text_char_len: usize,
) -> Vec<TextHighlightRange> {
    let mut highlights = Vec::new();
    if let Some(range) = selection_range.filter(|range| range.end <= text_char_len) {
        highlights.push(TextHighlightRange {
            range,
            kind: HighlightKind::Selection,
        });
    }
    for (index, range) in search_highlights.ranges.iter().enumerate() {
        if range.start >= range.end || range.end > text_char_len {
            continue;
        }
        highlights.push(TextHighlightRange {
            range: range.clone(),
            kind: if search_highlights.active_range_index == Some(index) {
                HighlightKind::SearchActive
            } else {
                HighlightKind::SearchPassive
            },
        });
    }
    highlights
}

fn highlight_kind_for_segment(
    highlights: &[TextHighlightRange],
    segment_start: usize,
) -> Option<HighlightKind> {
    let contains = |highlight: &TextHighlightRange| {
        highlight.range.start <= segment_start && segment_start < highlight.range.end
    };
    if highlights
        .iter()
        .any(|highlight| contains(highlight) && matches!(highlight.kind, HighlightKind::Selection))
    {
        Some(HighlightKind::Selection)
    } else if highlights
        .iter()
        .any(|highlight| contains(highlight) && matches!(highlight.kind, HighlightKind::SearchActive))
    {
        Some(HighlightKind::SearchActive)
    } else if highlights
        .iter()
        .any(|highlight| contains(highlight) && matches!(highlight.kind, HighlightKind::SearchPassive))
    {
        Some(HighlightKind::SearchPassive)
    } else {
        None
    }
}

fn append_job_segment(
    job: &mut egui::text::LayoutJob,
    text: &str,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    background: egui::Color32,
) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font_id.clone(),
            color: text_color,
            background,
            ..Default::default()
        },
    );
}

fn blend_colors(left: egui::Color32, right: egui::Color32, right_weight: f32) -> egui::Color32 {
    let right_weight = right_weight.clamp(0.0, 1.0);
    let left_weight = 1.0 - right_weight;
    let channel = |left: u8, right: u8| {
        ((left as f32 * left_weight) + (right as f32 * right_weight)).round() as u8
    };

    egui::Color32::from_rgb(
        channel(left.r(), right.r()),
        channel(left.g(), right.g()),
        channel(left.b(), right.b()),
    )
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
    egui::TextEdit::load_state(ctx, widget_id)?.cursor.char_range()
}

fn selection_char_range(
    cursor_range: egui::text::CCursorRange,
) -> Option<std::ops::Range<usize>> {
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
    use super::{selection_char_range, sync_text_edit_state_before_render, transparent_selection_style};
    use crate::app::domain::EditorViewState;
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
        let widget_id = egui::Id::new("selection-sync");
        let live_selection = range(2, 6);
        let stale_view_selection = range(2, 3);

        let mut state = egui::widgets::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(live_selection));
        egui::TextEdit::store_state(&ctx, widget_id, state);

        let mut view = EditorViewState::new(1, false);
        view.cursor_range = Some(stale_view_selection);

        sync_text_edit_state_before_render(&ctx, widget_id, &mut view, None);

        let stored = egui::TextEdit::load_state(&ctx, widget_id).expect("text edit state");
        assert_eq!(stored.cursor.char_range(), Some(live_selection));
    }

    #[test]
    fn pending_cursor_range_overrides_existing_text_edit_state() {
        let ctx = egui::Context::default();
        let widget_id = egui::Id::new("pending-selection-sync");
        let existing_selection = range(4, 8);
        let requested_selection = range(10, 14);

        let mut state = egui::widgets::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(existing_selection));
        egui::TextEdit::store_state(&ctx, widget_id, state);

        let mut view = EditorViewState::new(2, false);
        view.pending_cursor_range = Some(requested_selection);

        sync_text_edit_state_before_render(&ctx, widget_id, &mut view, None);

        let stored = egui::TextEdit::load_state(&ctx, widget_id).expect("text edit state");
        assert_eq!(stored.cursor.char_range(), Some(requested_selection));
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
}
