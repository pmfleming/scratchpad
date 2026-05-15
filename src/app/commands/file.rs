use super::FileCommand;
use crate::app::app_state::{ScratchpadApp, workspace_controller};
use crate::app::domain::ViewId;
use crate::app::services::file_controller::FileController;

pub(super) fn handle_file_command(app: &mut ScratchpadApp, command: FileCommand) -> bool {
    match command {
        FileCommand::OpenFile => {
            workspace_controller::open_file(app);
            true
        }
        FileCommand::OpenFileHere => {
            workspace_controller::open_file_here(app);
            true
        }
        FileCommand::OpenUserManual => {
            workspace_controller::open_user_manual(app);
            true
        }
        FileCommand::ReopenBufferWithEncoding {
            tab_index,
            encoding_name,
        } => reopen_buffer_with_encoding_command(app, tab_index, &encoding_name),
        FileCommand::SaveFile => {
            workspace_controller::save_file(app);
            true
        }
        FileCommand::SaveAllFiles => {
            workspace_controller::save_all_files(app);
            true
        }
        FileCommand::SaveFileAs => {
            workspace_controller::save_file_as(app);
            true
        }
        FileCommand::SaveFileWithEncoding {
            tab_index,
            encoding_name,
        } => save_file_with_encoding_command(app, tab_index, &encoding_name),
        FileCommand::SaveConflictOverwrite { tab_index, view_id } => {
            save_conflict_overwrite_command(app, tab_index, view_id)
        }
        FileCommand::ReloadBufferFromDisk { tab_index, view_id } => {
            reload_buffer_from_disk_command(app, tab_index, view_id)
        }
        FileCommand::SaveConflictAsCopy { tab_index, view_id } => {
            save_conflict_as_copy_command(app, tab_index, view_id)
        }
    }
}

fn reopen_buffer_with_encoding_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    encoding_name: &str,
) -> bool {
    FileController::reopen_buffer_with_encoding(app, tab_index, encoding_name)
}

fn save_file_with_encoding_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    encoding_name: &str,
) -> bool {
    FileController::save_file_with_encoding_at(app, tab_index, encoding_name)
}

fn save_conflict_overwrite_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    run_save_conflict_command(
        app,
        tab_index,
        view_id,
        FileController::save_conflict_overwrite,
    )
}

fn reload_buffer_from_disk_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    run_save_conflict_command(
        app,
        tab_index,
        view_id,
        FileController::reload_buffer_from_disk,
    )
}

fn save_conflict_as_copy_command(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> bool {
    run_save_conflict_command(app, tab_index, view_id, |app, tab_index| {
        crate::app::app_state::workspace_controller::save_file_as_at(app, tab_index)
    })
}

fn run_save_conflict_command(
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
