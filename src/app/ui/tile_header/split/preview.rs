use super::{SplitAxis, SplitPreviewOverlay, split_rect};
use crate::app::domain::RenderedTextWindow;
use crate::app::theme::header_bg;
use eframe::egui;

pub fn paint_split_preview(ui: &egui::Ui, overlay: &SplitPreviewOverlay) {
    let preview_shell_rect = overlay.tile_rect.shrink(1.0);
    ui.painter().rect_stroke(
        preview_shell_rect,
        6.0,
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(120, 180, 255, 90),
        ),
        egui::StrokeKind::Outside,
    );

    if let Some(axis) = overlay.axis {
        paint_split_overlay_details(ui, overlay, axis);
    } else {
        paint_pending_split_hint(ui, preview_shell_rect);
        paint_floating_preview_tile(ui, overlay, false);
    }
}

pub fn build_preview_lines(content: &str) -> Vec<String> {
    let mut lines = content
        .lines()
        .take(4)
        .map(|line| line.replace('\t', "    "))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::from("Untitled"));
    }
    lines
}

pub fn build_preview_lines_for_window(window: &RenderedTextWindow) -> Vec<String> {
    let mut lines = Vec::with_capacity(4);
    if window.truncated_start {
        lines.push(String::from("..."));
    }

    for line in window.text.lines() {
        if lines.len() >= 4 {
            break;
        }
        lines.push(line.replace('\t', "    "));
    }

    if window.truncated_end && lines.len() < 4 {
        lines.push(String::from("..."));
    }

    if lines.is_empty() {
        lines.push(String::from("Untitled"));
    }

    lines.truncate(4);
    lines
}

fn paint_split_overlay_details(ui: &egui::Ui, overlay: &SplitPreviewOverlay, axis: SplitAxis) {
    let preview_rect = overlay.tile_rect.shrink(2.0);
    let (first_rect, second_rect) = split_rect(preview_rect, axis, overlay.ratio);
    let (new_tile_rect, existing_tile_rect) = if overlay.new_view_first {
        (first_rect, second_rect)
    } else {
        (second_rect, first_rect)
    };

    paint_target_split_region(ui, new_tile_rect);
    paint_preview_tile(
        ui,
        existing_tile_rect,
        false,
        "Current tile",
        &overlay.preview_lines,
    );
    paint_floating_preview_tile(ui, overlay, true);
}

fn paint_pending_split_hint(ui: &egui::Ui, tile_rect: egui::Rect) {
    ui.painter().rect_filled(
        tile_rect,
        6.0,
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 18),
    );
    ui.painter().text(
        tile_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::ARROWS_SPLIT,
        egui::FontId::proportional(18.0),
        egui::Color32::from_rgba_unmultiplied(190, 220, 255, 180),
    );
}

fn paint_target_split_region(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_filled(
        rect.shrink(1.0),
        6.0,
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 26),
    );
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        6.0,
        egui::Stroke::new(
            2.0,
            egui::Color32::from_rgba_unmultiplied(120, 180, 255, 160),
        ),
        egui::StrokeKind::Outside,
    );
}

fn calculate_floating_tile_rect(overlay: &SplitPreviewOverlay, max_rect: egui::Rect) -> egui::Rect {
    let anchor = egui::pos2(
        overlay
            .handle_anchor
            .x
            .clamp(max_rect.left() + 32.0, max_rect.right()),
        overlay
            .handle_anchor
            .y
            .clamp(max_rect.top(), max_rect.bottom() - 32.0),
    );
    let pointer = egui::pos2(
        overlay
            .pointer_pos
            .x
            .clamp(max_rect.left(), max_rect.right() - 32.0),
        overlay
            .pointer_pos
            .y
            .clamp(anchor.y + 32.0, max_rect.bottom()),
    );
    egui::Rect::from_min_max(
        egui::pos2(pointer.x.min(anchor.x - 32.0), anchor.y),
        egui::pos2(anchor.x, pointer.y.max(anchor.y + 32.0)),
    )
}

fn paint_floating_preview_tile(ui: &egui::Ui, overlay: &SplitPreviewOverlay, resolved: bool) {
    let max_rect = ui.max_rect().shrink(8.0);
    let rect = calculate_floating_tile_rect(overlay, max_rect);
    paint_preview_tile(
        ui,
        rect,
        true,
        if resolved {
            &overlay.title
        } else {
            "Split preview"
        },
        &overlay.preview_lines,
    );
}

fn paint_preview_tile(
    ui: &egui::Ui,
    rect: egui::Rect,
    is_new_tile: bool,
    title: &str,
    preview_lines: &[String],
) {
    let line_color = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(220, 235, 255, 180)
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 54)
    };

    paint_preview_frame(ui, rect, is_new_tile);
    paint_preview_content(ui, rect, title, preview_lines, line_color);
}

fn paint_preview_frame(ui: &egui::Ui, rect: egui::Rect, is_new_tile: bool) {
    let painter = ui.painter();
    let frame_fill = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(46, 63, 88, 220)
    } else {
        header_bg(ui).gamma_multiply(0.94)
    };
    let border = if is_new_tile {
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 220)
    } else {
        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 70)
    };

    painter.rect_filled(rect, 6.0, frame_fill);
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(if is_new_tile { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Outside,
    );
}

fn paint_preview_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    title: &str,
    preview_lines: &[String],
    line_color: egui::Color32,
) {
    let usable_width = rect.width() - 20.0;
    paint_preview_lines(ui, rect, preview_lines, line_color, usable_width);

    if !title.is_empty() {
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0),
            egui::Align2::LEFT_BOTTOM,
            elide_preview_line(title, usable_width),
            egui::FontId::proportional(11.0),
            line_color.gamma_multiply(0.75),
        );
    }
}

fn paint_preview_lines(
    ui: &egui::Ui,
    rect: egui::Rect,
    preview_lines: &[String],
    line_color: egui::Color32,
    usable_width: f32,
) {
    let top_offset = 12.0;
    for (index, line) in preview_lines.iter().take(4).enumerate() {
        let y = rect.top() + top_offset + index as f32 * 14.0;
        if y > rect.bottom() - 6.0 {
            break;
        }
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, y),
            egui::Align2::LEFT_TOP,
            elide_preview_line(line, usable_width),
            egui::FontId::monospace(11.0),
            line_color,
        );
    }
}

fn elide_preview_line(line: &str, max_width: f32) -> String {
    let max_chars = ((max_width / 7.0).floor() as usize).max(8);
    if line.chars().count() <= max_chars {
        return line.to_owned();
    }

    let trimmed = line
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{trimmed}…")
}

#[cfg(test)]
mod tests {
    use super::build_preview_lines_for_window;
    use crate::app::domain::RenderedTextWindow;

    #[test]
    fn visible_window_previews_include_truncation_markers() {
        let window = RenderedTextWindow {
            row_range: 12..15,
            line_range: 12..15,
            char_range: 120..150,
            layout_row_offset: 0,
            text: "alpha\nbeta\ngamma\n".to_owned(),
            truncated_start: true,
            truncated_end: true,
        };

        assert_eq!(
            build_preview_lines_for_window(&window),
            vec![
                "...".to_owned(),
                "alpha".to_owned(),
                "beta".to_owned(),
                "gamma".to_owned()
            ]
        );
    }

    #[test]
    fn empty_window_previews_fall_back_to_untitled() {
        let window = RenderedTextWindow {
            row_range: 0..0,
            line_range: 0..0,
            char_range: 0..0,
            layout_row_offset: 0,
            text: String::new(),
            truncated_start: false,
            truncated_end: false,
        };

        assert_eq!(
            build_preview_lines_for_window(&window),
            vec!["Untitled".to_owned()]
        );
    }
}
