use crate::app::domain::{BufferState, EditorViewState, RenderedLayout};
use crate::app::theme::*;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> bool {
    let (mut tracking_layouter, layout_capture) =
        tracked_layouter(editor_font_id.clone(), word_wrap);
    let editor = egui::TextEdit::multiline(&mut buffer.content)
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

    let changed = ui.add(editor).changed();
    view.latest_layout = layout_capture
        .borrow_mut()
        .take()
        .map(RenderedLayout::from_galley);
    changed
}

pub fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut text: String,
    desired_rows: usize,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) {
    let (mut tracking_layouter, layout_capture) =
        tracked_layouter(editor_font_id.clone(), word_wrap);
    let viewer = egui::TextEdit::multiline(&mut text)
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

    ui.add(viewer);
    view.latest_layout = layout_capture
        .borrow_mut()
        .take()
        .map(RenderedLayout::from_galley);
}

pub fn build_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
) -> impl FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley> {
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
    }
}

fn tracked_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
) -> (
    impl FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>,
    Rc<RefCell<Option<Arc<egui::Galley>>>>,
) {
    let mut layouter = build_layouter(font_id, word_wrap);
    let layout_capture = Rc::new(RefCell::new(None));
    let capture_for_layouter = Rc::clone(&layout_capture);
    let tracking_layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let galley = layouter(ui, buf, wrap_width);
        *capture_for_layouter.borrow_mut() = Some(galley.clone());
        galley
    };

    (tracking_layouter, layout_capture)
}
