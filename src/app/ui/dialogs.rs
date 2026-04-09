use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::PendingAction;
use eframe::egui;

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
