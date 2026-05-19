use super::AppCommand;
use crate::app::app_state::ScratchpadApp;

pub(crate) fn handle_command(app: &mut ScratchpadApp, command: AppCommand) -> bool {
    match command {
        AppCommand::Workspace(command) => super::workspace::handle_workspace_command(app, command),
        AppCommand::Search(command) => super::search::handle_search_command(app, command),
        AppCommand::File(command) => super::file::handle_file_command(app, command),
        AppCommand::Dialog(command) => super::dialogs::handle_dialog_command(app, command),
        AppCommand::Settings(command) => super::settings::handle_settings_command(app, command),
        AppCommand::Edit(command) => super::edit::handle_edit_command(app, command),
    }
}
