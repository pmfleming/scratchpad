use crate::app::domain::{BufferState, EditorViewState, RenderedLayout, display_line_count};
use crate::app::theme::*;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub(crate) fn render_editor_content(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> bool {
    egui::Frame::NONE
        .fill(EDITOR_BG)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.horizontal_top(|ui| {
                if view.show_line_numbers {
                    render_line_number_gutter(ui, buffer, view, previous_layout, editor_font_id);
                    ui.separator();
                }

                if buffer.artifact_summary.has_control_chars() {
                    render_artifact_view(ui, buffer, view, word_wrap, editor_font_id)
                } else {
                    render_editor_text_edit(ui, buffer, view, word_wrap, editor_font_id)
                }
            })
            .inner
        })
        .inner
}

fn render_line_number_gutter(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    font_id: &egui::FontId,
) {
    let fallback_line_count = displayed_line_count(buffer, view);
    let max_number = previous_layout
        .and_then(|layout| {
            layout
                .row_line_numbers
                .iter()
                .rev()
                .flatten()
                .copied()
                .next()
        })
        .unwrap_or(fallback_line_count);
    let digits = max_number.max(1).to_string().len().max(3);
    let gutter_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap("0".repeat(digits), font_id.clone(), TEXT_MUTED)
            .size()
            .x
    }) + 16.0;

    ui.allocate_ui_with_layout(
        egui::vec2(gutter_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0.0, HEADER_BG);
            ui.set_width(gutter_width);

            if let Some(layout) = previous_layout {
                render_layout_gutter_rows(ui, layout, font_id);
            } else {
                render_fallback_gutter_rows(ui, fallback_line_count, font_id);
            }
        },
    );
}

fn render_layout_gutter_rows(ui: &mut egui::Ui, layout: &RenderedLayout, font_id: &egui::FontId) {
    let desired_size = egui::vec2(
        ui.available_width(),
        layout.galley.rect.height().max(ui.available_height()),
    );
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter();

    for (row_index, row) in layout.galley.rows.iter().enumerate() {
        let Some(line_number) = layout
            .row_line_numbers
            .get(row_index)
            .and_then(|line_number| *line_number)
        else {
            continue;
        };

        painter.text(
            egui::pos2(rect.right() - 8.0, rect.top() + row.pos.y),
            egui::Align2::RIGHT_TOP,
            line_number.to_string(),
            font_id.clone(),
            TEXT_MUTED,
        );
    }
}

fn render_fallback_gutter_rows(ui: &mut egui::Ui, line_count: usize, font_id: &egui::FontId) {
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(font_id));
    let row_count = line_count.max(1);
    let desired_size = egui::vec2(ui.available_width(), row_height * row_count as f32);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter();

    for row_index in 0..row_count {
        painter.text(
            egui::pos2(
                rect.right() - 8.0,
                rect.top() + row_height * row_index as f32,
            ),
            egui::Align2::RIGHT_TOP,
            (row_index + 1).to_string(),
            font_id.clone(),
            TEXT_MUTED,
        );
    }
}

fn displayed_line_count(buffer: &BufferState, view: &EditorViewState) -> usize {
    if buffer.artifact_summary.has_control_chars() && !view.show_control_chars {
        display_line_count(&make_control_chars_clean(&buffer.content))
    } else {
        buffer.line_count
    }
}

fn render_artifact_view(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> bool {
    if view.show_control_chars {
        render_read_only_text_edit(
            ui,
            view,
            make_control_chars_visible(&buffer.content),
            buffer.line_count,
            word_wrap,
            editor_font_id,
        );
    } else {
        let clean_text = make_control_chars_clean(&buffer.content);
        let desired_rows = display_line_count(&clean_text);
        render_read_only_text_edit(
            ui,
            view,
            clean_text,
            desired_rows,
            word_wrap,
            editor_font_id,
        );
    }
    false
}

fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> bool {
    let mut layouter = build_layouter(editor_font_id.clone(), word_wrap);
    let layout_capture = Rc::new(RefCell::new(None));
    let capture_for_layouter = Rc::clone(&layout_capture);
    let mut tracking_layouter =
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let galley = layouter(ui, buf, wrap_width);
            *capture_for_layouter.borrow_mut() = Some(galley.clone());
            galley
        };
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

fn render_read_only_text_edit(
    ui: &mut egui::Ui,
    view: &mut EditorViewState,
    mut text: String,
    desired_rows: usize,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) {
    let mut layouter = build_layouter(editor_font_id.clone(), word_wrap);
    let layout_capture = Rc::new(RefCell::new(None));
    let capture_for_layouter = Rc::clone(&layout_capture);
    let mut tracking_layouter =
        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let galley = layouter(ui, buf, wrap_width);
            *capture_for_layouter.borrow_mut() = Some(galley.clone());
            galley
        };
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

fn build_layouter(
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

pub(crate) fn make_control_chars_visible(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1B}' => visible.push('␛'),
            '\u{0008}' => visible.push('␈'),
            '\t' => visible.push('→'),
            '\r' if chars.peek() == Some(&'\n') => visible.push('␍'),
            '\r' => visible.push('␍'),
            _ if ch.is_control() && ch != '\n' => {
                visible.push_str(&format!("\\x{:02X}", ch as u32));
            }
            _ => visible.push(ch),
        }
    }

    visible
}

pub(crate) fn make_control_chars_clean(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1B}' => skip_ansi_sequence(&mut chars),
            '\u{0008}' => {
                clean.pop();
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => {}
            '\n' | '\t' => clean.push(ch),
            _ if ch.is_control() => {}
            _ => clean.push(ch),
        }
    }

    clean
}

fn skip_ansi_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        }
        Some(']') => {
            chars.next();
            while let Some(ch) = chars.next() {
                if ch == '\u{0007}' {
                    break;
                }
                if ch == '\u{1B}' && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        }
        _ => {}
    }
}
