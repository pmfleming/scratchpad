use super::super::{SettingsUi, text_muted, text_primary};
use crate::app::{services::session_store::StorageUsage, ui::widget_ids};
use eframe::egui;

mod chart;
#[cfg(test)]
mod tests;

pub(super) fn summary(ui: &mut egui::Ui, usage: &StorageUsage) {
    usage_row(
        ui,
        "Files on disk",
        usage.total_file_bytes,
        "Actual file sizes, not a retention budget.",
    );
    chart::render(ui, usage);
    if usage.stored_error.is_none() {
        description(
            ui,
            &format!(
                "{} · {}",
                counted(usage.stored.archives as u64, "closed tab", "closed tabs"),
                counted(
                    usage.stored.recovery_points,
                    "recovery point",
                    "recovery points"
                )
            ),
        );
    } else {
        description(ui, "Recovery counts and payload sizes unavailable.");
    }
}

fn counted(count: u64, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

/// Measurements are always visible inside History. Physical files and logical
/// payloads have separate totals; overlapping references never become slices.
pub(super) fn details(ui: &mut egui::Ui, usage: &StorageUsage) {
    super::group_heading(ui, "Disk breakdown");
    chart::breakdown(ui, "recovery.disk.pie", &chart::parts(ui, usage));
    if let Some(bytes) = usage.allocated_bytes {
        usage_row(
            ui,
            "Allocated on disk",
            bytes,
            "Filesystem blocks occupied by the same files, not an additional category.",
        );
    }
    if let Some(error) = &usage.stored_error {
        notice(
            ui,
            egui_phosphor::regular::WARNING,
            "Recovery details unavailable",
            error,
        );
        return;
    }
    super::group_heading(ui, "Recovery payloads");
    let colors = chart::payload_colors(ui.visuals().dark_mode);
    let rows = payload_rows(usage);
    let parts: [_; 8] = std::array::from_fn(|index| {
        let (label, bytes, help) = rows[index];
        chart::Part {
            label,
            bytes,
            help,
            color: colors[index],
        }
    });
    chart::breakdown(ui, "recovery.payload.pie", &parts);
    usage_row(
        ui,
        "Recovery payload total",
        payload_total(usage),
        "Included in the database and journal, not extra disk usage.",
    );
    usage_row(
        ui,
        "Reusable database space",
        usage.stored.reusable_pages,
        "Free pages already inside the database. Excluded from the pie.",
    );
    usage_row(
        ui,
        "Archive references",
        usage.stored.archive_referenced_bytes,
        "Overlapping references, not additional usage. Excluded from the pie.",
    );
}

fn payload_rows(usage: &StorageUsage) -> [(&'static str, u64, &'static str); 8] {
    let stored = &usage.stored;
    [
        (
            "Current & archived text",
            stored.live_text,
            "Text referenced only by current or archived buffers.",
        ),
        (
            "Historical snapshot text",
            stored.recovery_text,
            "Text referenced only by historical snapshots.",
        ),
        (
            "Shared text",
            stored.shared_text,
            "Text shared by current or archived buffers and historical snapshots; counted once.",
        ),
        (
            "Undo history",
            stored.undo,
            "Detailed undo data for open tabs, archives and saved return branches.",
        ),
        (
            "Session metadata",
            stored.session_metadata,
            "Open tabs, views and session state.",
        ),
        (
            "Archive metadata",
            stored.archive_metadata,
            "Metadata for retained closed tabs.",
        ),
        (
            "Recovery point metadata",
            stored.snapshot_metadata,
            "Estimated metadata size for stored recovery points.",
        ),
        (
            "Awaiting cleanup",
            stored.unreferenced_text,
            "Text with no remaining live or historical references.",
        ),
    ]
}

fn payload_total(usage: &StorageUsage) -> u64 {
    payload_rows(usage)
        .iter()
        .fold(0u64, |total, (_, bytes, _)| total.saturating_add(*bytes))
}

fn usage_row(ui: &mut egui::Ui, label: &str, bytes: u64, help: &str) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        let value_width = 104.0_f32.min(width * 0.4);
        ui.add_sized(
            egui::vec2(
                (width - value_width - ui.spacing().item_spacing.x).max(0.0),
                0.0,
            ),
            egui::Label::new(egui::RichText::new(label).size(12.5).color(text_muted(ui)))
                .wrap()
                .halign(egui::Align::LEFT),
        )
        .on_hover_text(help);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format_bytes(bytes))
                    .size(13.0)
                    .color(text_primary(ui)),
            )
            .on_hover_text(format!("{bytes} bytes\n{help}"));
        });
    });
}

pub(super) fn notice(ui: &mut egui::Ui, glyph: &str, title: &str, detail: &str) {
    ui.horizontal_top(|ui| {
        ui.label(
            egui::RichText::new(glyph)
                .font(egui_phosphor::font_id(16.0))
                .color(SettingsUi::icon_color(ui)),
        );
        ui.vertical(|ui| {
            ui.label(title);
            if !detail.is_empty() {
                description(ui, detail);
            }
        });
    });
}

fn description(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(SettingsUi::TYPOGRAPHY.description)
            .color(text_muted(ui)),
    );
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
        value /= 1024.0;
        unit = next;
        if value < 1024.0 {
            break;
        }
    }
    format!("{value:.2} {unit}")
}
