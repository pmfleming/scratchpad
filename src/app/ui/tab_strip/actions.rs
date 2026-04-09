use super::layout::HeaderLayout;
use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::*;
use crate::app::commands::AppCommand;
use crate::app::theme::*;
use eframe::egui;

pub(crate) fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let button_spacing = 4.0;
    let width = BUTTON_SIZE.x * 3.0 + button_spacing * 2.0;

    ui.allocate_ui_with_layout(
        egui::vec2(width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if phosphor_button(
                ui,
                egui_phosphor::regular::FOLDER_OPEN,
                BUTTON_SIZE,
                ACTION_BG,
                ACTION_HOVER_BG,
                "Open File",
            )
            .clicked()
            {
                app.handle_command(AppCommand::OpenFile);
            }

            ui.add_space(button_spacing);
            if phosphor_button(
                ui,
                egui_phosphor::regular::FLOPPY_DISK,
                BUTTON_SIZE,
                ACTION_BG,
                ACTION_HOVER_BG,
                "Save As",
            )
            .clicked()
            {
                app.handle_command(AppCommand::SaveFileAs);
            }

            ui.add_space(button_spacing);
            if phosphor_button(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                BUTTON_SIZE,
                ACTION_BG,
                ACTION_HOVER_BG,
                "Search",
            )
            .clicked()
            {
                app.set_warning_status("Search is not implemented yet.");
            }
        },
    );
}

pub(crate) fn show_caption_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) {
    if caption_controls(ui, ctx, layout.caption_controls_width) {
        app.request_exit(ctx);
    }
}
