use super::DialogCommand;
use crate::app::app_state::ScratchpadApp;

pub(super) fn handle_dialog_command(app: &mut ScratchpadApp, command: DialogCommand) -> bool {
    match command {
        DialogCommand::OpenTextHistory => {
            app.state.dialogs.text_history.open();
            true
        }
    }
}
