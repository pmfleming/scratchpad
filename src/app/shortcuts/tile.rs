use super::consume_app_shortcut;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::domain::{SplitAxis, TileDirection};
use crate::app::shortcut_keymap::ShortcutAction;
use eframe::egui;

const DEFAULT_SPLIT_RATIO: f32 = 0.5;

pub(super) fn handle_tile_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if handle_tile_promotion_shortcuts(app, ctx) || handle_tile_close_shortcut(app, ctx) {
        return;
    }

    if let Some(direction) = consume_tile_move(app, ctx) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::MoveActiveTile { direction }),
        );
        return;
    }

    if let Some(direction) = consume_tile_resize(app, ctx) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ResizeActiveTile { direction }),
        );
        return;
    }

    if let Some((axis, new_view_first)) = consume_tile_split(app, ctx) {
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

fn handle_tile_promotion_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) -> bool {
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
        return true;
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
        return true;
    }

    false
}

fn handle_tile_close_shortcut(app: &mut ScratchpadApp, ctx: &egui::Context) -> bool {
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
        return true;
    }

    false
}

fn consume_tile_move(app: &ScratchpadApp, ctx: &egui::Context) -> Option<TileDirection> {
    consume_directional_shortcut(
        app,
        ctx,
        DirectionalActions {
            left: ShortcutAction::MoveTileLeft,
            right: ShortcutAction::MoveTileRight,
            up: ShortcutAction::MoveTileUp,
            down: ShortcutAction::MoveTileDown,
        },
    )
}

fn consume_tile_resize(app: &ScratchpadApp, ctx: &egui::Context) -> Option<TileDirection> {
    consume_directional_shortcut(
        app,
        ctx,
        DirectionalActions {
            left: ShortcutAction::ResizeTileLeft,
            right: ShortcutAction::ResizeTileRight,
            up: ShortcutAction::ResizeTileUp,
            down: ShortcutAction::ResizeTileDown,
        },
    )
}

struct DirectionalActions {
    left: ShortcutAction,
    right: ShortcutAction,
    up: ShortcutAction,
    down: ShortcutAction,
}

fn consume_directional_shortcut(
    app: &ScratchpadApp,
    ctx: &egui::Context,
    actions: DirectionalActions,
) -> Option<TileDirection> {
    if consume_app_shortcut(app, ctx, actions.left) {
        Some(TileDirection::Left)
    } else if consume_app_shortcut(app, ctx, actions.right) {
        Some(TileDirection::Right)
    } else if consume_app_shortcut(app, ctx, actions.up) {
        Some(TileDirection::Up)
    } else if consume_app_shortcut(app, ctx, actions.down) {
        Some(TileDirection::Down)
    } else {
        None
    }
}

fn consume_tile_split(app: &ScratchpadApp, ctx: &egui::Context) -> Option<(SplitAxis, bool)> {
    if consume_app_shortcut(app, ctx, ShortcutAction::SplitTile) {
        Some((app.state.workspace_reflow_axis, false))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitUp) {
        Some((SplitAxis::Horizontal, true))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitDown) {
        Some((SplitAxis::Horizontal, false))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitLeft) {
        Some((SplitAxis::Vertical, true))
    } else if consume_app_shortcut(app, ctx, ShortcutAction::SplitRight) {
        Some((SplitAxis::Vertical, false))
    } else {
        None
    }
}
