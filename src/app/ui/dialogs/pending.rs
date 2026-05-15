use super::common::{
    render_dialog_action_button, render_icon_choice_dialog, show_centered_callout,
};
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{AppCommand, FileCommand, WorkspaceCommand};
use crate::app::domain::{BufferFreshness, PendingAction, ViewId};
use crate::app::ui::callout;
use eframe::egui;
use egui_phosphor::regular::{ARROW_CLOCKWISE, COPY, FLOPPY_DISK, TRASH, WARNING, X};

const UNSAVED_CHANGES_SIZE: egui::Vec2 = egui::vec2(272.0, 154.0);
const MISSING_FILE_DIALOG_SIZE: egui::Vec2 = egui::vec2(432.0, 154.0);
const SAVE_CONFLICT_DIALOG_SIZE: egui::Vec2 = egui::vec2(432.0, 214.0);

struct SaveConflictDialogState {
    title: &'static str,
    message: String,
    path_label: String,
    freshness: BufferFreshness,
}

#[derive(Clone, Copy)]
enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

impl SaveConflictDialogState {
    fn from_freshness(path_label: String, freshness: BufferFreshness) -> Option<Self> {
        let (title, message) = match freshness {
            BufferFreshness::ConflictOnDisk => (
                "File Changed on Disk",
                format!("{path_label} changed on disk while this tab has unsaved edits."),
            ),
            BufferFreshness::MissingOnDisk => (
                "File Missing on Disk",
                format!("{path_label} is missing on disk, but this tab still has content."),
            ),
            BufferFreshness::StaleOnDisk => (
                "File Changed on Disk",
                format!("{path_label} changed on disk."),
            ),
            BufferFreshness::InSync | BufferFreshness::AutoReloaded => return None,
        };

        Some(Self {
            title,
            message,
            path_label,
            freshness,
        })
    }

    fn primary_action_label(&self) -> &'static str {
        "Overwrite"
    }

    fn can_reload(&self) -> bool {
        self.freshness != BufferFreshness::MissingOnDisk
    }

    fn is_missing_on_disk(&self) -> bool {
        self.freshness == BufferFreshness::MissingOnDisk
    }
}

pub(crate) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(action) = crate::app::app_state::workspace::accessors::pending_action(app) else {
        return;
    };

    match action {
        PendingAction::CloseTab(index) => handle_pending_close_tab(ctx, app, index),
        PendingAction::CloseView { tab_index, view_id } => {
            handle_pending_close_view(ctx, app, tab_index, view_id);
        }
        PendingAction::SaveConflict { tab_index, view_id }
            if save_conflict_dialog_state(app, tab_index, view_id).is_some() =>
        {
            show_save_conflict_confirmation(ctx, app, tab_index, view_id);
        }
        PendingAction::SaveConflict { .. } => clear_pending_action(app),
    }
}

fn handle_pending_close_tab(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    match app.tab_manager.tabs.as_slice().get(index) {
        None => clear_pending_action(app),
        Some(tab) if !tab.buffers.buffer.is_dirty => close_pending_tab(app, index),
        Some(_) => show_close_tab_confirmation(ctx, app, index),
    }
}

fn handle_pending_close_view(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) {
    let Some(tab) = app.tab_manager.tabs.as_slice().get(tab_index) else {
        clear_pending_action(app);
        return;
    };

    if tab.is_last_view_for_buffer(view_id) != Some(true) {
        close_pending_view(app, tab_index, view_id);
        return;
    }

    match tab.buffer_for_view(view_id) {
        None => clear_pending_action(app),
        Some(buffer) if !buffer.is_dirty => close_pending_view(app, tab_index, view_id),
        Some(_) => show_close_view_confirmation(ctx, app, tab_index, view_id),
    }
}

fn show_close_tab_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    let tab_name = app.tab_manager.tabs.as_slice()[index]
        .buffers
        .buffer
        .name
        .clone();
    let mut close_requested = false;

    show_centered_callout(
        ctx,
        "unsaved_changes_overlay_v3",
        UNSAVED_CHANGES_SIZE,
        |ui| render_unsaved_changes_dialog(ui, &tab_name, app, index, &mut close_requested),
    );

    if close_requested {
        clear_pending_action(app);
    }
}

fn show_close_view_confirmation(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) {
    let Some(tab_name) = app
        .tab_manager
        .tabs
        .as_slice()
        .get(tab_index)
        .and_then(|tab| tab.buffer_for_view(view_id))
        .map(|buffer| buffer.name.clone())
    else {
        clear_pending_action(app);
        return;
    };
    let mut close_requested = false;

    show_centered_callout(
        ctx,
        "unsaved_changes_overlay_v3",
        UNSAVED_CHANGES_SIZE,
        |ui| {
            render_unsaved_changes_view_dialog(
                ui,
                &tab_name,
                app,
                tab_index,
                view_id,
                &mut close_requested,
            );
        },
    );

    if close_requested {
        clear_pending_action(app);
    }
}

fn show_save_conflict_confirmation(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) {
    let Some(state) = save_conflict_dialog_state(app, tab_index, view_id) else {
        return;
    };

    let mut close_requested = false;
    let dialog_size = if state.is_missing_on_disk() {
        MISSING_FILE_DIALOG_SIZE
    } else {
        SAVE_CONFLICT_DIALOG_SIZE
    };

    show_centered_callout(ctx, "file_change_overlay_v1", dialog_size, |ui| {
        render_save_conflict_dialog(ui, app, tab_index, view_id, &state, &mut close_requested);
    });

    if close_requested {
        clear_pending_action(app);
    }
}

fn render_unsaved_changes_view_dialog(
    ui: &mut egui::Ui,
    tab_name: &str,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    close_requested: &mut bool,
) {
    match render_unsaved_changes_body(ui, tab_name, close_requested) {
        Some(UnsavedChoice::Save) => save_and_close_pending_view(app, tab_index, view_id),
        Some(UnsavedChoice::Discard) => close_pending_view(app, tab_index, view_id),
        Some(UnsavedChoice::Cancel) => *close_requested = true,
        None => {}
    }
}

fn render_unsaved_changes_dialog(
    ui: &mut egui::Ui,
    tab_name: &str,
    app: &mut ScratchpadApp,
    index: usize,
    close_requested: &mut bool,
) {
    match render_unsaved_changes_body(ui, tab_name, close_requested) {
        Some(UnsavedChoice::Save) => save_and_close_pending_tab(app, index),
        Some(UnsavedChoice::Discard) => close_pending_tab(app, index),
        Some(UnsavedChoice::Cancel) => *close_requested = true,
        None => {}
    }
}

fn render_unsaved_changes_body(
    ui: &mut egui::Ui,
    tab_name: &str,
    close_requested: &mut bool,
) -> Option<UnsavedChoice> {
    render_icon_choice_dialog(
        ui,
        tab_name,
        "Unsaved Changes",
        close_requested,
        [
            (FLOPPY_DISK, "Save changes", UnsavedChoice::Save),
            (TRASH, "Discard changes", UnsavedChoice::Discard),
            (X, "Cancel", UnsavedChoice::Cancel),
        ],
    )
}

fn save_and_close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    if crate::app::app_state::workspace_controller::save_file_at(app, index) {
        close_pending_tab(app, index);
    }
}

fn save_and_close_pending_view(app: &mut ScratchpadApp, tab_index: usize, view_id: ViewId) {
    if !activate_pending_view(app, tab_index, view_id) {
        clear_pending_action(app);
        return;
    }

    if crate::app::app_state::workspace_controller::save_file_at(app, tab_index) {
        close_pending_view(app, tab_index, view_id);
    }
}

fn render_save_conflict_dialog(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    state: &SaveConflictDialogState,
    close_requested: &mut bool,
) {
    if state.is_missing_on_disk() {
        render_missing_file_dialog(ui, app, tab_index, view_id, state, close_requested);
        return;
    }

    callout::apply_spacing(ui);

    if callout::header_row(
        ui,
        "pending_file_change.header",
        "Close file change prompt",
        |ui| {
            ui.label(
                egui::RichText::new(WARNING)
                    .size(16.0)
                    .color(callout::muted_text(ui)),
            );
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(state.title)
                        .size(15.0)
                        .color(callout::text(ui)),
                );
                ui.label(
                    egui::RichText::new("Resolve the on-disk mismatch before continuing.")
                        .size(11.5)
                        .color(callout::muted_text(ui)),
                );
            });
        },
    ) {
        *close_requested = true;
    }

    callout::section_frame(ui).show(ui, |ui| {
        ui.label(
            egui::RichText::new(&state.message)
                .size(12.5)
                .color(callout::text(ui)),
        );
    });

    ui.horizontal_wrapped(|ui| {
        if render_dialog_action_button(
            ui,
            "save_conflict.overwrite",
            FLOPPY_DISK,
            state.primary_action_label(),
            "Write the current buffer back to disk",
        ) {
            crate::app::commands::handle_command(
                app,
                AppCommand::File(FileCommand::SaveConflictOverwrite { tab_index, view_id }),
            );
        }

        if state.can_reload()
            && render_dialog_action_button(
                ui,
                "save_conflict.reload",
                ARROW_CLOCKWISE,
                "Reload",
                "Discard local buffer state and reload from disk",
            )
        {
            crate::app::commands::handle_command(
                app,
                AppCommand::File(FileCommand::ReloadBufferFromDisk { tab_index, view_id }),
            );
        }

        if render_dialog_action_button(
            ui,
            "save_conflict.save_as_copy",
            COPY,
            "Save As Copy",
            "Keep this buffer by saving it to a new file",
        ) {
            crate::app::commands::handle_command(
                app,
                AppCommand::File(FileCommand::SaveConflictAsCopy { tab_index, view_id }),
            );
        }

        if render_dialog_action_button(
            ui,
            "save_conflict.cancel",
            X,
            "Cancel",
            "Dismiss this prompt",
        ) {
            *close_requested = true;
        }
    });
}

fn render_missing_file_dialog(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    state: &SaveConflictDialogState,
    close_requested: &mut bool,
) {
    if let Some(action) = render_icon_choice_dialog(
        ui,
        &state.path_label,
        "File Missing on Disk",
        close_requested,
        [
            (
                FLOPPY_DISK,
                "Recreate the file at its original path",
                MissingFileChoice::Save,
            ),
            (
                TRASH,
                "Discard this missing file tab",
                MissingFileChoice::Discard,
            ),
        ],
    ) {
        match action {
            MissingFileChoice::Save => {
                crate::app::commands::handle_command(
                    app,
                    AppCommand::File(FileCommand::SaveConflictOverwrite { tab_index, view_id }),
                );
            }
            MissingFileChoice::Discard => close_pending_view(app, tab_index, view_id),
        }
    }
}

#[derive(Clone, Copy)]
enum MissingFileChoice {
    Save,
    Discard,
}

fn close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    clear_pending_action(app);
    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::CloseTab { index }),
    );
}

fn close_pending_view(app: &mut ScratchpadApp, tab_index: usize, view_id: ViewId) {
    clear_pending_action(app);
    if activate_pending_view(app, tab_index, view_id) {
        crate::app::commands::perform_close_view(app, view_id);
    }
}

fn activate_pending_view(app: &mut ScratchpadApp, tab_index: usize, view_id: ViewId) -> bool {
    crate::app::commands::activate_pending_view_command(app, tab_index, view_id)
}

fn clear_pending_action(app: &mut ScratchpadApp) {
    crate::app::app_state::workspace::accessors::set_pending_action(app, None);
}

fn save_conflict_dialog_state(
    app: &ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> Option<SaveConflictDialogState> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .get(tab_index)?
        .buffer_for_view(view_id)?;
    let path_label = buffer
        .path
        .as_ref()
        .map_or_else(|| buffer.name.clone(), |path| path.display().to_string());
    SaveConflictDialogState::from_freshness(path_label, buffer.freshness)
}
