use super::FileCommand;
use crate::app::app_state::{ScratchpadApp, workspace_controller};

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
        } => super::reopen_buffer_with_encoding_command(app, tab_index, &encoding_name),
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
        } => super::save_file_with_encoding_command(app, tab_index, &encoding_name),
        FileCommand::SaveConflictOverwrite { tab_index, view_id } => {
            super::save_conflict_overwrite_command(app, tab_index, view_id)
        }
        FileCommand::ReloadBufferFromDisk { tab_index, view_id } => {
            super::reload_buffer_from_disk_command(app, tab_index, view_id)
        }
        FileCommand::SaveConflictAsCopy { tab_index, view_id } => {
            super::save_conflict_as_copy_command(app, tab_index, view_id)
        }
    }
}
