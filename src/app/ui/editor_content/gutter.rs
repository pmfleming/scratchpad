use crate::app::capacity_metrics::{FramePhase, record_frame_phase};
use crate::app::domain::BufferState;
use crate::app::ui::scrolling::{DisplayRow, DisplaySnapshot};
use crate::app::ui::widget_ids::{self, WidgetRole};
use eframe::egui;
use std::time::Instant;

pub fn render_line_number_gutter(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    viewport: Option<egui::Rect>,
    previous_snapshot: Option<&DisplaySnapshot>,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    background_color: egui::Color32,
) {
    let started_at = Instant::now();
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
                viewport,
                row_height,
                font_id,
                text_color,
            );
        },
    );
    record_frame_phase(FramePhase::Gutter, started_at.elapsed());
}

fn gutter_width(
    ui: &mut egui::Ui,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    previous_snapshot: Option<&DisplaySnapshot>,
    line_count: usize,
) -> f32 {
    let max_number = max_gutter_line_number(previous_snapshot, line_count);
    let digits = gutter_digit_count(max_number);
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

fn gutter_digit_count(max_number: usize) -> usize {
    max_number.max(1).to_string().len()
}

fn render_gutter_body(
    ui: &mut egui::Ui,
    previous_snapshot: Option<&DisplaySnapshot>,
    line_count: usize,
    viewport: Option<egui::Rect>,
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
        fallback_gutter_rows(line_count, row_height, viewport),
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
    let response = widget_ids::allocate_exact_rect_interact(
        ui,
        desired_size,
        ("editor_gutter", WidgetRole::TextEdit),
        egui::Sense::hover(),
        "editor_gutter",
    );
    let rect = response.rect;
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

fn fallback_gutter_rows(
    line_count: usize,
    row_height: f32,
    viewport: Option<egui::Rect>,
) -> impl Iterator<Item = (f32, usize)> {
    let row_count = line_count.max(1);
    let range = fallback_gutter_row_range(row_count, row_height, viewport);
    range.map(move |row_index| (row_height * row_index as f32, row_index + 1))
}

fn fallback_gutter_row_range(
    row_count: usize,
    row_height: f32,
    viewport: Option<egui::Rect>,
) -> std::ops::Range<usize> {
    if row_height <= 0.0 {
        return 0..row_count.min(1);
    }

    let Some(viewport) = viewport else {
        return 0..row_count;
    };

    let overscan_rows = 2usize;
    let first = (viewport.min.y / row_height).floor().max(0.0) as usize;
    let last = (viewport.max.y / row_height).ceil().max(1.0) as usize;
    first.saturating_sub(overscan_rows)..(last + overscan_rows).min(row_count)
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

#[cfg(test)]
mod tests {
    use super::{
        fallback_gutter_row_range, gutter_digit_count, max_gutter_line_number,
        render_line_number_gutter, snapshot_gutter_rows,
    };
    use crate::app::domain::BufferState;
    use crate::app::ui::scrolling::DisplaySnapshot;
    use eframe::egui;

    #[test]
    fn wrapped_lines_are_numbered_once_on_their_first_display_row() {
        let ctx = egui::Context::default();
        let font = egui::FontId::monospace(14.0);
        let text = "one\ntwo two two\nthree";
        let buffer = BufferState::new("gutter".into(), text.into(), None);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let row_height = ui.fonts_mut(|fonts| fonts.row_height(&font));
            let galley = ui.fonts_mut(|fonts| {
                fonts.layout(text.into(), font.clone(), egui::Color32::WHITE, 40.0)
            });
            let snapshot = DisplaySnapshot::from_galley(&galley, row_height, 0, 0);
            assert!(snapshot.row_count() > 3, "the middle line should wrap");
            let numbers = snapshot_gutter_rows(&snapshot)
                .map(|(_, number)| number)
                .collect::<Vec<_>>();
            assert_eq!(numbers, [1, 2, 3]);
            assert_eq!(max_gutter_line_number(Some(&snapshot), 2), 3);
            assert_eq!(max_gutter_line_number(None, 2), 2);

            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0));
            for (viewport, snapshot) in [(None, Some(&snapshot)), (Some(viewport), None)] {
                render_line_number_gutter(
                    ui,
                    &buffer,
                    viewport,
                    snapshot,
                    &font,
                    egui::Color32::WHITE,
                    egui::Color32::BLACK,
                );
            }
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn fallback_gutter_rows_cover_the_viewport_or_the_whole_buffer() {
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 200.0), egui::pos2(400.0, 260.0));
        assert_eq!(
            fallback_gutter_row_range(10_000, 20.0, Some(viewport)),
            8..15
        );
        assert_eq!(fallback_gutter_row_range(12, 20.0, None), 0..12);
        assert_eq!(fallback_gutter_row_range(12, 0.0, Some(viewport)), 0..1);
    }

    #[test]
    fn gutter_digits_follow_line_count_without_fixed_three_digit_floor() {
        assert_eq!(gutter_digit_count(0), 1);
        assert_eq!(gutter_digit_count(9), 1);
        assert_eq!(gutter_digit_count(10), 2);
        assert_eq!(gutter_digit_count(999), 3);
    }
}
