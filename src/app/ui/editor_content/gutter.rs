use crate::app::domain::{BufferState, PublishedViewport, RenderedLayout};
use crate::app::ui::scrolling::{DisplayRow, DisplaySnapshot};
use eframe::egui;

pub struct LineNumberGutterInput<'a> {
    pub buffer: &'a BufferState,
    pub previous_layout: Option<&'a RenderedLayout>,
    pub display_snapshot: Option<&'a DisplaySnapshot>,
    pub published_viewport: Option<&'a PublishedViewport>,
    pub font_id: &'a egui::FontId,
    pub text_color: egui::Color32,
    pub background_color: egui::Color32,
}

pub fn render_line_number_gutter(ui: &mut egui::Ui, input: LineNumberGutterInput<'_>) {
    let line_count = input.buffer.line_count;
    let max_number = max_gutter_line_number(input.previous_layout, line_count);
    let digits = max_number.max(1).to_string().len().max(3);
    let gutter_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                "0".repeat(digits),
                input.font_id.clone(),
                input.text_color.gamma_multiply(0.62),
            )
            .size()
            .x
    }) + 16.0;

    ui.allocate_ui_with_layout(
        egui::vec2(gutter_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, input.background_color);
            ui.set_width(gutter_width);
            let row_height = ui.fonts_mut(|fonts| fonts.row_height(input.font_id));
            let previous_layout = input
                .previous_layout
                .filter(|layout| layout.matches_row_height(row_height));

            if let Some(snapshot) = input
                .display_snapshot
                .filter(|snapshot| snapshot.row_height() == row_height)
            {
                render_gutter_rows(
                    ui,
                    snapshot.content_height().max(ui.available_height()),
                    input.font_id,
                    input.text_color,
                    snapshot_gutter_rows(snapshot, row_height, input.published_viewport),
                );
            } else if let Some(layout) = previous_layout {
                render_gutter_rows(
                    ui,
                    layout.content_height().max(ui.available_height()),
                    input.font_id,
                    input.text_color,
                    layout_gutter_rows(layout, row_height, input.published_viewport),
                );
            } else {
                render_gutter_rows(
                    ui,
                    row_height * line_count.max(1) as f32,
                    input.font_id,
                    input.text_color,
                    fallback_gutter_rows(line_count, row_height),
                );
            }
        },
    );
}

fn snapshot_gutter_rows<'a>(
    snapshot: &'a DisplaySnapshot,
    row_height: f32,
    published_viewport: Option<&'a PublishedViewport>,
) -> impl Iterator<Item = (f32, usize)> + 'a {
    let y_offset = visible_layout_y_offset(published_viewport, row_height);
    let row_range = published_viewport
        .map(|viewport| viewport.row_range.start as u32..viewport.row_range.end as u32)
        .unwrap_or(0..snapshot.row_count());

    row_range.filter_map(move |row_index| {
        let row = DisplayRow(row_index);
        let row_top = snapshot.row_top(row)?;
        let line_number = snapshot.logical_line_for(row)? as usize + 1;
        Some((y_offset + row_top, line_number))
    })
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

fn layout_gutter_rows<'a>(
    layout: &'a RenderedLayout,
    row_height: f32,
    published_viewport: Option<&'a PublishedViewport>,
) -> impl Iterator<Item = (f32, usize)> + 'a {
    let y_offset = visible_layout_y_offset(published_viewport, row_height);
    let row_range = published_viewport
        .map(|viewport| viewport.row_range.clone())
        .unwrap_or(0..layout.row_count());

    row_range.filter_map(move |row_index| {
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

fn visible_layout_y_offset(published_viewport: Option<&PublishedViewport>, row_height: f32) -> f32 {
    published_viewport
        .map(|window| window.layout_row_offset as f32 * row_height)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::{
        layout_gutter_rows, max_gutter_line_number, snapshot_gutter_rows, visible_layout_y_offset,
    };
    use crate::app::domain::{PublishedViewport, RenderedLayout};
    use crate::app::ui::scrolling::DisplaySnapshot;
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
        layout.offset_line_numbers(97);

        assert_eq!(max_gutter_line_number(Some(&layout), 1_000), 1_000);
    }

    #[test]
    fn published_viewports_shift_gutter_rows_down_by_their_row_offset() {
        let viewport = PublishedViewport {
            row_range: 0..3,
            line_range: 40..43,
            layout_row_offset: 40,
        };

        assert_eq!(visible_layout_y_offset(Some(&viewport), 18.0), 720.0);
    }

    /// Lay out four lines forced to wrap at a narrow width so the resulting
    /// galley has more display rows than logical lines. Used to verify that
    /// gutter rows align to the *first* row of each wrapped logical line.
    fn wrapped_test_layout() -> RenderedLayout {
        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            // 40-char lines wrapped at 100px → multiple display rows per line.
            let text = format!("{}\n{}\nshort\nlast", "x".repeat(40), "y".repeat(40),);
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    100.0,
                ))
            });
            layout = Some(RenderedLayout::from_galley(galley));
        });
        layout.expect("layout should be captured")
    }

    fn wrapped_test_snapshot() -> DisplaySnapshot {
        let ctx = egui::Context::default();
        let mut snapshot = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = format!("{}\n{}\nshort\nlast", "x".repeat(40), "y".repeat(40),);
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    100.0,
                ))
            });
            snapshot = Some(DisplaySnapshot::from_galley(galley, 18.0));
        });
        snapshot.expect("snapshot should be captured")
    }

    #[test]
    fn gutter_emits_one_row_per_logical_line_under_wrap() {
        let layout = wrapped_test_layout();
        let row_height = 18.0;
        let rows: Vec<(f32, usize)> = layout_gutter_rows(&layout, row_height, None).collect();

        // Exactly four logical lines should produce four gutter labels.
        assert_eq!(
            rows.len(),
            4,
            "expected one gutter row per logical line; got {rows:?}"
        );

        // Line numbers must be 1..=4 in document order.
        let line_numbers: Vec<usize> = rows.iter().map(|(_, n)| *n).collect();
        assert_eq!(line_numbers, vec![1, 2, 3, 4]);

        // Y positions must be strictly monotonically increasing.
        for pair in rows.windows(2) {
            assert!(
                pair[1].0 > pair[0].0,
                "gutter rows should advance vertically: {pair:?}"
            );
        }
    }

    #[test]
    fn gutter_y_for_wrapped_line_aligns_with_layout_row_top() {
        let layout = wrapped_test_layout();
        let row_height = 18.0;
        let rows: Vec<(f32, usize)> = layout_gutter_rows(&layout, row_height, None).collect();

        // The first gutter row should sit at y = 0 with no published viewport offset.
        assert_eq!(rows[0].0, 0.0);

        // Each subsequent gutter y must equal `row_top` of the row that owns
        // the *first* display row for that logical line.
        for (y, line_no) in &rows {
            let first_row = layout
                .row_line_numbers
                .iter()
                .position(|n| *n == Some(*line_no))
                .expect("line number must appear");
            let expected = layout.row_top(first_row).expect("row_top exists");
            assert_eq!(*y, expected, "line {line_no} y mismatch");
        }
    }

    #[test]
    fn gutter_y_offset_applies_when_published_viewport_starts_partway_down() {
        let mut layout = wrapped_test_layout();
        // Pretend this layout fragment begins 10 logical-row offsets into the
        // overall document (large-file path).
        let viewport = PublishedViewport {
            row_range: 0..layout.row_count(),
            line_range: 100..104,
            layout_row_offset: 10,
        };
        layout.offset_line_numbers(100);

        let row_height = 18.0;
        let rows: Vec<(f32, usize)> =
            layout_gutter_rows(&layout, row_height, Some(&viewport)).collect();

        // Offset = 10 rows × 18 px = 180 px shift on the first line.
        assert_eq!(rows[0].0, 180.0);
        // Line numbers should reflect the offset_line_numbers shift.
        assert_eq!(
            rows.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            vec![101, 102, 103, 104]
        );
    }

    #[test]
    fn snapshot_gutter_rows_use_display_snapshot_as_authority() {
        let snapshot = wrapped_test_snapshot();
        let rows: Vec<(f32, usize)> = snapshot_gutter_rows(&snapshot, 18.0, None).collect();

        assert_eq!(
            rows.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(rows[0].0, 0.0);
    }
}
