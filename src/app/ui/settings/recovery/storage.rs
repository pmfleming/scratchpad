use super::super::{SettingsUi, text_muted, text_primary};
use crate::app::services::session_store::StorageUsage;
use eframe::egui;
use egui_phosphor::regular::{
    ARCHIVE, CARET_DOWN, CARET_RIGHT, CLOCK_COUNTER_CLOCKWISE, COPY_SIMPLE, DATABASE, FOLDER_OPEN,
    INFO, WARNING,
};
use std::path::Path;

#[cfg(test)]
mod tests;

pub(super) fn render(ui: &mut egui::Ui, usage: &StorageUsage) {
    ui.spacing_mut().item_spacing.y = 8.0;
    let available = usage.stored_error.is_none();
    ui.columns(3, |columns| {
        metric(
            &mut columns[0],
            DATABASE,
            "Data files",
            &format_bytes(usage.total_file_bytes),
            &format!(
                "{} bytes across the data and settings directories",
                usage.total_file_bytes
            ),
        );
        metric(
            &mut columns[1],
            ARCHIVE,
            "Closed archives",
            &count(available, usage.stored.archives as u64),
            "Saved tabs available in Recently Closed",
        );
        metric(
            &mut columns[2],
            CLOCK_COUNTER_CLOCKWISE,
            "Recovery points",
            &count(available, usage.stored.recovery_points),
            "Stored text snapshots; not additional copies of all undo history",
        );
    });
    ui.add_space(8.0);
    heading(ui, "Files on disk");
    for (label, bytes, help) in [
        (
            "Session database",
            usage.database,
            "The main database file. Recent committed data can also be in the write-ahead log.",
        ),
        (
            "Write-ahead log & shared memory",
            usage.journal,
            "Database journal and shared-memory files; included in the data-files total.",
        ),
        (
            "Other data files",
            usage.other,
            "Other files in Scratchpad's state directory, including diagnostics.",
        ),
        (
            "Settings files",
            usage.settings,
            "Files in the separate settings directory. Overlapping directories are counted only once.",
        ),
    ] {
        usage_row(ui, label, bytes, help);
    }
    if let Some(bytes) = usage.allocated_bytes {
        ui.separator();
        usage_row(
            ui,
            "Allocated on disk",
            bytes,
            "Actual filesystem blocks occupied by all measured files, rather than their file lengths. Not an additional category.",
        );
    }
    ui.add_space(4.0);
    if let Some(error) = &usage.stored_error {
        notice(ui, WARNING, "Recovery details unavailable", error);
    } else {
        details(ui, usage);
    }
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        icon(ui, INFO, SettingsUi::icon_color(ui));
        description(ui, "Refreshes every 5 seconds. Your original documents and the application itself are not included.");
    });
}

fn count(available: bool, value: u64) -> String {
    if available {
        value.to_string()
    } else {
        "—".into()
    }
}

fn metric(ui: &mut egui::Ui, glyph: &str, label: &str, value: &str, help: &str) {
    let width = (ui.available_width() - 22.0).max(0.0);
    egui::Frame::new()
        .fill(SettingsUi::control_bg(ui))
        .stroke(egui::Stroke::new(1.0, SettingsUi::card_border(ui)))
        .corner_radius(egui::CornerRadius::same(SettingsUi::LAYOUT.card_radius))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                icon(ui, glyph, SettingsUi::icon_color(ui));
                ui.label(
                    egui::RichText::new(label)
                        .size(SettingsUi::TYPOGRAPHY.description)
                        .color(text_muted(ui)),
                );
            });
            ui.label(
                egui::RichText::new(value)
                    .size(24.0)
                    .strong()
                    .color(text_primary(ui)),
            )
            .on_hover_text(help);
        });
}

fn details(ui: &mut egui::Ui, usage: &StorageUsage) {
    egui::CollapsingHeader::new(egui::RichText::new("Stored recovery data")
        .size(SettingsUi::TYPOGRAPHY.body).color(text_primary(ui)))
        .id_salt("recovery.storage.details")
        .icon(|ui, openness, response| {
            ui.painter().text(response.rect.center(), egui::Align2::CENTER_CENTER,
                if openness > 0.5 { CARET_DOWN } else { CARET_RIGHT },
                egui_phosphor::font_id(14.0), SettingsUi::icon_color(ui));
        })
        .show(ui, |ui| {
            description(ui, "Payload sizes, not additional disk usage. Shared text is counted once; indexes, page overhead and the journal are only in the file totals above.");
            ui.add_space(4.0);
            for (label, bytes, help) in [
                ("Session metadata", usage.stored.session_metadata, "Open tabs, views and session state."),
                ("Undo history", usage.stored.undo, "Detailed undo data for open tabs, archives and saved return branches."),
                ("Archive metadata", usage.stored.archive_metadata, "Metadata for retained closed tabs."),
                ("Recovery point metadata", usage.stored.snapshot_metadata, "Estimated metadata size for stored recovery points."),
                ("Current & archived text", usage.stored.live_text, "Text referenced only by current or archived buffers, not historical snapshots."),
                ("Historical snapshot text", usage.stored.recovery_text, "Text referenced only by historical snapshots."),
                ("Shared text", usage.stored.shared_text, "Text shared between live or archived buffers and historical snapshots; counted once."),
                ("Awaiting cleanup", usage.stored.unreferenced_text, "Text with no remaining live or historical references."),
            ] {
                usage_row(ui, label, bytes, help);
            }
            ui.separator();
            usage_row(ui, "Reusable database space", usage.stored.reusable_pages,
                "Free pages already inside the database file, available for reuse. Not additional disk usage.");
            usage_row(ui, "Archive references", usage.stored.archive_referenced_bytes,
                "Payloads referenced by closed archives. Overlaps the breakdown above and can count shared references more than once; do not add to the totals.");
        });
}

fn heading(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(SettingsUi::TYPOGRAPHY.body)
            .strong()
            .color(text_primary(ui)),
    );
}

fn usage_row(ui: &mut egui::Ui, label: &str, bytes: u64, help: &str) {
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        ui.label(
            egui::RichText::new(label)
                .size(SettingsUi::TYPOGRAPHY.description)
                .color(text_muted(ui)),
        )
        .on_hover_text(help);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format_bytes(bytes))
                    .size(SettingsUi::TYPOGRAPHY.body)
                    .color(text_primary(ui)),
            )
            .on_hover_text(format!("{bytes} bytes\n{help}"));
        });
    });
}

pub(super) fn location(ui: &mut egui::Ui, path: &Path) {
    ui.add_space(6.0);
    ui.separator();
    let path = path.display().to_string();
    ui.horizontal(|ui| {
        icon(ui, FOLDER_OPEN, SettingsUi::icon_color(ui));
        let width = (ui.available_width() - 40.0).max(0.0);
        ui.add_sized(
            egui::vec2(width, 24.0),
            egui::Label::new(
                egui::RichText::new(&path)
                    .size(SettingsUi::TYPOGRAPHY.description)
                    .color(text_muted(ui)),
            )
            .truncate(),
        )
        .on_hover_text(format!("Data directory\n{path}"));
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(COPY_SIMPLE)
                        .font(egui_phosphor::font_id(16.0))
                        .color(text_primary(ui)),
                )
                .fill(SettingsUi::control_bg(ui))
                .min_size(egui::vec2(28.0, 26.0)),
            )
            .accessible_name("Copy data directory")
            .on_hover_text("Copy data directory")
            .clicked()
        {
            ui.ctx().copy_text(path);
        }
    });
}

pub(super) fn notice(ui: &mut egui::Ui, glyph: &str, title: &str, detail: &str) {
    ui.horizontal_top(|ui| {
        icon(ui, glyph, SettingsUi::icon_color(ui));
        ui.vertical(|ui| {
            heading(ui, title);
            description(ui, detail);
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

fn icon(ui: &mut egui::Ui, glyph: &str, color: egui::Color32) {
    ui.label(
        egui::RichText::new(glyph)
            .font(egui_phosphor::font_id(16.0))
            .color(color),
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
