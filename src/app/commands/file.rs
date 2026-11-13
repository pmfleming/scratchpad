use super::FileCommand;
use crate::app::app_state::{ScratchpadApp, workspace_controller};
use crate::app::domain::ViewId;
use crate::app::services::file_controller::FileController;

pub(super) fn handle_file_command(app: &mut ScratchpadApp, command: FileCommand) -> bool {
    match command {
        FileCommand::OpenFile => workspace_controller::open_file(app),
        FileCommand::OpenFileHere => workspace_controller::open_file_here(app),
        FileCommand::OpenUserManual => workspace_controller::open_user_manual(app),
        FileCommand::SaveFile => workspace_controller::save_file(app),
        FileCommand::SaveAllFiles => workspace_controller::save_all_files(app),
        FileCommand::SaveFileAs => workspace_controller::save_file_as(app),
        FileCommand::ReopenBufferWithEncoding {
            tab_index,
            encoding_name,
        } => return FileController::reopen_buffer_with_encoding(app, tab_index, &encoding_name),
        FileCommand::SaveFileWithEncoding {
            tab_index,
            encoding_name,
            bom,
        } => {
            return FileController::save_file_with_encoding_at(app, tab_index, &encoding_name, bom);
        }
        FileCommand::SaveConflictOverwrite { tab_index, view_id } => {
            return run_pending_view_action(
                app,
                tab_index,
                view_id,
                FileController::save_conflict_overwrite,
            );
        }
        FileCommand::ReloadBufferFromDisk { tab_index, view_id } => {
            return run_pending_view_action(
                app,
                tab_index,
                view_id,
                FileController::reload_buffer_from_disk,
            );
        }
        FileCommand::SaveConflictAsCopy { tab_index, view_id } => {
            return run_pending_view_action(
                app,
                tab_index,
                view_id,
                FileController::save_conflict_as_copy,
            );
        }
        FileCommand::SaveMissingAs { tab_index, view_id } => {
            return run_pending_view_action(
                app,
                tab_index,
                view_id,
                FileController::save_file_as_at,
            );
        }
    }
    true
}

/// Activate the view a pending file prompt belongs to, run its resolution and
/// clear the prompt when the action succeeds.
fn run_pending_view_action(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    action: impl FnOnce(&mut ScratchpadApp, usize) -> bool,
) -> bool {
    if !super::activate_pending_view_command(app, tab_index, view_id) || !action(app, tab_index) {
        return false;
    }
    crate::app::app_state::workspace::accessors::set_pending_action(app, None);
    true
}
