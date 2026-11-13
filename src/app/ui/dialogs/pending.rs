//! Prompts for closing modified files and saving over disk changes.
use super::common::{Choice, Confirm, show_confirm};
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{AppCommand, FileCommand, WorkspaceCommand};
use crate::app::domain::{BufferFreshness, DocumentSnapshot, PendingAction, ViewId};
use crate::app::theme;
use crate::app::ui::recovery as query;
use eframe::egui;
use std::path::PathBuf;
use std::time::Duration;

mod disk_diff;
#[cfg(test)]
mod tests;

const UNSAVED_WIDTH: f32 = 360.0;
const DISK_WIDTH: f32 = 440.0;
const COMPARE_WIDTH: f32 = 720.0;
const DIFF_SIZE: f32 = 11.5;

/// What the tab and its disk file look like when a save runs into a change.
struct DiskPrompt {
    path: Option<PathBuf>,
    path_label: String,
    freshness: BufferFreshness,
    encoding: String,
    snapshot: DocumentSnapshot,
}

#[derive(Clone, Copy)]
enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy)]
enum ConflictChoice {
    Overwrite,
    Reload,
    SaveCopy,
    Cancel,
}

#[derive(Clone, Copy)]
enum MissingChoice {
    Recreate,
    SaveAs,
    Close,
    Cancel,
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
        PendingAction::SaveConflict { tab_index, view_id } => {
            match disk_prompt(app, tab_index, view_id) {
                Some(prompt) if prompt.freshness == BufferFreshness::MissingOnDisk => {
                    show_missing(ctx, app, tab_index, view_id, &prompt);
                }
                Some(prompt) => show_conflict(ctx, app, tab_index, view_id, &prompt),
                None => clear_pending_action(app),
            }
        }
    }
}

fn handle_pending_close_tab(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    match app.tab_manager.tabs.as_slice().get(index) {
        None => clear_pending_action(app),
        Some(tab) if !tab.buffers.buffer.is_dirty => close_pending_tab(app, index),
        Some(tab) => {
            let buffer = &tab.buffers.buffer;
            let path = path_label(buffer);
            let untitled = buffer.path.is_none();
            match ask_unsaved(ctx, app, &path, untitled) {
                Some(UnsavedChoice::Save) => save_and_close_pending_tab(app, index),
                Some(UnsavedChoice::Discard) => close_pending_tab(app, index),
                Some(UnsavedChoice::Cancel) => clear_pending_action(app),
                None => {}
            }
        }
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
        Some(buffer) => {
            let path = path_label(buffer);
            let untitled = buffer.path.is_none();
            match ask_unsaved(ctx, app, &path, untitled) {
                Some(UnsavedChoice::Save) => save_and_close_pending_view(app, tab_index, view_id),
                Some(UnsavedChoice::Discard) => close_pending_view(app, tab_index, view_id),
                Some(UnsavedChoice::Cancel) => clear_pending_action(app),
                None => {}
            }
        }
    }
}

fn path_label(buffer: &crate::app::domain::BufferState) -> String {
    buffer
        .path
        .as_ref()
        .map_or_else(|| buffer.name.clone(), |path| path.display().to_string())
}

fn ask_unsaved(
    ctx: &egui::Context,
    app: &ScratchpadApp,
    path: &str,
    untitled: bool,
) -> Option<UnsavedChoice> {
    show_confirm(
        ctx,
        app.state.app_settings.editor_background_color(),
        Confirm {
            id: "unsaved_changes_panel",
            title: "Unsaved changes",
            path,
            width: UNSAVED_WIDTH,
            hints: "Enter Save   Esc Cancel",
            choices: vec![
                Choice::new("Cancel", "Keep the file open", UnsavedChoice::Cancel).quiet(),
                Choice::new(
                    "Don't save",
                    "Close and discard the changes",
                    UnsavedChoice::Discard,
                ),
                Choice::new(
                    if untitled { "Save…" } else { "Save" },
                    "Save the changes, then close",
                    UnsavedChoice::Save,
                )
                .accent(),
            ],
            enter: Some(UnsavedChoice::Save),
            escape: UnsavedChoice::Cancel,
        },
        |_| {},
        |_| {},
    )
}

fn compare_key() -> egui::Id {
    egui::Id::unique("pending.disk.compare")
}

fn show_conflict(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    prompt: &DiskPrompt,
) {
    let showing = ctx.data(|d| d.get_temp::<bool>(compare_key()).unwrap_or(false));
    let mut compare = showing;
    let choice = show_confirm(
        ctx,
        app.state.app_settings.editor_background_color(),
        Confirm {
            id: "disk_change_panel",
            title: "Changed on disk",
            path: &prompt.path_label,
            width: if showing { COMPARE_WIDTH } else { DISK_WIDTH },
            hints: "Enter Overwrite   Esc Cancel",
            choices: vec![
                Choice::new("Cancel", "Decide later", ConflictChoice::Cancel).quiet(),
                Choice::new(
                    "Save copy…",
                    "Keep this text in a new file",
                    ConflictChoice::SaveCopy,
                ),
                Choice::new(
                    "Reload",
                    "Discard this tab's edits and load the disk version",
                    ConflictChoice::Reload,
                ),
                Choice::new(
                    "Overwrite",
                    "Replace the disk version with this tab",
                    ConflictChoice::Overwrite,
                )
                .accent(),
            ],
            enter: Some(ConflictChoice::Overwrite),
            escape: ConflictChoice::Cancel,
        },
        |ui| {
            ui.selectable_value(&mut compare, true, "Compare")
                .on_hover_text("Show both versions side by side");
            ui.selectable_value(&mut compare, false, "Summary")
                .on_hover_text("Show the first change");
        },
        |ui| disk_change(ui, prompt, showing),
    );
    ctx.data_mut(|d| d.insert_temp(compare_key(), compare));
    let Some(choice) = choice else {
        return;
    };
    ctx.data_mut(|d| d.remove::<bool>(compare_key()));
    let command = match choice {
        ConflictChoice::Overwrite => FileCommand::SaveConflictOverwrite { tab_index, view_id },
        ConflictChoice::Reload => FileCommand::ReloadBufferFromDisk { tab_index, view_id },
        ConflictChoice::SaveCopy => FileCommand::SaveConflictAsCopy { tab_index, view_id },
        ConflictChoice::Cancel => {
            clear_pending_action(app);
            return;
        }
    };
    crate::app::commands::handle_command(app, AppCommand::File(command));
}

/// The first changed lines, or both versions in full.
fn disk_change(ui: &mut egui::Ui, prompt: &DiskPrompt, compare: bool) {
    let Some(path) = prompt.path.clone() else {
        return;
    };
    let encoding = prompt.encoding.clone();
    let snapshot = prompt.snapshot.clone();
    let diff = query::query_with_interval(
        ui,
        ("pending.disk_diff", path.clone(), snapshot.revision()),
        Duration::from_hours(1),
        move || disk_diff::load(&path, &encoding, &snapshot),
    );
    ui.separator();
    let diff = match diff {
        None => {
            muted(ui, "Reading disk file…");
            return;
        }
        Some(Err(error)) => {
            muted(ui, &error);
            return;
        }
        Some(Ok(diff)) => diff,
    };
    if compare {
        ui.columns(2, |columns| {
            super::recovery::text_preview(&mut columns[0], "Disk", &diff.disk);
            super::recovery::text_preview(&mut columns[1], "This tab", &diff.tab);
        });
        return;
    }
    if diff.removed.is_empty() && diff.added.is_empty() {
        muted(ui, "Same text");
        return;
    }
    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, theme::border(ui)))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            muted(ui, &format!("Ln {}", diff.line));
            for (lines, sign, color) in [
                (&diff.removed, '-', theme::diff_del(ui)),
                (&diff.added, '+', theme::diff_add(ui)),
            ] {
                for line in lines {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{sign} {line}"))
                                .monospace()
                                .size(DIFF_SIZE)
                                .color(color),
                        )
                        .truncate(),
                    );
                }
            }
            if diff.more {
                muted(ui, "…");
            }
        });
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .color(theme::text_muted(ui)),
    );
}

fn show_missing(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    prompt: &DiskPrompt,
) {
    let choice = show_confirm(
        ctx,
        app.state.app_settings.editor_background_color(),
        Confirm {
            id: "disk_missing_panel",
            title: "Missing on disk",
            path: &prompt.path_label,
            width: DISK_WIDTH,
            hints: "Enter Recreate   Esc Cancel",
            choices: vec![
                Choice::new(
                    "Cancel",
                    "Keep the tab and decide later",
                    MissingChoice::Cancel,
                )
                .quiet(),
                Choice::new(
                    "Close tab",
                    "Close the tab without saving",
                    MissingChoice::Close,
                ),
                Choice::new(
                    "Save as…",
                    "Save this text somewhere new",
                    MissingChoice::SaveAs,
                ),
                Choice::new(
                    "Recreate",
                    "Write the file again at its original path",
                    MissingChoice::Recreate,
                )
                .accent(),
            ],
            enter: Some(MissingChoice::Recreate),
            escape: MissingChoice::Cancel,
        },
        |_| {},
        |_| {},
    );
    let command = match choice {
        None => return,
        Some(MissingChoice::Recreate) => FileCommand::SaveConflictOverwrite { tab_index, view_id },
        Some(MissingChoice::SaveAs) => FileCommand::SaveMissingAs { tab_index, view_id },
        Some(MissingChoice::Close) => {
            close_pending_view(app, tab_index, view_id);
            return;
        }
        Some(MissingChoice::Cancel) => {
            clear_pending_action(app);
            return;
        }
    };
    crate::app::commands::handle_command(app, AppCommand::File(command));
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

fn disk_prompt(app: &ScratchpadApp, tab_index: usize, view_id: ViewId) -> Option<DiskPrompt> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .get(tab_index)?
        .buffer_for_view(view_id)?;
    match buffer.freshness {
        BufferFreshness::InSync | BufferFreshness::AutoReloaded => None,
        freshness => Some(DiskPrompt {
            path: buffer.path.clone(),
            path_label: path_label(buffer),
            freshness,
            encoding: buffer.format.encoding_name.clone(),
            snapshot: buffer.document_snapshot(),
        }),
    }
}
