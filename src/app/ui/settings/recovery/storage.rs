use super::super::{SettingsUi, text_muted, text_primary};
use crate::app::{services::session_store::StorageUsage, ui::widget_ids};
use eframe::egui;
use egui_phosphor::regular::{CARET_DOWN, CARET_UP, COPY_SIMPLE, FOLDER_OPEN, WARNING};
use std::path::Path;

mod chart;
#[cfg(test)]
mod tests;

pub(super) fn summary(ui: &mut egui::Ui, usage: &StorageUsage) {
    ui.horizontal(|ui| {
        ui.set_min_height(44.0);
        heading(ui, "Files on disk");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format_bytes(usage.total_file_bytes))
                    .size(23.0)
                    .color(text_primary(ui)),
            )
            .on_hover_text(format!(
                "{} bytes across the data and settings directories",
                usage.total_file_bytes
            ));
        });
    });
    chart::render(ui, usage);
    ui.add_space(4.0);
    if usage.stored_error.is_none() {
        let stored = &usage.stored;
        description(
            ui,
            &format!(
                "{} · {} · {} payloads",
                counted(stored.archives as u64, "closed tab", "closed tabs"),
                counted(stored.recovery_points, "recovery point", "recovery points"),
                format_bytes(payload_total(usage))
            ),
        );
    } else {
        description(ui, "Recovery counts and payload sizes unavailable.");
    }
    ui.add_space(6.0);
}

fn counted(count: u64, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

pub(super) fn details(ui: &mut egui::Ui, usage: &StorageUsage) {
    let allocated = usage
        .allocated_bytes
        .map(|bytes| format!("{} allocated", format_bytes(bytes)));
    disclosure(
        ui,
        "recovery.storage.disk",
        "Disk breakdown",
        allocated.as_deref(),
        |ui| {
            description(ui, "Share of total file size.");
            chart::breakdown(ui, "recovery.disk.bars", &chart::parts(ui, usage));
            if let Some(bytes) = usage.allocated_bytes {
                ui.separator();
                usage_row(
                    ui,
                    "Allocated on disk",
                    bytes,
                    "Actual filesystem blocks occupied by the same files. Not an additional category.",
                );
            }
            description(ui, "File sizes, not a capacity or retention budget.");
            ui.add_space(6.0);
        },
    );
    if let Some(error) = &usage.stored_error {
        ui.separator();
        notice(ui, WARNING, "Recovery details unavailable", error);
    } else {
        disclosure(
            ui,
            "recovery.storage.details",
            "Recovery details",
            None,
            |ui| {
                description(ui, "Share of total recovery payloads.");
                let color = if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(111, 193, 173)
                } else {
                    egui::Color32::from_rgb(41, 133, 111)
                };
                let parts = payload_rows(usage).map(|(label, bytes, help)| chart::Part {
                    label,
                    bytes,
                    help,
                    color,
                });
                chart::breakdown(ui, "recovery.payload.bars", &parts);
                ui.separator();
                usage_row(
                    ui,
                    "Recovery payload total",
                    payload_total(usage),
                    "Sum of the payload categories above. Not extra disk usage or a measure of retention-budget utilisation.",
                );
                ui.separator();
                usage_row(
                    ui,
                    "Reusable database space",
                    usage.stored.reusable_pages,
                    "Free pages already inside the database file, available for reuse. Not additional disk usage.",
                );
                usage_row(
                    ui,
                    "Archive references",
                    usage.stored.archive_referenced_bytes,
                    "Overlaps the payloads above and may count shared references more than once. Not added to the total.",
                );
                ui.add_space(6.0);
            },
        );
    }
}

fn payload_rows(usage: &StorageUsage) -> [(&'static str, u64, &'static str); 8] {
    let stored = &usage.stored;
    [
        (
            "Current & archived text",
            stored.live_text,
            "Text referenced only by current or archived buffers, not historical snapshots.",
        ),
        (
            "Historical snapshot text",
            stored.recovery_text,
            "Text referenced only by historical snapshots.",
        ),
        (
            "Shared text",
            stored.shared_text,
            "Text shared between live or archived buffers and historical snapshots; counted once.",
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

/// Full-width disclosures keep the same left edge as policy labels and do not
/// introduce another nested card or shift the expanded accounting rows inward.
fn disclosure(
    ui: &mut egui::Ui,
    key: &str,
    label: &str,
    value: Option<&str>,
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.separator();
    let id = widget_ids::local(ui, key);
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let response =
        widget_ids::surface_response(ui, key, widget_ids::WidgetRole::SettingsCardHeader, |ui| {
            ui.add(
                egui::Button::new("")
                    .frame(false)
                    .min_size(egui::vec2(ui.available_width(), 34.0)),
            )
        });
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::DisclosureTriangle, ui.is_enabled(), label)
    });
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_expanded(state.is_open()));
    let font = egui::FontId::proportional(SettingsUi::TYPOGRAPHY.description);
    ui.painter().text(
        response.rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        font.clone(),
        text_muted(ui),
    );
    if let Some(value) = value {
        ui.painter().text(
            response.rect.right_center() - egui::vec2(28.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            value,
            font,
            text_muted(ui),
        );
        response.clone().on_hover_text(value);
    }
    ui.painter().text(
        response.rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::CENTER_CENTER,
        if state.is_open() {
            CARET_UP
        } else {
            CARET_DOWN
        },
        egui_phosphor::font_id(14.0),
        SettingsUi::icon_color(ui),
    );
    if response.clicked() {
        state.toggle(ui);
    }
    state.show_body_unindented(ui, body);
}

fn heading(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(SettingsUi::TYPOGRAPHY.body)
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
            .truncate()
            .halign(egui::Align::LEFT),
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
