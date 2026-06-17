use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{
    AppCommand, FileCommand, SearchCommand, SettingsCommand, WorkspaceCommand,
};
use crate::app::domain::{SplitAxis, ViewId};
use crate::app::shortcut_keymap::{ShortcutAction, consume_shortcut};
use eframe::egui;
use std::path::PathBuf;

const DEFAULT_SPLIT_RATIO: f32 = 0.5;

pub(crate) fn handle_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    handle_global_shortcuts(app, ctx);
    handle_utility_shortcuts(app, ctx);
    handle_file_shortcuts(app, ctx);
    handle_view_shortcuts(app, ctx);
    handle_tile_shortcuts(app, ctx);
    handle_tab_shortcuts(app, ctx);
}

fn handle_global_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !crate::app::app_state::settings_state::showing_settings(app)
        && handle_region_traversal_shortcut(app, ctx)
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

fn handle_utility_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if consume_app_shortcut(app, ctx, ShortcutAction::RenameTab) {
        begin_active_tab_rename(app);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::OpenTextHistory) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Dialog(crate::app::commands::DialogCommand::OpenTextHistory),
        );
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::OpenEncodingDialog) {
        crate::app::app_state::frame::open_encoding_dialog(app);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::OpenStatusHistory) {
        app.state.dialogs.status_history.open();
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::CopyActivePath) {
        copy_active_path(app, ctx);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::RevealActivePath) {
        reveal_active_path_in_explorer(app);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::ToggleTabList) {
        crate::app::app_state::settings_controller::toggle_tab_list(app);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::ToggleTabListAutoHide) {
        let next = !app.state.app_settings.auto_hide_tab_list();
        crate::app::app_state::settings_controller::set_auto_hide_tab_list(app, next);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::ToggleReadingOrder) {
        toggle_active_buffer_reading_order(app);
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::ToggleControlChars) {
        toggle_active_buffer_control_chars(app);
    }
}

fn handle_region_traversal_shortcut(app: &mut ScratchpadApp, ctx: &egui::Context) -> bool {
    let direction = if consume_app_shortcut(app, ctx, ShortcutAction::TraverseRegionForward) {
        Some(1)
    } else if consume_app_shortcut(app, ctx, ShortcutAction::TraverseRegionBackward) {
        Some(-1)
    } else {
        None
    };
    let Some(direction) = direction else {
        return false;
    };
    let Some(next_view_id) = next_view_for_region_traversal(app, direction) else {
        return true;
    };

    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateView {
            view_id: next_view_id,
        }),
    );
    true
}

fn next_view_for_region_traversal(app: &ScratchpadApp, direction: i32) -> Option<ViewId> {
    let tab = app.tab_manager.active_tab()?;
    let ordered = tab.ordered_view_ids_in_layout_order();
    if ordered.len() <= 1 {
        return None;
    }

    let current = ordered
        .iter()
        .position(|view_id| *view_id == tab.layout.active_view_id)
        .unwrap_or(0);
    let next = next_region_index(current, ordered.len(), direction)?;
    ordered.get(next).copied()
}

fn next_region_index(current: usize, len: usize, direction: i32) -> Option<usize> {
    if len <= 1 {
        return None;
    }
    Some(if direction < 0 {
        current.checked_sub(1).unwrap_or(len - 1)
    } else {
        (current + 1) % len
    })
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

fn handle_tile_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if consume_app_shortcut(app, ctx, ShortcutAction::PromoteTileToTab)
        && let Some(tab) = app.tab_manager.active_tab()
        && crate::app::domain::tab::summary::can_promote_view(tab, tab.layout.active_view_id)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::PromoteViewToTab {
                view_id: tab.layout.active_view_id,
            }),
        );
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::PromoteTabFilesToTabs)
        && let Some(tab) = app.tab_manager.active_tab()
        && crate::app::domain::tab::summary::can_promote_all_files(tab)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::PromoteTabFilesToTabs {
                index: app.tab_manager.active_tab_index,
            }),
        );
        return;
    }

    if consume_app_shortcut(app, ctx, ShortcutAction::CloseTile)
        && let Some(tab) = app.tab_manager.active_tab()
        && tab.layout.root_pane.leaf_count() > 1
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::CloseView {
                view_id: tab.layout.active_view_id,
            }),
        );
        return;
    }

    let split = if consume_app_shortcut(app, ctx, ShortcutAction::SplitUp) {
        Some((SplitAxis::Horizontal, true))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitDown) {
        Some((SplitAxis::Horizontal, false))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitLeft) {
        Some((SplitAxis::Vertical, true))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitRight) {
        Some((SplitAxis::Vertical, false))
    } else {
        None
    };

    if let Some((axis, new_view_first)) = split {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::SplitActiveView {
                axis,
                new_view_first,
                ratio: DEFAULT_SPLIT_RATIO,
            }),
        );
    }
}

fn consume_app_shortcut(app: &ScratchpadApp, ctx: &egui::Context, action: ShortcutAction) -> bool {
    consume_shortcut(
        ctx,
        app.state.app_settings.platform_profile(),
        &app.state.app_settings.shortcuts,
        action,
    )
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

fn active_buffer_path(app: &ScratchpadApp) -> Option<PathBuf> {
    app.tab_manager
        .active_tab()
        .and_then(|tab| tab.active_buffer().path.clone())
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

    if let Err(error) = crate::app::platform_file::reveal_file(&path) {
        app.state.status.set_warning_status_with_detail(
            crate::app::app_state::StatusDomain::File,
            crate::app::platform_file::reveal_file_error_message(),
            error.to_string(),
        );
    }
}

fn toggle_active_buffer_reading_order(app: &mut ScratchpadApp) {
    if let Some(tab) = app.tab_manager.active_tab_mut()
        && let Some(buffer_id) = tab.layout.active_view().map(|view| view.buffer_id)
    {
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer.right_to_left_reading_order = !buffer.right_to_left_reading_order;
        }
        for view in &mut tab.layout.views {
            if view.buffer_id == buffer_id {
                view.layout_cache.clear();
            }
        }
        app.tab_manager.mark_session_dirty();
    }
}

fn toggle_active_buffer_control_chars(app: &mut ScratchpadApp) {
    if let Some(tab) = app.tab_manager.active_tab_mut()
        && let Some(buffer_id) = tab.layout.active_view().map(|view| view.buffer_id)
    {
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer.show_control_chars = !buffer.show_control_chars;
        }
        app.tab_manager.mark_session_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::next_region_index;

    #[test]
    fn f6_traversal_wraps_forward() {
        assert_eq!(next_region_index(2, 3, 1), Some(0));
    }

    #[test]
    fn shift_f6_traversal_wraps_backward() {
        assert_eq!(next_region_index(0, 3, -1), Some(2));
    }
}
