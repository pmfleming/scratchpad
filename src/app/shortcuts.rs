use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{
    AppCommand, FileCommand, SearchCommand, SettingsCommand, WorkspaceCommand,
};
use crate::app::shortcut_keymap::{ShortcutAction, consume_shortcut};
use eframe::egui;

mod tile;
mod traversal;
mod utility;

#[cfg(test)]
mod tests;

pub(crate) fn handle_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    handle_global_shortcuts(app, ctx);
    utility::handle_utility_shortcuts(app, ctx);
    handle_file_shortcuts(app, ctx);
    handle_view_shortcuts(app, ctx);
    tile::handle_tile_shortcuts(app, ctx);
    handle_tab_shortcuts(app, ctx);
}

/// When a global shortcut is live.
#[derive(Clone, Copy)]
enum GlobalScope {
    Always,
    OutsideSettings,
    InSettings,
    SearchOpen,
}

impl GlobalScope {
    fn applies(self, app: &ScratchpadApp) -> bool {
        let showing_settings = || crate::app::app_state::settings_state::showing_settings(app);
        match self {
            Self::Always => true,
            Self::OutsideSettings => !showing_settings(),
            Self::InSettings => showing_settings(),
            Self::SearchOpen => app.state.search_state.open(),
        }
    }
}

/// Global shortcuts in priority order; the first one consumed wins.
/// The flag requests a repaint so search panels open or close immediately.
fn global_shortcuts() -> [(ShortcutAction, GlobalScope, AppCommand, bool); 6] {
    use GlobalScope::{Always, InSettings, OutsideSettings, SearchOpen};
    [
        (
            ShortcutAction::OpenUserManual,
            Always,
            AppCommand::File(FileCommand::OpenUserManual),
            false,
        ),
        (
            ShortcutAction::OpenSearch,
            OutsideSettings,
            AppCommand::Search(SearchCommand::Open),
            true,
        ),
        (
            ShortcutAction::OpenReplace,
            OutsideSettings,
            AppCommand::Search(SearchCommand::OpenAndReplace),
            true,
        ),
        (
            ShortcutAction::OpenSettings,
            Always,
            AppCommand::Settings(SettingsCommand::OpenSettings),
            false,
        ),
        (
            ShortcutAction::CloseSettings,
            InSettings,
            AppCommand::Settings(SettingsCommand::CloseSettings),
            false,
        ),
        (
            ShortcutAction::CloseSearch,
            SearchOpen,
            AppCommand::Search(SearchCommand::Close),
            true,
        ),
    ]
}

fn handle_global_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if GlobalScope::OutsideSettings.applies(app)
        && traversal::handle_region_traversal_shortcut(app, ctx)
    {
        return;
    }

    for (action, scope, command, repaint) in global_shortcuts() {
        if scope.applies(app) && consume_app_shortcut(app, ctx, action) {
            crate::app::commands::handle_command(app, command);
            if repaint {
                ctx.request_repaint();
            }
            return;
        }
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
    use crate::app::app_state::workspace::zoom::{self, ZoomChange};
    for (shortcut, change) in [
        (ShortcutAction::IncreaseFontSize, ZoomChange::In),
        (ShortcutAction::DecreaseFontSize, ZoomChange::Out),
        (ShortcutAction::ResetZoom, ZoomChange::Reset),
    ] {
        if consume_app_shortcut(app, ctx, shortcut) {
            zoom::active_view(app, change);
            crate::app::ui::dialogs::zoom_scrubber::open(app);
            ctx.request_repaint();
        }
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

/// Consume the first pressed shortcut in `bindings` and return its value.
pub(super) fn first_consumed<T: Copy>(
    app: &ScratchpadApp,
    ctx: &egui::Context,
    bindings: &[(ShortcutAction, T)],
) -> Option<T> {
    bindings
        .iter()
        .find_map(|&(action, value)| consume_app_shortcut(app, ctx, action).then_some(value))
}
