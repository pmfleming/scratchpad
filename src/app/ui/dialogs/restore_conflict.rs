use super::common::{render_icon_choice_dialog, show_centered_callout};
use crate::app::app_state::{ScratchpadApp, StartupRestoreConflict, workspace::restore_conflict};
use eframe::egui;
use egui_phosphor::regular::{FILE_TEXT, FLOPPY_DISK, X};

const RESTORE_CONFLICT_DIALOG_SIZE: egui::Vec2 = egui::vec2(272.0, 154.0);

#[derive(Clone, Copy)]
enum RestoreConflictChoice {
    KeepSession,
    OpenDisk,
    Dismiss,
}

pub(crate) fn show_startup_restore_conflict_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(conflict) = restore_conflict::current_startup_restore_conflict(app).cloned() else {
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
            let _ = restore_conflict::open_disk_version_for_current_startup_restore_conflict(app);
        }
        Some(RestoreConflictChoice::KeepSession) => {
            restore_conflict::keep_session_version_for_current_startup_restore_conflict(app);
        }
        Some(RestoreConflictChoice::Dismiss) => {
            restore_conflict::dismiss_current_startup_restore_conflict(app);
        }
        None => {
            if close_requested {
                restore_conflict::dismiss_current_startup_restore_conflict(app);
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
            (
                FLOPPY_DISK,
                "Use Disk Version",
                RestoreConflictChoice::OpenDisk,
            ),
            (X, "Dismiss", RestoreConflictChoice::Dismiss),
        ],
    )
}
