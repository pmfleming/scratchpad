use super::{SettingsUi, StorageUsage, egui, format_bytes, widget_ids};

pub(super) struct DiskPart {
    pub label: &'static str,
    pub bytes: u64,
    pub color: egui::Color32,
    pub help: &'static str,
}

pub(super) fn parts(ui: &egui::Ui, usage: &StorageUsage) -> [DiskPart; 4] {
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
        DiskPart {
            label: "Other data files",
            bytes: usage.other,
            color: colors[0],
            help: "Other files in Scratchpad's state directory, including diagnostics.",
        },
        DiskPart {
            label: "Session database",
            bytes: usage.database,
            color: colors[1],
            help: "The main database file. Recent committed data can also be in the write-ahead log.",
        },
        DiskPart {
            label: "Write-ahead log & shared memory",
            bytes: usage.journal,
            color: colors[2],
            help: "Database journal and shared-memory files; included in the file total.",
        },
        DiskPart {
            label: "Settings files",
            bytes: usage.settings,
            color: colors[3],
            help: "Files in the settings directory. Overlapping directories are counted only once.",
        },
    ]
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
    use super::ranges;

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
