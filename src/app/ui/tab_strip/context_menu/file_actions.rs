use super::activate_slot;
use super::menu_ui::{
    OPEN_DISPOSITION_BUTTON_SIZE, OPEN_FILE_SUBMENU_WIDTH, SUBMENU_WIDTH,
    WIDTH as TAB_CONTEXT_MENU_WIDTH, menu_button, open_disposition_button,
    primary_menu_button_enabled, recent_file_button, submenu_button, submenu_button_sized,
};
use crate::app::app_state::{ScratchpadApp, workspace::accessors as workspace_accessors};
use crate::app::commands::{AppCommand, FileCommand, WorkspaceCommand};
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::FileOpenDisposition;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_SQUARE_IN, FILE_PLUS, FLOPPY_DISK, FOLDER_OPEN, PENCIL_SIMPLE_LINE, PLUS,
};
use std::path::PathBuf;

pub(super) fn render_file_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    workspace_index: Option<usize>,
    open_here_enabled: bool,
    rename_enabled: bool,
    save_enabled: bool,
) {
    if menu_button(ui, TAB_CONTEXT_MENU_WIDTH, "New Tab", Some(PLUS), true) {
        crate::app::commands::handle_command(app, AppCommand::Workspace(WorkspaceCommand::NewTab));
        ui.close();
    }
    if app.state.app_settings.recent_files_enabled() {
        render_open_file_actions(ui, app, slot_index, open_here_enabled);
    } else if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Open File Here",
        Some(FOLDER_OPEN),
        open_here_enabled,
    ) {
        activate_slot(app, slot_index);
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFileHere));
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Rename",
        Some(PENCIL_SIMPLE_LINE),
        rename_enabled,
    ) {
        if let Some(index) = workspace_index {
            workspace_accessors::begin_tab_rename(app, index);
        }
        ui.close();
    }
    render_save_actions(ui, app, workspace_index, save_enabled);
}

fn render_open_file_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    open_enabled: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        if primary_menu_button_enabled(
            ui,
            "tab_context.open_file_primary",
            "Open File",
            FOLDER_OPEN,
            open_enabled,
        ) {
            activate_slot(app, slot_index);
            crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFile));
            ui.close();
        }
        render_open_file_submenu(ui, app, slot_index);
    });
}

fn render_open_file_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp, slot_index: usize) {
    submenu_button_sized(
        ui,
        "tab_context.open_file_caret",
        OPEN_FILE_SUBMENU_WIDTH,
        |ui| {
            render_open_file_disposition_buttons(ui, app);
            ui.separator();
            render_recently_closed_files(ui, app, slot_index);
        },
    );
}

fn render_open_file_disposition_buttons(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let current = app.state.app_settings.file_open_disposition();
    let spacing = 6.0;
    let group_width = OPEN_DISPOSITION_BUTTON_SIZE.x * 2.0 + spacing;
    let leading_space = (OPEN_FILE_SUBMENU_WIDTH - group_width).max(0.0) * 0.5;
    ui.horizontal(|ui| {
        ui.add_space(leading_space);
        ui.spacing_mut().item_spacing.x = spacing;
        if open_disposition_button(
            ui,
            "tab_context.open_file.new_tab",
            FILE_PLUS,
            "Open in new tab",
            matches!(current, FileOpenDisposition::NewTab),
        ) {
            crate::app::app_state::settings_controller::set_file_open_disposition(
                app,
                FileOpenDisposition::NewTab,
            );
        }
        if open_disposition_button(
            ui,
            "tab_context.open_file.current_tab",
            ARROW_SQUARE_IN,
            "Open in current tab",
            matches!(current, FileOpenDisposition::CurrentTab),
        ) {
            crate::app::app_state::settings_controller::set_file_open_disposition(
                app,
                FileOpenDisposition::CurrentTab,
            );
        }
    });
}

fn render_recently_closed_files(ui: &mut egui::Ui, app: &mut ScratchpadApp, slot_index: usize) {
    let paths = app
        .state
        .recently_closed_files
        .iter()
        .take(crate::app::app_state::RECENTLY_CLOSED_FILE_LIMIT)
        .cloned()
        .collect::<Vec<_>>();

    if paths.is_empty() {
        let _ = menu_button(ui, OPEN_FILE_SUBMENU_WIDTH, "No Recent Files", None, false);
        return;
    }

    for path in paths {
        if recent_file_button(
            ui,
            ("tab_context.recently_closed_file", path.clone()),
            OPEN_FILE_SUBMENU_WIDTH,
            &path,
        ) {
            activate_slot(app, slot_index);
            open_recent_file(app, path);
            ui.close();
        }
    }
}

fn open_recent_file(app: &mut ScratchpadApp, path: PathBuf) {
    match app.state.app_settings.file_open_disposition() {
        FileOpenDisposition::NewTab => FileController::open_paths_async(app, vec![path]),
        FileOpenDisposition::CurrentTab => {
            FileController::open_external_paths_here_async(app, vec![path]);
        }
    }
}

fn render_save_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    workspace_index: Option<usize>,
    save_enabled: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        if primary_menu_button_enabled(
            ui,
            "tab_context.save_primary",
            "Save",
            FLOPPY_DISK,
            save_enabled,
        ) {
            if let Some(index) = workspace_index {
                crate::app::app_state::workspace_controller::save_file_at(app, index);
            }
            ui.close();
        }
        render_save_submenu(ui, app, save_enabled);
    });
}

fn render_save_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp, save_enabled: bool) {
    submenu_button(ui, "tab_context.save_caret", |ui| {
        if menu_button(
            ui,
            SUBMENU_WIDTH,
            "Save All",
            Some(FLOPPY_DISK),
            save_enabled,
        ) {
            crate::app::commands::handle_command(app, AppCommand::File(FileCommand::SaveAllFiles));
            ui.close();
        }
    });
}
