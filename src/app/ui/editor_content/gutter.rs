use super::artifact::make_control_chars_clean;
use crate::app::domain::{BufferState, EditorViewState, RenderedLayout, display_line_count};
use eframe::egui;

pub fn render_line_number_gutter(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    background_color: egui::Color32,
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
            .layout_no_wrap(
                "0".repeat(digits),
                font_id.clone(),
                text_color.gamma_multiply(0.62),
            )
            .size()
            .x
    }) + 16.0;

    ui.allocate_ui_with_layout(
        egui::vec2(gutter_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, background_color);
            ui.set_width(gutter_width);

            if let Some(layout) = previous_layout {
                render_layout_gutter_rows(ui, layout, font_id, text_color);
            } else {
                render_fallback_gutter_rows(ui, fallback_line_count, font_id, text_color);
            }
        },
    );
}

fn render_layout_gutter_rows(
    ui: &mut egui::Ui,
    layout: &RenderedLayout,
    font_id: &egui::FontId,
    text_color: egui::Color32,
) {
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
            text_color.gamma_multiply(0.62),
        );
    }
}

fn render_fallback_gutter_rows(
    ui: &mut egui::Ui,
    line_count: usize,
    font_id: &egui::FontId,
    text_color: egui::Color32,
) {
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
            text_color.gamma_multiply(0.62),
        );
    }
}

fn displayed_line_count(buffer: &BufferState, view: &EditorViewState) -> usize {
    if buffer.artifact_summary.has_control_chars() && !view.show_control_chars {
        display_line_count(&make_control_chars_clean(buffer.text()))
    } else {
        buffer.line_count
    }
}
