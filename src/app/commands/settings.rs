use super::SettingsCommand;
use crate::app::app_state::{ScratchpadApp, settings_controller};

pub(super) fn handle_settings_command(app: &mut ScratchpadApp, command: SettingsCommand) -> bool {
    match command {
        SettingsCommand::OpenSettings => settings_controller::open_settings(app),
        SettingsCommand::CloseSettings => settings_controller::close_settings(app),
    }
    true
}
