use super::common::show_centered_callout;
use crate::app::app_state::{ScratchpadApp, StartupRestoreConflict};
use crate::app::ui::callout;
use eframe::egui;
use egui_phosphor::regular::{COPY, FILE_TEXT, WARNING, X};

const RESTORE_CONFLICT_DIALOG_SIZE: egui::Vec2 = egui::vec2(452.0, 220.0);

pub(crate) fn show_startup_restore_conflict_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(conflict) = app.current_startup_restore_conflict().cloned() else {
        return;
    };

    let mut keep_session = false;
    let mut open_compare = false;
    let total_conflicts = app.startup_restore_conflict_count();

    show_centered_callout(
        ctx,
        "startup_restore_conflict_overlay_v1",
        RESTORE_CONFLICT_DIALOG_SIZE,
        |ui| {
            render_restore_conflict_dialog(
                ui,
                &conflict,
                total_conflicts,
                &mut keep_session,
                &mut open_compare,
            );
        },
    );

    if open_compare {
        let _ = app.open_disk_version_for_current_startup_restore_conflict();
    } else if keep_session {
        app.dismiss_current_startup_restore_conflict();
    }
}

fn render_restore_conflict_dialog(
    ui: &mut egui::Ui,
    conflict: &StartupRestoreConflict,
    total_conflicts: usize,
    keep_session: &mut bool,
    open_compare: &mut bool,
) {
    callout::apply_spacing(ui);

    if callout::header_row(ui, "Close restore conflict prompt", |ui| {
        ui.label(
            egui::RichText::new(WARNING)
                .size(16.0)
                .color(callout::muted_text(ui)),
        );
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Restore conflict detected")
                    .size(15.0)
                    .color(callout::text(ui)),
            );
            ui.label(
                egui::RichText::new(conflict_counter_label(total_conflicts))
                    .size(11.5)
                    .color(callout::muted_text(ui)),
            );
        });
    }) {
        *keep_session = true;
    }

    callout::section_frame(ui).show(ui, |ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} changed on disk while Scratchpad was closed. The session version was restored because this tab had unsaved edits.",
                conflict.path.display()
            ))
            .size(12.5)
            .color(callout::text(ui)),
        );
    });

    ui.horizontal_wrapped(|ui| {
        if restore_conflict_button(
            ui,
            FILE_TEXT,
            "Keep Session Version",
            "Continue with the restored session copy",
        ) {
            *keep_session = true;
        }

        if restore_conflict_button(
            ui,
            COPY,
            "Open Disk Version",
            "Open the current file from disk in a separate comparison tab",
        ) {
            *open_compare = true;
        }

        if restore_conflict_button(ui, X, "Dismiss", "Close this prompt for now") {
            *keep_session = true;
        }
    });
}

fn restore_conflict_button(ui: &mut egui::Ui, icon: &str, label: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{icon} {label}"))
                .size(12.0)
                .color(callout::text(ui)),
        )
        .fill(callout::section_fill(ui))
        .corner_radius(egui::CornerRadius::same(8))
        .min_size(egui::vec2(124.0, 34.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}

fn conflict_counter_label(total_conflicts: usize) -> String {
    if total_conflicts == 1 {
        "1 unresolved restore conflict".to_owned()
    } else {
        format!("{total_conflicts} unresolved restore conflicts")
    }
}
