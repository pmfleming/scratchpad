use super::consume_app_shortcut;
use crate::app::app_state::{ScratchpadApp, workspace::editing};
use crate::app::commands::{AppCommand, DialogCommand};
use crate::app::shortcut_keymap::ShortcutAction;
use eframe::egui;
use std::path::Path;

type UtilityHandler = fn(&mut ScratchpadApp, &egui::Context);

// Order is significant: only the first matching utility action consumes a key.
const UTILITY_HANDLERS: &[(ShortcutAction, UtilityHandler)] = &[
    (ShortcutAction::RenameTab, |app, _| {
        begin_active_tab_rename(app);
    }),
    (ShortcutAction::OpenTextHistory, |app, _| {
        crate::app::commands::handle_command(
            app,
            AppCommand::Dialog(DialogCommand::OpenTextHistory),
        );
    }),
    (ShortcutAction::OpenEncodingDialog, |app, _| {
        crate::app::app_state::frame::open_encoding_dialog(app);
    }),
    (ShortcutAction::OpenStatusHistory, |app, _| {
        app.state.dialogs.status_history.open();
    }),
    (ShortcutAction::CopyActivePath, copy_active_path),
    (ShortcutAction::RevealActivePath, |app, _| {
        reveal_active_path_in_explorer(app);
    }),
    (ShortcutAction::ToggleTabList, |app, _| {
        crate::app::app_state::settings_controller::toggle_tab_list(app);
    }),
    (ShortcutAction::ToggleTabListAutoHide, |app, _| {
        let next = !app.state.app_settings.auto_hide_tab_list();
        crate::app::app_state::settings_controller::set_auto_hide_tab_list(app, next);
    }),
    (ShortcutAction::ToggleReadingOrder, |app, _| {
        editing::toggle_active_buffer_reading_order(app);
    }),
    (ShortcutAction::ToggleControlChars, |app, _| {
        editing::toggle_active_buffer_control_chars(app);
    }),
];

pub(super) fn handle_utility_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    for &(action, handle) in UTILITY_HANDLERS {
        if consume_app_shortcut(app, ctx, action) {
            handle(app, ctx);
            return;
        }
    }
}

fn begin_active_tab_rename(app: &mut ScratchpadApp) {
    if crate::app::app_state::settings_state::showing_settings(app) {
        return;
    }
    if !app.tab_manager.tabs.as_slice().is_empty() {
        crate::app::app_state::workspace::accessors::begin_tab_rename(
            app,
            app.tab_manager.active_tab_index,
        );
    }
}

fn active_buffer_path(app: &ScratchpadApp) -> Option<&Path> {
    app.tab_manager
        .active_tab()
        .and_then(|tab| tab.active_buffer().path.as_deref())
}

fn copy_active_path(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let Some(path) = active_buffer_path(app) else {
        app.state.status.set_warning_status_in_domain(
            crate::app::app_state::StatusDomain::File,
            "No file path to copy.",
        );
        return;
    };
    ctx.copy_text(path.display().to_string());
    app.state.status.set_info_status_in_domain(
        crate::app::app_state::StatusDomain::File,
        "Copied file path.",
    );
}

fn reveal_active_path_in_explorer(app: &mut ScratchpadApp) {
    let Some(path) = active_buffer_path(app) else {
        app.state.status.set_warning_status_in_domain(
            crate::app::app_state::StatusDomain::File,
            "No file path to reveal.",
        );
        return;
    };

    if let Err(error) = crate::app::platform_file::reveal_file(path) {
        app.state.status.set_warning_status_with_detail(
            crate::app::app_state::StatusDomain::File,
            crate::app::platform_file::reveal_file_error_message(),
            error.to_string(),
        );
    }
}
