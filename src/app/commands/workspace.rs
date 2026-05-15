use super::WorkspaceCommand;
use crate::app::app_state::{ScratchpadApp, workspace_controller};

pub(super) fn handle_workspace_command(app: &mut ScratchpadApp, command: WorkspaceCommand) -> bool {
    match command {
        WorkspaceCommand::ActivateTab { index } => super::activate_tab(app, index),
        WorkspaceCommand::ActivateView { view_id } => super::activate_view_command(app, view_id),
        WorkspaceCommand::CloseTab { index } => {
            workspace_controller::perform_close_tab(app, index);
            true
        }
        WorkspaceCommand::CloseView { view_id } => super::close_view_command(app, view_id),
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
        } => super::reorder_tab_command(app, from_index, to_index),
        WorkspaceCommand::ReorderDisplayTab {
            from_index,
            to_index,
        } => super::reorder_display_tab_command(app, from_index, to_index),
        WorkspaceCommand::RequestCloseTab { index } => super::request_close_tab(app, index),
        WorkspaceCommand::ResizeSplit { path, ratio } => {
            super::resize_split_command(app, path, ratio)
        }
        WorkspaceCommand::SplitActiveView {
            axis,
            new_view_first,
            ratio,
        } => super::split_active_view_command(app, axis, new_view_first, ratio),
    }
}
