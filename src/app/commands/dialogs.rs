use super::DialogCommand;
use crate::app::app_state::ScratchpadApp;

pub(super) fn handle_dialog_command(app: &mut ScratchpadApp, command: DialogCommand) -> bool {
    match command {
        DialogCommand::OpenTextHistory => {
            super::open_text_history(&mut app.state.dialogs);
            true
        }
    }
}
