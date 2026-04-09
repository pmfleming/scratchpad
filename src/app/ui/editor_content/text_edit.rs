use crate::app::domain::{BufferState, EditorViewState, RenderedLayout};
use crate::app::theme::*;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type TextLayouter = Box<dyn FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>>;
type LayoutCapture = Rc<RefCell<Option<Arc<egui::Galley>>>>;

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    request_focus: bool,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> (bool, bool) {
    let (mut tracking_layouter, layout_capture) =
        tracked_layouter(editor_font_id.clone(), word_wrap);
    let editor_id = editor_widget_id(view.id);
    let editor = egui::TextEdit::multiline(&mut buffer.content)
        .id(editor_id)
        .font(editor_font_id.clone())
        .desired_width(if word_wrap {
            ui.available_width()
        } else {
            f32::INFINITY
        })
        .desired_rows(buffer.line_count)
        .frame(egui::Frame::NONE)
        .lock_focus(true)
        .layouter(&mut tracking_layouter);

    let output = editor.show(ui);
    if request_focus {
        output.response.response.request_focus();
    }
    let response = &output.response.response;
    let changed = response.changed();
    let focused = response.has_focus() || response.gained_focus();
    view.latest_layout = layout_capture
        .borrow_mut()
        .take()
        .map(RenderedLayout::from_galley);
    (changed, focused)
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut text: String,
    desired_rows: usize,
    request_focus: bool,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> bool {
    let (mut tracking_layouter, layout_capture) =
        tracked_layouter(editor_font_id.clone(), word_wrap);
    let editor_id = editor_widget_id(view.id);
    let viewer = egui::TextEdit::multiline(&mut text)
        .id(editor_id)
        .font(editor_font_id.clone())
        .desired_width(if word_wrap {
            ui.available_width()
        } else {
            f32::INFINITY
        })
        .desired_rows(desired_rows)
        .interactive(false)
        .frame(egui::Frame::NONE)
        .lock_focus(true)
        .layouter(&mut tracking_layouter);

    let output = viewer.show(ui);
    if request_focus {
        output.response.response.request_focus();
    }
    let response = &output.response.response;
    view.latest_layout = layout_capture
        .borrow_mut()
        .take()
        .map(RenderedLayout::from_galley);
    response.has_focus() || response.gained_focus()
}

fn editor_widget_id(view_id: u64) -> egui::Id {
    egui::Id::new(("editor_text", view_id))
}

pub fn build_layouter(font_id: egui::FontId, word_wrap: bool) -> TextLayouter {
    Box::new(
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
            job.append(
                buf.as_str(),
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: TEXT_PRIMARY,
                    ..Default::default()
                },
            );
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        },
    )
}

fn tracked_layouter(font_id: egui::FontId, word_wrap: bool) -> (TextLayouter, LayoutCapture) {
    let mut layouter = build_layouter(font_id, word_wrap);
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
