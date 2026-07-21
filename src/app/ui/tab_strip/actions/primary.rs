use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::commands::{AppCommand, FileCommand, SearchCommand};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use crate::app::theme::{BUTTON_SIZE, TAB_HEIGHT, action_bg, action_hover_bg};
use eframe::egui;

const PRIMARY_ACTION_SPACING: f32 = 4.0;

pub(super) fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let width = BUTTON_SIZE.x * 3.0 + PRIMARY_ACTION_SPACING * 2.0;
    let search_tooltip = if app.state.search_state.open() {
        shortcut_tooltips::action(ui.ctx(), ShortcutAction::CloseSearch, "Close Search")
    } else {
        shortcut_tooltips::action(ui.ctx(), ShortcutAction::OpenSearch, "Search")
    };
    let open_tooltip = shortcut_tooltips::action(ui.ctx(), ShortcutAction::OpenFile, "Open File");
    let save_as_tooltip =
        shortcut_tooltips::action(ui.ctx(), ShortcutAction::SaveFileAs, "Save As");

    ui.allocate_ui_with_layout(
        egui::vec2(width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            primary_action_button(
                ui,
                "primary_open_file",
                egui_phosphor::regular::FOLDER_OPEN,
                &open_tooltip,
                || {
                    crate::app::commands::handle_command(
                        app,
                        AppCommand::File(FileCommand::OpenFile),
                    );
                },
            );
            ui.add_space(PRIMARY_ACTION_SPACING);
            primary_action_button(
                ui,
                "primary_save_as",
                egui_phosphor::regular::FLOPPY_DISK,
                &save_as_tooltip,
                || {
                    crate::app::commands::handle_command(
                        app,
                        AppCommand::File(FileCommand::SaveFileAs),
                    );
                },
            );
            ui.add_space(PRIMARY_ACTION_SPACING);
            primary_action_button(
                ui,
                "primary_search",
                egui_phosphor::regular::MAGNIFYING_GLASS,
                &search_tooltip,
                || {
                    crate::app::commands::handle_command(
                        app,
                        AppCommand::Search(SearchCommand::Toggle),
                    );
                },
            );
        },
    );
}

fn primary_action_button(
    ui: &mut egui::Ui,
    id_source: &'static str,
    icon: &str,
    tooltip: &str,
    on_click: impl FnOnce(),
) {
    if phosphor_button(
        ui,
        id_source,
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
