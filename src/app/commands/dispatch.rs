use super::AppCommand;
use crate::app::app_state::ScratchpadApp;

impl ScratchpadApp {
    pub(crate) fn handle_command(&mut self, command: AppCommand) -> bool {
        match command {
            AppCommand::Workspace(command) => {
                super::workspace::handle_workspace_command(self, command)
            }
            AppCommand::Search(command) => super::search::handle_search_command(self, command),
            AppCommand::File(command) => super::file::handle_file_command(self, command),
            AppCommand::Dialog(command) => super::dialogs::handle_dialog_command(self, command),
            AppCommand::Settings(command) => {
                super::settings::handle_settings_command(self, command)
            }
            AppCommand::Edit(command) => super::edit::handle_edit_command(self, command),
        }
    }
}
