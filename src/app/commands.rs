use crate::app::app_state::{PendingAction, ScratchpadApp};

pub enum AppCommand {
    ActivateTab { index: usize },
    CloseTab { index: usize },
    NewTab,
    OpenFile,
    RequestCloseTab { index: usize },
    SaveFile,
    SaveFileAs,
}

impl ScratchpadApp {
    pub(crate) fn handle_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::ActivateTab { index } => {
                if index < self.tabs.len() {
                    self.active_tab_index = index;
                    self.pending_scroll_to_active = true;
                    self.mark_session_dirty();
                }
            }
            AppCommand::CloseTab { index } => self.perform_close_tab(index),
            AppCommand::NewTab => self.new_tab(),
            AppCommand::OpenFile => self.open_file(),
            AppCommand::RequestCloseTab { index } => {
                if index < self.tabs.len() {
                    self.pending_action = Some(PendingAction::CloseTab(index));
                }
            }
            AppCommand::SaveFile => self.save_file(),
            AppCommand::SaveFileAs => self.save_file_as(),
        }
    }
}
