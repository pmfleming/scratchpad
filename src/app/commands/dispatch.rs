use super::AppCommand;
use crate::app::app_state::ScratchpadApp;

impl ScratchpadApp {
    pub(crate) fn handle_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::ActivateTab { index } => self.activate_tab(index),
            AppCommand::ActivateView { view_id } => self.activate_view_command(view_id),
            AppCommand::CloseTab { index } => self.perform_close_tab(index),
            AppCommand::CloseView { view_id } => self.close_view_command(view_id),
            AppCommand::CloseSettings => self.close_settings(),
            AppCommand::CombineTabIntoTab {
                source_index,
                target_index,
            } => self.combine_tab_into_tab_command(source_index, target_index),
            AppCommand::CombineTabsIntoTab {
                source_indices,
                target_index,
            } => self.combine_tabs_into_tab_command(source_indices, target_index),
            AppCommand::PromoteViewToTab { view_id } => self.promote_view_to_tab_command(view_id),
            AppCommand::PromoteTabFilesToTabs { index } => {
                self.promote_tab_files_to_tabs_command(index)
            }
            AppCommand::NewTab => self.new_tab(),
            AppCommand::OpenFile => self.open_file(),
            AppCommand::OpenFileHere => self.open_file_here(),
            AppCommand::OpenHistory => self.open_transaction_log(),
            AppCommand::OpenSearch => self.open_search(),
            AppCommand::OpenSearchAndReplace => self.open_search_and_replace(),
            AppCommand::OpenSettings => self.open_settings(),
            AppCommand::OpenUserManual => self.open_user_manual(),
            AppCommand::CloseSearch => self.close_search(),
            AppCommand::UndoActiveBufferTextOperation => {
                self.undo_active_buffer_text_operation();
            }
            AppCommand::RedoActiveBufferTextOperation => {
                self.redo_active_buffer_text_operation();
            }
            AppCommand::NextSearchMatch => {
                self.select_next_search_match();
            }
            AppCommand::PreviousSearchMatch => {
                self.select_previous_search_match();
            }
            AppCommand::ReplaceCurrentMatch => {
                self.replace_current_search_match();
            }
            AppCommand::ReplaceAllMatches => {
                self.replace_all_search_matches();
            }
            AppCommand::ReorderTab {
                from_index,
                to_index,
            } => self.reorder_tab_command(from_index, to_index),
            AppCommand::ReorderDisplayTab {
                from_index,
                to_index,
            } => self.reorder_display_tab_command(from_index, to_index),
            AppCommand::RequestCloseTab { index } => self.request_close_tab(index),
            AppCommand::ResizeSplit { path, ratio } => self.resize_split_command(path, ratio),
            AppCommand::SaveFile => self.save_file(),
            AppCommand::SaveFileAs => self.save_file_as(),
            AppCommand::SplitActiveView {
                axis,
                new_view_first,
                ratio,
            } => self.split_active_view_command(axis, new_view_first, ratio),
        }
    }
}
