use crate::app::domain::{BufferState, RenderedLayout};
use eframe::egui;

pub fn render_line_number_gutter(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    previous_layout: Option<&RenderedLayout>,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    background_color: egui::Color32,
) {
    let line_count = buffer.line_count;
    let max_number = max_gutter_line_number(previous_layout, line_count);
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
            let row_height = ui.fonts_mut(|fonts| fonts.row_height(font_id));
            let previous_layout =
                previous_layout.filter(|layout| layout.matches_row_height(row_height));

            if let Some(layout) = previous_layout {
                render_gutter_rows(
                    ui,
                    layout.content_height().max(ui.available_height()),
                    font_id,
                    text_color,
                    layout_gutter_rows(layout, row_height),
                );
            } else {
                render_gutter_rows(
                    ui,
                    row_height * line_count.max(1) as f32,
                    font_id,
                    text_color,
                    fallback_gutter_rows(line_count, row_height),
                );
            }
        },
    );
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

fn layout_gutter_rows(
    layout: &RenderedLayout,
    row_height: f32,
) -> impl Iterator<Item = (f32, usize)> + '_ {
    let y_offset = visible_layout_y_offset(layout, row_height);

    layout.visible_row_range().filter_map(move |row_index| {
        let row_top = layout.row_top(row_index)?;
        let line_number = layout
            .row_line_numbers
            .get(row_index)
            .and_then(|line_number| *line_number)?;
        Some((y_offset + row_top, line_number))
    })
}

fn fallback_gutter_rows(line_count: usize, row_height: f32) -> impl Iterator<Item = (f32, usize)> {
    let row_count = line_count.max(1);
    (0..row_count).map(move |row_index| (row_height * row_index as f32, row_index + 1))
}

fn max_gutter_line_number(
    previous_layout: Option<&RenderedLayout>,
    fallback_line_count: usize,
) -> usize {
    previous_layout
        .and_then(|layout| {
            layout
                .row_line_numbers
                .iter()
                .rev()
                .flatten()
                .copied()
                .next()
        })
        .unwrap_or(fallback_line_count)
        .max(fallback_line_count)
}

fn visible_layout_y_offset(layout: &RenderedLayout, row_height: f32) -> f32 {
    layout
        .visible_text
        .as_ref()
        .map(|window| window.layout_row_offset as f32 * row_height)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::{max_gutter_line_number, visible_layout_y_offset};
    use crate::app::domain::{RenderedLayout, RenderedTextWindow};
    use eframe::egui;

    fn test_layout(line_count: usize) -> RenderedLayout {
        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = (0..line_count)
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
            layout = Some(RenderedLayout::from_galley(galley));
        });
        layout.expect("layout should be captured")
    }

    #[test]
    fn gutter_width_uses_full_document_line_count() {
        let mut layout = test_layout(10);
        layout.set_visible_text(RenderedTextWindow {
            row_range: 0..3,
            line_range: 97..100,
            char_range: 0..12,
            layout_row_offset: 97,
            text: "line 97\nline 98\nline 99\n".to_owned(),
            truncated_start: true,
            truncated_end: true,
        });
        layout.offset_line_numbers(97);

        assert_eq!(max_gutter_line_number(Some(&layout), 1_000), 1_000);
    }

    #[test]
    fn visible_line_windows_shift_gutter_rows_down_by_their_row_offset() {
        let mut layout = test_layout(3);
        layout.set_visible_text(RenderedTextWindow {
            row_range: 0..3,
            line_range: 40..43,
            char_range: 0..18,
            layout_row_offset: 40,
            text: "line 40\nline 41\nline 42\n".to_owned(),
            truncated_start: true,
            truncated_end: true,
        });

        assert_eq!(visible_layout_y_offset(&layout, 18.0), 720.0);
    }
}
