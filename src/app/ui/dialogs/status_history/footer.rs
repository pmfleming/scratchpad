//! The selected message in full, with copy, measured like Search's footer.
use super::Actions;
use super::model::{self, StatusRow};
use crate::app::{theme, ui::panel_style::window};
use eframe::egui;

const TEXT_SIZE: f32 = 13.0;
const META_SIZE: f32 = 11.0;
const DETAIL_MAX_HEIGHT: f32 = 140.0;
const KEY_HINTS: &str = "↑ ↓ Move   Ctrl+C Copy   Esc Close";

pub(super) fn panel(ui: &mut egui::Ui, row: Option<&StatusRow>, actions: &mut Actions) {
    window::measured_footer(
        ui,
        "status_history.footer",
        0.0,
        "Status footer content changed size",
        |ui| show(ui, row, actions),
    );
}

fn show(ui: &mut egui::Ui, row: Option<&StatusRow>, actions: &mut Actions) {
    ui.separator();
    if let Some(row) = row {
        ui.add(
            egui::Label::new(
                egui::RichText::new(&row.text)
                    .size(TEXT_SIZE)
                    .color(theme::text_primary(ui)),
            )
            .wrap(),
        );
        ui.label(
            egui::RichText::new(format!(
                "{} · {}",
                row.subtitle(),
                model::full_time(row.time)
            ))
            .size(META_SIZE)
            .color(theme::text_muted(ui)),
        );
        if let Some(detail) = &row.detail {
            egui::ScrollArea::vertical()
                .id_salt(("status_history.detail", row.id))
                .max_height(DETAIL_MAX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(detail)
                                .size(12.0)
                                .monospace()
                                .color(theme::text_muted(ui)),
                        )
                        .wrap(),
                    );
                });
        }
    }
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(KEY_HINTS).size(META_SIZE).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            actions.copy |= ui
                .add_enabled(row.is_some(), egui::Button::new("Copy"))
                .on_hover_text("Copy the message and its detail")
                .clicked();
        });
    });
}
