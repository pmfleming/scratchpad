use crate::app::app_state::{PendingAction, ScratchpadApp};
use crate::app::commands::AppCommand;
use eframe::egui;

pub(crate) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(action) = app.pending_action else {
        return;
    };

    match action {
        PendingAction::CloseTab(index) => {
            if index >= app.tabs.len() {
                app.pending_action = None;
                return;
            }

            let is_dirty = app.tabs[index].buffer.is_dirty;
            let tab_name = app.tabs[index].buffer.name.clone();

            if !is_dirty {
                app.pending_action = None;
                app.handle_command(AppCommand::CloseTab { index });
                return;
            }

            egui::Window::new("Unsaved Changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("Do you want to save changes to {}?", tab_name));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() && app.save_file_at(index) {
                            app.pending_action = None;
                            app.handle_command(AppCommand::CloseTab { index });
                        }
                        if ui.button("Don't Save").clicked() {
                            app.pending_action = None;
                            app.handle_command(AppCommand::CloseTab { index });
                        }
                        if ui.button("Cancel").clicked() {
                            app.pending_action = None;
                        }
                    });
                });
        }
    }
}