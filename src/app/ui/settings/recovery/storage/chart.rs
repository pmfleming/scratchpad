use super::{SettingsUi, StorageUsage, egui, format_bytes, widget_ids};

pub(super) struct Part {
    pub label: &'static str,
    pub bytes: u64,
    pub color: egui::Color32,
    pub help: &'static str,
}

pub(super) fn parts(ui: &egui::Ui, usage: &StorageUsage) -> [Part; 4] {
    let colors = if ui.visuals().dark_mode {
        [
            [134, 152, 180],
            [72, 182, 247],
            [182, 161, 223],
            [225, 185, 117],
        ]
    } else {
        [
            [121, 141, 169],
            [24, 136, 201],
            [149, 130, 198],
            [182, 138, 57],
        ]
    }
    .map(|[r, g, b]| egui::Color32::from_rgb(r, g, b));
    [
        Part {
            label: "Other data files",
            bytes: usage.other,
            color: colors[0],
            help: "Other files in Scratchpad's state directory, including diagnostics.",
        },
        Part {
            label: "Session database",
            bytes: usage.database,
            color: colors[1],
            help: "The main database file. Recent committed data can also be in the write-ahead log.",
        },
        Part {
            label: "Write-ahead log & shared memory",
            bytes: usage.journal,
            color: colors[2],
            help: "Database journal and shared-memory files; included in the file total.",
        },
        Part {
            label: "Settings files",
            bytes: usage.settings,
            color: colors[3],
            help: "Files in the settings directory. Overlapping directories are counted only once.",
        },
    ]
}

pub(super) fn payload_colors(dark: bool) -> [egui::Color32; 8] {
    let colors = if dark {
        [
            [111, 193, 173],
            [72, 182, 247],
            [182, 161, 223],
            [225, 185, 117],
            [232, 135, 145],
            [151, 188, 106],
            [221, 153, 212],
            [134, 152, 180],
        ]
    } else {
        [
            [41, 133, 111],
            [24, 136, 201],
            [149, 130, 198],
            [182, 138, 57],
            [186, 78, 92],
            [98, 137, 53],
            [172, 93, 162],
            [101, 121, 149],
        ]
    };
    colors.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
}

/// Composition of physical files only. Do not mix in logical payloads,
/// allocated blocks, reusable pages or overlapping archive references.
pub(super) fn render(ui: &mut egui::Ui, usage: &StorageUsage) {
    let parts = parts(ui, usage);
    let response = widget_ids::surface_response(
        ui,
        "settings.recovery.disk_composition",
        widget_ids::WidgetRole::Label,
        |ui| ui.allocate_response(egui::vec2(ui.available_width(), 7.0), egui::Sense::hover()),
    );
    let rect = response.rect;
    ui.painter()
        .rect_filled(rect, 2.0, SettingsUi::control_bg(ui));
    for (part, (start, end)) in parts
        .iter()
        .zip(ranges(parts.each_ref().map(|part| part.bytes)))
    {
        if end > start {
            let segment = egui::Rect::from_min_max(
                egui::pos2(rect.left() + start * rect.width(), rect.top()),
                egui::pos2(rect.left() + end * rect.width(), rect.bottom()),
            );
            ui.painter().rect_filled(segment, 0.0, part.color);
        }
    }
    let label = format!(
        "Disk composition: {}",
        parts
            .iter()
            .map(|part| format!("{}: {}", part.label, format_bytes(part.bytes)))
            .collect::<Vec<_>>()
            .join("; ")
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    response.on_hover_text(label);
}

/// Pie slices use true byte proportions. The legend retains zero and tiny
/// categories even when their slices are not visible.
pub(super) fn breakdown(ui: &mut egui::Ui, key: &str, parts: &[Part]) {
    ui.add_space(12.0);
    if ui.available_width() >= 600.0 {
        ui.horizontal_top(|ui| {
            pie(ui, key, parts, 156.0);
            ui.add_space(16.0);
            ui.vertical(|ui| legend(ui, parts));
        });
    } else {
        let diameter = ui.available_width().min(156.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - diameter) / 2.0);
            pie(ui, key, parts, diameter);
        });
        ui.add_space(12.0);
        legend(ui, parts);
    }
    ui.add_space(12.0);
}

fn part_label(part: &Part, total: f64) -> String {
    format!(
        "{}: {} ({:.2}% of breakdown total)",
        part.label,
        format_bytes(part.bytes),
        fraction(part.bytes, total) * 100.0
    )
}

fn legend(ui: &mut egui::Ui, parts: &[Part]) {
    let total = parts.iter().map(|part| part.bytes as f64).sum();
    for part in parts {
        super::row::usage_row_with_marker(
            ui,
            part.label,
            part.bytes,
            &format!("{}\n{}", part_label(part, total), part.help),
            Some(part.color),
        );
    }
}

fn pie(ui: &mut egui::Ui, key: &str, parts: &[Part], diameter: f32) {
    let response = widget_ids::surface_response(ui, key, widget_ids::WidgetRole::Label, |ui| {
        ui.allocate_response(egui::Vec2::splat(diameter), egui::Sense::hover())
    });
    let center = response.rect.center();
    let radius = diameter / 2.0;
    let total: f64 = parts.iter().map(|part| part.bytes as f64).sum();
    let label = format!(
        "Pie chart: {}",
        parts
            .iter()
            .map(|part| part_label(part, total))
            .collect::<Vec<_>>()
            .join("; ")
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    if total == 0.0 {
        ui.painter()
            .circle_filled(center, radius, SettingsUi::control_bg(ui));
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "No data",
            SettingsUi::font(ui, SettingsUi::TYPOGRAPHY.description),
            super::text_muted(ui),
        );
        response.on_hover_text(label);
        return;
    }

    let hovered_angle = response.hover_pos().and_then(|pos| {
        let delta = pos - center;
        (delta.length() <= radius).then(|| {
            (delta.y.atan2(delta.x) + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU)
                / std::f32::consts::TAU
        })
    });
    let mut used = 0.0;
    let mut hover_text = None;
    for part in parts {
        let start = (used / total) as f32;
        used += part.bytes as f64;
        let end = (used / total) as f32;
        if end <= start {
            continue;
        }
        ui.painter().add(egui::Shape::mesh(slice_mesh(
            center, radius, start, end, part.color,
        )));
        if hovered_angle.is_some_and(|angle| angle >= start && angle < end) {
            hover_text = Some(format!(
                "{}\n{} bytes\n{}",
                part_label(part, total),
                part.bytes,
                part.help
            ));
        }
    }
    response.on_hover_text(hover_text.unwrap_or(label));
}

fn slice_mesh(
    center: egui::Pos2,
    radius: f32,
    start: f32,
    end: f32,
    color: egui::Color32,
) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let steps = ((end - start) * 128.0).ceil().max(1.0) as u32;
    mesh.colored_vertex(center, color);
    for step in 0..=steps {
        let share = start + (end - start) * step as f32 / steps as f32;
        let angle = share * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        mesh.colored_vertex(
            center + radius * egui::vec2(angle.cos(), angle.sin()),
            color,
        );
        if step > 0 {
            mesh.add_triangle(0, step, step + 1);
        }
    }
    mesh
}

fn fraction(bytes: u64, total: f64) -> f32 {
    if total == 0.0 {
        0.0
    } else {
        (bytes as f64 / total).clamp(0.0, 1.0) as f32
    }
}

fn ranges(bytes: [u64; 4]) -> [(f32, f32); 4] {
    // Convert before summing to avoid overflowing on very large directories.
    let total: f64 = bytes.iter().map(|bytes| *bytes as f64).sum();
    let mut used = 0.0;
    bytes.map(|bytes| {
        let start = used;
        used += bytes as f64;
        if total == 0.0 {
            (0.0, 0.0)
        } else {
            ((start / total) as f32, (used / total) as f32)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{egui, fraction, payload_colors, ranges, slice_mesh};

    #[test]
    fn pie_mesh_handles_tiny_majority_and_full_circle_slices() {
        for (start, end) in [(0.0, 0.00001), (0.0, 0.8), (0.0, 1.0), (0.8, 1.0)] {
            let center = egui::pos2(80.0, 80.0);
            let mesh = slice_mesh(center, 78.0, start, end, egui::Color32::WHITE);
            assert!(mesh.is_valid());
            assert!(!mesh.indices.is_empty());
            assert_eq!(mesh.vertices[0].pos, center);
            for vertex in &mesh.vertices[1..] {
                assert!(vertex.pos.is_finite());
                assert!(((vertex.pos - center).length() - 78.0).abs() < 0.001);
            }
        }
        for dark in [true, false] {
            let colors = payload_colors(dark);
            for (index, color) in colors.iter().enumerate() {
                assert!(!colors[..index].contains(color));
            }
        }
    }

    #[test]
    fn pie_slices_use_linear_shares_without_inflating_small_or_zero_values() {
        assert_eq!(fraction(0, 0.0), 0.0);
        assert_eq!(fraction(0, 100.0), 0.0);
        assert_eq!(fraction(25, 100.0), 0.25);
        assert_eq!(fraction(1, 100_000.0), 0.00001);
        assert_eq!(fraction(u64::MAX, 4.0 * u64::MAX as f64), 0.25);
    }

    #[test]
    fn graph_preserves_proportions_and_handles_empty_or_large_directories() {
        assert_eq!(ranges([0; 4]), [(0.0, 0.0); 4]);
        assert_eq!(
            ranges([8, 0, 1, 1]),
            [(0.0, 0.8), (0.8, 0.8), (0.8, 0.9), (0.9, 1.0)]
        );
        assert_eq!(
            ranges([u64::MAX; 4]),
            [(0.0, 0.25), (0.25, 0.5), (0.5, 0.75), (0.75, 1.0)]
        );
    }
}
