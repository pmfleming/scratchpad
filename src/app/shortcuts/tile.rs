use super::{consume_app_shortcut, first_consumed};
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

    if let Some(direction) = first_consumed(app, ctx, &TILE_MOVES) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::MoveActiveTile { direction }),
        );
        return;
    }

    if let Some(direction) = first_consumed(app, ctx, &TILE_RESIZES) {
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

const TILE_MOVES: [(ShortcutAction, TileDirection); 4] = [
    (ShortcutAction::MoveTileLeft, TileDirection::Left),
    (ShortcutAction::MoveTileRight, TileDirection::Right),
    (ShortcutAction::MoveTileUp, TileDirection::Up),
    (ShortcutAction::MoveTileDown, TileDirection::Down),
];

const TILE_RESIZES: [(ShortcutAction, TileDirection); 4] = [
    (ShortcutAction::ResizeTileLeft, TileDirection::Left),
    (ShortcutAction::ResizeTileRight, TileDirection::Right),
    (ShortcutAction::ResizeTileUp, TileDirection::Up),
    (ShortcutAction::ResizeTileDown, TileDirection::Down),
];

/// Returns the split axis and whether the new view goes first.
fn consume_tile_split(app: &ScratchpadApp, ctx: &egui::Context) -> Option<(SplitAxis, bool)> {
    first_consumed(
        app,
        ctx,
        &[
            (
                ShortcutAction::SplitTile,
                (app.state.workspace_reflow_axis, false),
            ),
            (ShortcutAction::SplitUp, (SplitAxis::Horizontal, true)),
            (ShortcutAction::SplitDown, (SplitAxis::Horizontal, false)),
            (ShortcutAction::SplitLeft, (SplitAxis::Vertical, true)),
            (ShortcutAction::SplitRight, (SplitAxis::Vertical, false)),
        ],
    )
}
