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
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Reopen Last Closed Tab",
        None,
        !app.startup_restore_pending() && !app.state.persistence.session_restore_failed,
    ) {
        crate::app::app_state::workspace::lifecycle::reopen_last_closed_tab(app);
        ui.close();
    }
    egui::containers::menu::SubMenuButton::new("Recently Closed")
        .config(egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
        .ui(ui, |ui| render_recently_closed_files(ui, app, slot_index));
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
    if let Some(index) = workspace_index {
        render_history_action(ui, app, index);
    }
}

fn render_history_action(ui: &mut egui::Ui, app: &mut ScratchpadApp, index: usize) {
    if menu_button(ui, TAB_CONTEXT_MENU_WIDTH, "History…", Some(egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE), true) {
        crate::app::commands::handle_command(app, AppCommand::Workspace(WorkspaceCommand::ActivateTab { index }));
        crate::app::commands::handle_command(app, AppCommand::Dialog(crate::app::commands::DialogCommand::OpenTextHistory));
        ui.close();
    }
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

fn render_open_file_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp, _slot_index: usize) {
    submenu_button_sized(
        ui,
        "tab_context.open_file_caret",
        OPEN_FILE_SUBMENU_WIDTH,
        |ui| {
            render_open_file_disposition_buttons(ui, app);

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
    let store = app.state.persistence.session_store.clone();
    let root = store.root().to_owned();
    let archives = match crate::app::ui::recovery::query(ui, ("closed", root), move || store.closed_tabs()) {
        Some(Ok(archives)) => archives,
        Some(Err(error)) => { ui.label(format!("Could not read archives: {error}")); Vec::new() }
        None => { ui.label("Loading archives…"); Vec::new() }
    };
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        for archive in &archives {
            ui.horizontal(|ui| {
                let label = format!("{} · Recoverable tab{}", archive.name, if archive.has_unsaved_changes { " · unsaved" } else { "" });
                if ui.button(label).on_hover_text(crate::app::ui::recovery::timestamp(archive.closed_at_unix_seconds)).clicked() {
                    crate::app::app_state::workspace::lifecycle::reopen_archived_tab(app, &archive.token);
                    ui.close();
                }
                if ui.button("Archived text").on_hover_text("Recover the archived text and undo history into untitled buffers, regardless of disk changes").clicked() {
                    crate::app::app_state::workspace::lifecycle::recover_archived_copy(app, &archive.token);
                    ui.close();
                }
                let mut pinned = app.state.app_settings.history.recovery.pinned_archives.contains(&archive.token);
                if ui.checkbox(&mut pinned, "Pin").changed() {
                    let mut policy = app.state.app_settings.history.recovery.clone();
                    policy.pinned_archives.retain(|token| token != &archive.token);
                    if pinned { policy.pinned_archives.push(archive.token.clone()); }
                    crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
                }
            });
        }
    });
    let paths = app.state.recently_closed_files.iter()
        .filter(|_| app.state.app_settings.recent_files_enabled())
        .filter(|path| !archives.iter().any(|archive| archive.paths.iter().any(|saved| crate::app::paths_match(saved, path))))
        .cloned().collect::<Vec<_>>();
    if paths.is_empty() && archives.is_empty() { ui.label("No recently closed files"); }
    if !paths.is_empty() { ui.separator(); ui.label("Files on disk"); }

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
