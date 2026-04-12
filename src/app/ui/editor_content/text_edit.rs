use crate::app::domain::{BufferState, EditorViewState, RenderedLayout};
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type TextLayouter = Box<dyn FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>>;
type LayoutCapture = Rc<RefCell<Option<Arc<egui::Galley>>>>;

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
}

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
) -> (bool, bool) {
    let outcome = render_text_edit_widget(
        ui,
        &mut buffer.content,
        view.id,
        buffer.line_count,
        options,
        true,
    );
    view.latest_layout = outcome.latest_layout;
    (outcome.changed, outcome.focused)
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut text: String,
    desired_rows: usize,
    options: TextEditOptions<'_>,
) -> bool {
    let outcome = render_text_edit_widget(ui, &mut text, view.id, desired_rows, options, false);
    view.latest_layout = outcome.latest_layout;
    outcome.focused
}

fn editor_widget_id(view_id: u64) -> egui::Id {
    egui::Id::new(("editor_text", view_id))
}

fn render_text_edit_widget(
    ui: &mut egui::Ui,
    text: &mut String,
    view_id: u64,
    desired_rows: usize,
    options: TextEditOptions<'_>,
    interactive: bool,
) -> TextEditOutcome {
    let (mut tracking_layouter, layout_capture) = tracked_layouter(
        options.editor_font_id.clone(),
        options.word_wrap,
        options.text_color,
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
    }
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
) -> TextLayouter {
    Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
            job.append(
                buf.as_str(),
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: text_color,
                    ..Default::default()
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
) -> (TextLayouter, LayoutCapture) {
    let mut layouter = build_layouter(font_id, word_wrap, text_color);
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
