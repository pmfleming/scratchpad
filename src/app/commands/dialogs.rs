use super::DialogCommand;
use crate::app::app_state::{DialogState, ScratchpadApp};

pub(crate) fn close_text_history(dialogs: &mut DialogState) {
    dialogs.text_history.close();
}

pub(super) fn handle_dialog_command(app: &mut ScratchpadApp, command: DialogCommand) -> bool {
    match command {
        DialogCommand::OpenTextHistory => {
            app.state.dialogs.text_history.open();
            true
        }
    }
}
