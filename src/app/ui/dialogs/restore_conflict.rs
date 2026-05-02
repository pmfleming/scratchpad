use super::common::{render_icon_choice_dialog, show_centered_callout};
use crate::app::app_state::{ScratchpadApp, StartupRestoreConflict};
use eframe::egui;
use egui_phosphor::regular::{COPY, FILE_TEXT, X};

const RESTORE_CONFLICT_DIALOG_SIZE: egui::Vec2 = egui::vec2(272.0, 154.0);

#[derive(Clone, Copy)]
enum RestoreConflictChoice {
    KeepSession,
    OpenDisk,
    Dismiss,
}

pub(crate) fn show_startup_restore_conflict_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(conflict) = app.current_startup_restore_conflict().cloned() else {
        return;
    };

    let mut choice: Option<RestoreConflictChoice> = None;
    let mut close_requested = false;

    show_centered_callout(
        ctx,
        "startup_restore_conflict_overlay_v1",
        RESTORE_CONFLICT_DIALOG_SIZE,
        |ui| {
            choice = render_restore_conflict_body(ui, &conflict, &mut close_requested);
        },
    );

    match choice {
        Some(RestoreConflictChoice::OpenDisk) => {
            let _ = app.open_disk_version_for_current_startup_restore_conflict();
        }
        Some(RestoreConflictChoice::KeepSession) => {
            app.keep_session_version_for_current_startup_restore_conflict();
        }
        Some(RestoreConflictChoice::Dismiss) => {
            app.dismiss_current_startup_restore_conflict();
        }
        None => {
            if close_requested {
                app.dismiss_current_startup_restore_conflict();
            }
        }
    }
}

fn render_restore_conflict_body(
    ui: &mut egui::Ui,
    conflict: &StartupRestoreConflict,
    close_requested: &mut bool,
) -> Option<RestoreConflictChoice> {
    render_icon_choice_dialog(
        ui,
        &conflict.buffer_name,
        "Restore Conflict Detected",
        close_requested,
        [
            (
                FILE_TEXT,
                "Keep Session Version",
                RestoreConflictChoice::KeepSession,
            ),
            (COPY, "Load Disk Version", RestoreConflictChoice::OpenDisk),
            (X, "Dismiss", RestoreConflictChoice::Dismiss),
        ],
    )
}
