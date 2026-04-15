use crate::app::domain::{
    BufferState, EditorViewState, RenderedLayout, SearchHighlightState, TextDocumentUndoer,
};
use crate::app::theme::{search_active_match_bg, search_match_bg};
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type TextLayouter = Box<dyn FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>>;
type LayoutCapture = Rc<RefCell<Option<Arc<egui::Galley>>>>;

struct HighlightLayoutStyle<'a> {
    wrap_width: f32,
    word_wrap: bool,
    font_id: &'a egui::FontId,
    text_color: egui::Color32,
    passive_match_bg: egui::Color32,
    active_match_bg: egui::Color32,
}

#[derive(Clone, Copy)]
pub struct TextEditOptions<'a> {
    pub request_focus: bool,
    pub word_wrap: bool,
    pub editor_font_id: &'a egui::FontId,
    pub text_color: egui::Color32,
}

impl<'a> TextEditOptions<'a> {
    pub fn new(
        request_focus: bool,
        word_wrap: bool,
        editor_font_id: &'a egui::FontId,
        text_color: egui::Color32,
    ) -> Self {
        Self {
            request_focus,
            word_wrap,
            editor_font_id,
            text_color,
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
    let (mut tracking_layouter, layout_capture) = tracked_layouter(
        options.editor_font_id.clone(),
        options.word_wrap,
        options.text_color,
        view.search_highlights.clone(),
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

    let output = editor.show(ui);
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
    search_highlights: SearchHighlightState,
) -> TextLayouter {
    Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let passive_match_bg = search_match_bg(ui);
            let active_match_bg = search_active_match_bg(ui);
            let job = layout_job_with_highlights(
                buf.as_str(),
                &search_highlights,
                HighlightLayoutStyle {
                    wrap_width,
                    word_wrap,
                    font_id: &font_id,
                    text_color,
                    passive_match_bg,
                    active_match_bg,
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
    search_highlights: SearchHighlightState,
) -> (TextLayouter, LayoutCapture) {
    let mut layouter = build_layouter(font_id, word_wrap, text_color, search_highlights);
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
    style: HighlightLayoutStyle<'_>,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if style.word_wrap {
        style.wrap_width
    } else {
        f32::INFINITY
    };

    if search_highlights.ranges.is_empty() {
        append_job_segment(
            &mut job,
            text,
            style.font_id,
            style.text_color,
            egui::Color32::TRANSPARENT,
        );
        return job;
    }

    let char_to_byte = char_to_byte_map(text);
    let text_char_len = char_to_byte.len().saturating_sub(1);
    let mut cursor = 0;

    for (index, range) in search_highlights.ranges.iter().enumerate() {
        if range.start >= range.end || range.end > text_char_len {
            continue;
        }
        let start_byte = char_to_byte[range.start];
        let end_byte = char_to_byte[range.end];
        if cursor < start_byte {
            append_job_segment(
                &mut job,
                &text[cursor..start_byte],
                style.font_id,
                style.text_color,
                egui::Color32::TRANSPARENT,
            );
        }
        let background = if search_highlights.active_range_index == Some(index) {
            style.active_match_bg
        } else {
            style.passive_match_bg
        };
        append_job_segment(
            &mut job,
            &text[start_byte..end_byte],
            style.font_id,
            style.text_color,
            background,
        );
        cursor = end_byte;
    }

    if cursor < text.len() {
        append_job_segment(
            &mut job,
            &text[cursor..],
            style.font_id,
            style.text_color,
            egui::Color32::TRANSPARENT,
        );
    }

    job
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

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

#[cfg(test)]
mod tests {
    use super::sync_text_edit_state_before_render;
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
}
