use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::commands::AppCommand;
use crate::app::theme::{BUTTON_SIZE, TAB_HEIGHT, action_bg, action_hover_bg};
use eframe::egui;

const PRIMARY_ACTION_SPACING: f32 = 4.0;

pub(super) fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let width = BUTTON_SIZE.x * 3.0 + PRIMARY_ACTION_SPACING * 2.0;
    let search_tooltip = if app.search_open() {
        "Close Search"
    } else {
        "Search"
    };

    ui.allocate_ui_with_layout(
        egui::vec2(width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            primary_action_button(
                ui,
                egui_phosphor::regular::FOLDER_OPEN,
                "Open File",
                || app.handle_command(AppCommand::OpenFile),
            );
            ui.add_space(PRIMARY_ACTION_SPACING);
            primary_action_button(
                ui,
                egui_phosphor::regular::FLOPPY_DISK,
                "Save As",
                || app.handle_command(AppCommand::SaveFileAs),
            );
            ui.add_space(PRIMARY_ACTION_SPACING);
            primary_action_button(ui, egui_phosphor::regular::MAGNIFYING_GLASS, search_tooltip, || {
                app.toggle_search()
            });
        },
    );
}

fn primary_action_button(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    on_click: impl FnOnce(),
) {
    if phosphor_button(
        ui,
        icon,
        BUTTON_SIZE,
        action_bg(ui),
        action_hover_bg(ui),
        tooltip,
    )
    .clicked()
    {
        on_click();
    }
}