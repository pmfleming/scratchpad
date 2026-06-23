use super::{AppCommand, WorkspaceCommand};
use crate::app::app_state::{
    ScratchpadApp, settings_controller,
    workspace::{accessors as workspace_accessors, display_tabs, mutation as workspace_mutation},
    workspace_controller,
};
use crate::app::domain::{BufferId, PendingAction, SplitAxis, SplitPath, TileDirection, ViewId};
use crate::app::services::file_controller::FileController;
use std::path::PathBuf;

pub(super) fn handle_workspace_command(app: &mut ScratchpadApp, command: WorkspaceCommand) -> bool {
    match command {
        WorkspaceCommand::ActivateTab { index } => activate_tab(app, index),
        WorkspaceCommand::ActivateView { view_id } => activate_view_command(app, view_id),
        WorkspaceCommand::CloseTab { index } => {
            workspace_controller::perform_close_tab(app, index);
            true
        }
        WorkspaceCommand::CloseView { view_id } => close_view_command(app, view_id),
        WorkspaceCommand::CombineTabIntoTab {
            source_index,
            target_index,
        } => {
            super::tab_transfer::combine_tab_into_tab_command(app, source_index, target_index);
            true
        }
        WorkspaceCommand::CombineTabsIntoTab {
            source_indices,
            target_index,
        } => {
            super::tab_transfer::combine_tabs_into_tab_command(app, source_indices, target_index);
            true
        }
        WorkspaceCommand::PromoteViewToTab { view_id } => {
            super::tab_transfer::promote_view_to_tab_command(app, view_id);
            true
        }
        WorkspaceCommand::PromoteTabFilesToTabs { index } => {
            super::tab_transfer::promote_tab_files_to_tabs_command(app, index);
            true
        }
        WorkspaceCommand::NewTab => {
            workspace_controller::new_tab(app);
            true
        }
        WorkspaceCommand::ReorderTab {
            from_index,
            to_index,
        } => reorder_tab_command(app, from_index, to_index),
        WorkspaceCommand::ReorderDisplayTab {
            from_index,
            to_index,
        } => reorder_display_tab_command(app, from_index, to_index),
        WorkspaceCommand::RequestCloseTab { index } => request_close_tab(app, index),
        WorkspaceCommand::ResizeSplit { path, ratio } => resize_split_command(app, path, ratio),
        WorkspaceCommand::ResizeActiveTile { direction } => {
            resize_active_tile_command(app, direction)
        }
        WorkspaceCommand::MoveActiveTile { direction } => move_active_tile_command(app, direction),
        WorkspaceCommand::SplitActiveView {
            axis,
            new_view_first,
            ratio,
        } => split_active_view_command(app, axis, new_view_first, ratio),
    }
}

fn activate_tab(app: &mut ScratchpadApp, index: usize) -> bool {
    if index >= app.tab_manager.tabs.as_slice().len() {
        return false;
    }

    app.reload_settings_before_workspace_change();
    settings_controller::activate_workspace_surface(app);
    app.tab_manager.set_active_tab_index_clamped(index);
    app.hydrate_tab_if_needed(index);
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    crate::app::app_state::search_runtime::refresh_search_view_state(app);
    workspace_accessors::request_focus_for_active_view(app);
    FileController::refresh_active_buffer_disk_state(app);
    app.tab_manager.mark_session_dirty();
    true
}

fn activate_view_command(app: &mut ScratchpadApp, view_id: ViewId) -> bool {
    app.reload_settings_if_switching_views(view_id);

    let index = app.tab_manager.active_tab_index;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.activate_view(view_id)
    {
        crate::app::app_state::search_runtime::refresh_search_view_state(app);
        workspace_accessors::request_focus_for_view(app, view_id);
        FileController::refresh_active_buffer_disk_state(app);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

fn close_view_command(app: &mut ScratchpadApp, view_id: ViewId) -> bool {
    let index = app.tab_manager.active_tab_index;
    let Some(tab) = app.tab_manager.tabs.as_slice().get(index) else {
        return false;
    };
    if tab.layout.leaf_count() <= 1 || !tab.layout.contains_view(view_id) {
        return false;
    }

    if tab
        .buffer_for_view(view_id)
        .is_some_and(|buffer| buffer.is_dirty)
        && tab.is_last_view_for_buffer(view_id) == Some(true)
    {
        crate::app::app_state::workspace::accessors::set_pending_action(
            app,
            Some(PendingAction::CloseView {
                tab_index: index,
                view_id,
            }),
        );
        return true;
    }

    perform_close_view(app, view_id);
    true
}

pub(crate) fn perform_close_view(app: &mut ScratchpadApp, view_id: ViewId) {
    app.reload_settings_if_closing_view(view_id);

    let index = app.tab_manager.active_tab_index;
    let open_buffer_ids_before = app
        .tab_manager
        .tabs
        .as_slice()
        .get(index)
        .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
        .unwrap_or_default();
    let open_file_paths_before = app
        .tab_manager
        .tabs
        .as_slice()
        .get(index)
        .map(|tab| {
            tab.buffers()
                .filter_map(|buffer| buffer.path.clone().map(|path| (buffer.id, path)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut next_active_view = None;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.close_view(view_id)
    {
        next_active_view = Some(tab.layout.active_view_id());
    }
    if let Some(next_active_view) = next_active_view {
        let open_buffer_ids_after = app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)
            .map(|tab| tab.buffers().map(|buffer| buffer.id).collect::<Vec<_>>())
            .unwrap_or_default();
        let closed_buffer_ids = removed_buffer_ids(open_buffer_ids_before, &open_buffer_ids_after);
        let closed_file_paths = removed_buffer_paths(&open_file_paths_before, &closed_buffer_ids);
        crate::app::app_state::workspace_controller::record_recently_closed_file_paths(
            app,
            closed_file_paths,
        );
        workspace_mutation::prune_text_history_for_buffers(app, closed_buffer_ids);
        app.tab_manager.rebuild_buffer_tab_index();
        crate::app::app_state::frame::begin_layout_transition(app);
        crate::app::app_state::search_runtime::mark_search_dirty(app);
        workspace_accessors::request_focus_for_view(app, next_active_view);
        app.tab_manager.mark_session_dirty();
    }
}

fn request_close_tab(app: &mut ScratchpadApp, index: usize) -> bool {
    if index < app.tab_manager.tabs.as_slice().len() {
        crate::app::app_state::workspace::accessors::set_pending_action(
            app,
            Some(PendingAction::CloseTab(index)),
        );
        return true;
    }
    false
}

fn reorder_tab_command(app: &mut ScratchpadApp, from_index: usize, to_index: usize) -> bool {
    if !app.tab_manager.reorder_tab(from_index, to_index) {
        return false;
    }
    crate::app::app_state::frame::begin_layout_transition(app);
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    true
}

fn reorder_display_tab_command(
    app: &mut ScratchpadApp,
    from_index: usize,
    to_index: usize,
) -> bool {
    if display_tabs::reorder_display_tab(app, from_index, to_index) {
        crate::app::app_state::frame::begin_layout_transition(app);
        crate::app::app_state::search_runtime::mark_search_dirty(app);
        return true;
    }
    false
}

fn resize_split_command(app: &mut ScratchpadApp, path: SplitPath, ratio: f32) -> bool {
    let index = app.tab_manager.active_tab_index;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.layout.root_pane.resize_split(&path, ratio)
    {
        crate::app::app_state::frame::begin_layout_transition(app);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

const KEYBOARD_TILE_RESIZE_STEP: f32 = 0.05;

fn resize_active_tile_command(app: &mut ScratchpadApp, direction: TileDirection) -> bool {
    let index = app.tab_manager.active_tab_index;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.resize_active_view_in_direction(direction, KEYBOARD_TILE_RESIZE_STEP)
    {
        crate::app::app_state::frame::begin_layout_transition(app);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

fn move_active_tile_command(app: &mut ScratchpadApp, direction: TileDirection) -> bool {
    let index = app.tab_manager.active_tab_index;
    let mut active_view = None;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab.move_active_view_in_direction(direction)
    {
        active_view = Some(tab.layout.active_view_id());
    }
    if let Some(active_view) = active_view {
        crate::app::app_state::frame::begin_layout_transition(app);
        crate::app::app_state::search_runtime::mark_search_dirty(app);
        workspace_accessors::request_focus_for_view(app, active_view);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

fn split_active_view_command(
    app: &mut ScratchpadApp,
    axis: SplitAxis,
    new_view_first: bool,
    ratio: f32,
) -> bool {
    let index = app.tab_manager.active_tab_index;
    let mut new_active_view = None;
    if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(index)
        && tab
            .split_active_view_with_placement(axis, new_view_first, ratio)
            .is_some()
    {
        new_active_view = Some(tab.layout.active_view_id());
    }
    if let Some(new_active_view) = new_active_view {
        crate::app::app_state::frame::begin_layout_transition(app);
        crate::app::app_state::search_runtime::mark_search_dirty(app);
        workspace_accessors::request_focus_for_view(app, new_active_view);
        app.tab_manager.mark_session_dirty();
        return true;
    }
    false
}

pub(crate) fn activate_pending_view_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    if tab_index >= app.tab_manager.tabs.as_slice().len() {
        return false;
    }

    if app.tab_manager.active_tab_index != tab_index {
        super::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateTab { index: tab_index }),
        );
    }

    let Some(tab) = app.tab_manager.tabs.as_slice().get(tab_index) else {
        return false;
    };
    if tab.layout.view(view_id).is_none() {
        return false;
    }

    if tab.layout.active_view_id() != view_id {
        super::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id }),
        );
    }

    app.tab_manager
        .tabs
        .as_slice()
        .get(tab_index)
        .is_some_and(|tab| tab.layout.active_view_id() == view_id)
}

fn removed_buffer_paths(
    open_file_paths_before: &[(BufferId, PathBuf)],
    closed_buffer_ids: &[BufferId],
) -> Vec<PathBuf> {
    open_file_paths_before
        .iter()
        .filter(|(buffer_id, _)| closed_buffer_ids.contains(buffer_id))
        .map(|(_, path)| path.clone())
        .collect()
}

fn removed_buffer_ids(before: Vec<BufferId>, after: &[BufferId]) -> Vec<BufferId> {
    before
        .into_iter()
        .filter(|buffer_id| !after.contains(buffer_id))
        .collect()
}
