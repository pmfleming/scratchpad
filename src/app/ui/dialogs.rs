use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::PendingAction;
use eframe::egui;
use egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE;

pub(crate) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(PendingAction::CloseTab(index)) = app.pending_action() else {
        return;
    };

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
                            ui.label(egui::RichText::new(&entry.action_label).monospace().strong());
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
