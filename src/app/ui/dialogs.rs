use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferFreshness, PendingAction};
use crate::app::services::file_controller::FileController;
use eframe::egui;
use egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE;

pub(crate) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(action) = app.pending_action() else {
        return;
    };

    match action {
        PendingAction::CloseTab(index) => {
            if !pending_close_tab_is_valid(app, index) {
                clear_pending_action(app);
                return;
            }

            if !app.tabs()[index].buffer.is_dirty {
                close_pending_tab(app, index);
                return;
            }

            show_close_tab_confirmation(ctx, app, index);
        }
        PendingAction::SaveConflict(index) => {
            if !pending_save_conflict_is_valid(app, index) {
                clear_pending_action(app);
                return;
            }

            show_save_conflict_confirmation(ctx, app, index);
        }
    }
}

pub(crate) fn show_transaction_log_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.transaction_log_open() {
        return;
    }

    let mut open = true;
    let entries = app.transaction_log_entries().to_vec();
    let mut undo_entry_id = None;

    egui::Window::new("Transaction Log")
        .open(&mut open)
        .resizable(true)
        .default_size(egui::vec2(480.0, 320.0))
        .show(ctx, |ui| {
            if entries.is_empty() {
                ui.label("No undoable workspace transactions yet.");
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for entry in entries.iter().rev() {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let undo_button = ui
                                .button(ARROW_COUNTER_CLOCKWISE)
                                .on_hover_text("Undo to this point");
                            if undo_button.clicked() {
                                undo_entry_id = Some(entry.id);
                            }
                            ui.label(
                                egui::RichText::new(&entry.action_label)
                                    .monospace()
                                    .strong(),
                            );
                            if !entry.affected_items.is_empty() {
                                ui.label(
                                    egui::RichText::new(entry.affected_items.join(", ")).small(),
                                );
                            }
                        });
                        if let Some(details) = &entry.details {
                            ui.label(egui::RichText::new(details).small().italics());
                        }
                    });
                    ui.separator();
                }
            });
        });

    if let Some(entry_id) = undo_entry_id {
        let _ = app.undo_transaction_entry(entry_id);
        return;
    }

    if !open {
        app.close_transaction_log();
    }
}

fn pending_close_tab_is_valid(app: &ScratchpadApp, index: usize) -> bool {
    index < app.tabs().len()
}

fn pending_save_conflict_is_valid(app: &ScratchpadApp, index: usize) -> bool {
    index < app.tabs().len()
        && app.tabs()[index].active_buffer().path.is_some()
        && !matches!(
            app.tabs()[index].active_buffer().freshness,
            BufferFreshness::InSync
        )
}

fn show_close_tab_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    let tab_name = app.tabs()[index].buffer.name.clone();

    egui::Window::new("Unsaved Changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("Do you want to save changes to {}?", tab_name));
            ui.horizontal(|ui| {
                render_close_tab_confirmation_buttons(ui, app, index);
            });
        });
}

fn show_save_conflict_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    let buffer = app.tabs()[index].active_buffer();
    let path_label = buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| buffer.name.clone());
    let freshness = buffer.freshness;
    let title = match freshness {
        BufferFreshness::MissingOnDisk => "File Missing on Disk",
        _ => "File Changed on Disk",
    };
    let message = match freshness {
        BufferFreshness::ConflictOnDisk => {
            format!("{path_label} changed on disk while this tab has unsaved edits.")
        }
        BufferFreshness::MissingOnDisk => {
            format!("{path_label} is missing on disk, but this tab still has content.")
        }
        BufferFreshness::StaleOnDisk => format!("{path_label} changed on disk."),
        BufferFreshness::InSync => return,
    };

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(message);
            ui.horizontal(|ui| {
                let primary_label = if freshness == BufferFreshness::MissingOnDisk {
                    "Recreate"
                } else {
                    "Overwrite"
                };
                if ui.button(primary_label).clicked()
                    && FileController::save_conflict_overwrite(app, index)
                {
                    clear_pending_action(app);
                }
                if freshness != BufferFreshness::MissingOnDisk
                    && ui.button("Reload").clicked()
                    && FileController::reload_buffer_from_disk(app, index)
                {
                    clear_pending_action(app);
                }
                if ui.button("Save As Copy").clicked() && app.save_file_as_at(index) {
                    clear_pending_action(app);
                }
                if ui.button("Cancel").clicked() {
                    clear_pending_action(app);
                }
            });
        });
}

fn render_close_tab_confirmation_buttons(ui: &mut egui::Ui, app: &mut ScratchpadApp, index: usize) {
    if ui.button("Save").clicked() {
        save_and_close_pending_tab(app, index);
    }
    if ui.button("Don't Save").clicked() {
        close_pending_tab(app, index);
    }
    if ui.button("Cancel").clicked() {
        clear_pending_action(app);
    }
}

fn save_and_close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    if app.save_file_at(index) {
        close_pending_tab(app, index);
    }
}

fn close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    clear_pending_action(app);
    app.handle_command(AppCommand::CloseTab { index });
}

fn clear_pending_action(app: &mut ScratchpadApp) {
    app.set_pending_action(None);
}
