use super::EditCommand;
use crate::app::app_state::{ScratchpadApp, workspace::editing as workspace_editing};

pub(super) fn handle_edit_command(app: &mut ScratchpadApp, command: EditCommand) -> bool {
    match command {
        EditCommand::UndoActiveBufferTextOperation => {
            workspace_editing::undo_active_buffer_text_operation(app);
            true
        }
        EditCommand::RedoActiveBufferTextOperation => {
            workspace_editing::redo_active_buffer_text_operation(app);
            true
        }
    }
}
