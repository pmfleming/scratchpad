use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{
    AppCommand, FileCommand, SearchCommand, SettingsCommand, WorkspaceCommand,
};
use crate::app::shortcut_keymap::{ShortcutAction, consume_shortcut};
use eframe::egui;

mod tile;
mod traversal;
mod utility;

pub(crate) fn handle_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    handle_global_shortcuts(app, ctx);
    utility::handle_utility_shortcuts(app, ctx);
    handle_file_shortcuts(app, ctx);
    handle_view_shortcuts(app, ctx);
    tile::handle_tile_shortcuts(app, ctx);
    handle_tab_shortcuts(app, ctx);
}

fn handle_global_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !crate::app::app_state::settings_state::showing_settings(app)
        && traversal::handle_region_traversal_shortcut(app, ctx)
    {
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::OpenUserManual) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenUserManual));
        return;
    }

    if !crate::app::app_state::settings_state::showing_settings(app)
        && consume_app_shortcut(app, ctx, ShortcutAction::OpenSearch)
    {
        crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Open));
        ctx.request_repaint();
        return;
    }

    if !crate::app::app_state::settings_state::showing_settings(app)
        && consume_app_shortcut(app, ctx, ShortcutAction::OpenReplace)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::OpenAndReplace),
        );
        ctx.request_repaint();
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::OpenSettings) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::OpenSettings),
        );
        return;
    }

    if crate::app::app_state::settings_state::showing_settings(app)
        && consume_app_shortcut(app, ctx, ShortcutAction::CloseSettings)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::CloseSettings),
        );
        return;
    }

    if app.state.search_state.open() && consume_app_shortcut(app, ctx, ShortcutAction::CloseSearch)
    {
        crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Close));
        ctx.request_repaint();
    }
}

fn handle_file_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if consume_app_shortcut(app, ctx, ShortcutAction::OpenFileHere) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFileHere));
        return;
    }
    if consume_app_shortcut(app, ctx, ShortcutAction::NewTab) {
        crate::app::commands::handle_command(app, AppCommand::Workspace(WorkspaceCommand::NewTab));
    }
    if consume_app_shortcut(app, ctx, ShortcutAction::OpenFile) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFile));
    }
    if consume_app_shortcut(app, ctx, ShortcutAction::SaveFileAs) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::SaveFileAs));
    }
    if consume_app_shortcut(app, ctx, ShortcutAction::SaveFile) {
        crate::app::commands::handle_command(app, AppCommand::File(FileCommand::SaveFile));
    }
}

fn handle_view_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if consume_app_shortcut(app, ctx, ShortcutAction::IncreaseFontSize) {
        crate::app::app_state::settings_controller::set_font_size(
            app,
            app.state.app_settings.font_size() + 1.0,
        );
    }
    if consume_app_shortcut(app, ctx, ShortcutAction::DecreaseFontSize) {
        crate::app::app_state::settings_controller::set_font_size(
            app,
            app.state.app_settings.font_size() - 1.0,
        );
    }
    if consume_app_shortcut(app, ctx, ShortcutAction::ToggleLineNumbers)
        && let Some(tab) = app.tab_manager.active_tab_mut()
    {
        let next_visible = !tab.layout.line_numbers_visible();
        tab.layout.set_line_numbers_visible(next_visible);
        app.tab_manager.mark_session_dirty();
    }
}

fn handle_tab_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if consume_app_shortcut(app, ctx, ShortcutAction::CloseTab) {
        if crate::app::app_state::settings_state::showing_settings(app) {
            crate::app::commands::handle_command(
                app,
                AppCommand::Settings(SettingsCommand::CloseSettings),
            );
        } else if !app.tab_manager.tabs.as_slice().is_empty() {
            crate::app::commands::handle_command(
                app,
                AppCommand::Workspace(WorkspaceCommand::RequestCloseTab {
                    index: app.tab_manager.active_tab_index,
                }),
            );
        }
    }
}

pub(super) fn consume_app_shortcut(
    app: &ScratchpadApp,
    ctx: &egui::Context,
    action: ShortcutAction,
) -> bool {
    consume_shortcut(
        ctx,
        app.state.app_settings.platform_profile(),
        &app.state.app_settings.shortcuts,
        action,
    )
}
