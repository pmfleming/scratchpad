use crate::app::domain::BufferState;
use crate::app::ui::scrolling::{DisplayRow, DisplaySnapshot};
use eframe::egui;

pub fn render_line_number_gutter(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    previous_snapshot: Option<&DisplaySnapshot>,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    background_color: egui::Color32,
) {
    let line_count = buffer.line_count;
    let gutter_width = gutter_width(ui, font_id, text_color, previous_snapshot, line_count);

    ui.allocate_ui_with_layout(
        egui::vec2(gutter_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, background_color);
            ui.set_width(gutter_width);
            let row_height = ui.fonts_mut(|fonts| fonts.row_height(font_id));
            render_gutter_body(
                ui,
                previous_snapshot,
                line_count,
                row_height,
                font_id,
                text_color,
            );
        },
    );
}

fn gutter_width(
    ui: &mut egui::Ui,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    previous_snapshot: Option<&DisplaySnapshot>,
    line_count: usize,
) -> f32 {
    let max_number = max_gutter_line_number(previous_snapshot, line_count);
    let digits = max_number.max(1).to_string().len().max(3);
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                "0".repeat(digits),
                font_id.clone(),
                text_color.gamma_multiply(0.62),
            )
            .size()
            .x
    }) + 16.0
}

fn render_gutter_body(
    ui: &mut egui::Ui,
    previous_snapshot: Option<&DisplaySnapshot>,
    line_count: usize,
    row_height: f32,
    font_id: &egui::FontId,
    text_color: egui::Color32,
) {
    if let Some(snapshot) = matching_row_height_snapshot(previous_snapshot, row_height) {
        render_gutter_rows(
            ui,
            snapshot.content_height().max(ui.available_height()),
            font_id,
            text_color,
            snapshot_gutter_rows(snapshot),
        );
        return;
    }

    render_gutter_rows(
        ui,
        row_height * line_count.max(1) as f32,
        font_id,
        text_color,
        fallback_gutter_rows(line_count, row_height),
    );
}

fn matching_row_height_snapshot(
    snapshot: Option<&DisplaySnapshot>,
    row_height: f32,
) -> Option<&DisplaySnapshot> {
    snapshot.filter(|snap| (snap.row_height() - row_height).abs() < 0.01)
}

fn render_gutter_rows(
    ui: &mut egui::Ui,
    desired_height: f32,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    rows: impl Iterator<Item = (f32, usize)>,
) {
    let desired_size = egui::vec2(ui.available_width(), desired_height);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter();

    for (row_top, line_number) in rows {
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.top() + row_top),
            egui::Align2::RIGHT_TOP,
            line_number.to_string(),
            font_id.clone(),
            text_color.gamma_multiply(0.62),
        );
    }
}

fn snapshot_gutter_rows(snapshot: &DisplaySnapshot) -> impl Iterator<Item = (f32, usize)> + '_ {
    let row_count = snapshot.row_count();
    let mut prev_logical: Option<u32> = None;
    (0..row_count).filter_map(move |i| {
        let row = DisplayRow(i);
        let row_top = snapshot.row_top(row)?;
        let logical = snapshot.logical_line_for(row)?;
        let is_leading = prev_logical != Some(logical);
        prev_logical = Some(logical);
        is_leading.then_some((row_top, logical as usize + 1))
    })
}

fn fallback_gutter_rows(line_count: usize, row_height: f32) -> impl Iterator<Item = (f32, usize)> {
    let row_count = line_count.max(1);
    (0..row_count).map(move |row_index| (row_height * row_index as f32, row_index + 1))
}

fn max_gutter_line_number(
    previous_snapshot: Option<&DisplaySnapshot>,
    fallback_line_count: usize,
) -> usize {
    previous_snapshot
        .and_then(|snap| {
            let count = snap.row_count();
            if count == 0 {
                return None;
            }
            snap.logical_line_for(DisplayRow(count - 1))
                .map(|n| n as usize + 1)
        })
        .unwrap_or(fallback_line_count)
        .max(fallback_line_count)
}
