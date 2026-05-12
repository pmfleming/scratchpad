use super::AppCommand;
use crate::app::app_state::search_controller::{
    close_search, open_search, open_search_and_replace, select_next_search_match,
    select_previous_search_match, set_search_match_case, set_search_mode, set_search_query,
    set_search_replace_open, set_search_replacement, set_search_scope, set_search_whole_word,
    toggle_search,
};
use crate::app::app_state::{ScratchpadApp, workspace_controller};

impl ScratchpadApp {
    pub(crate) fn handle_command(&mut self, command: AppCommand) -> bool {
        match command {
            AppCommand::ActivateTab { index } => super::activate_tab(self, index),
            AppCommand::ActivateView { view_id } => super::activate_view_command(self, view_id),
            AppCommand::CloseTab { index } => {
                workspace_controller::perform_close_tab(self, index);
                true
            }
            AppCommand::CloseView { view_id } => super::close_view_command(self, view_id),
            AppCommand::CloseSettings => {
                self.close_settings();
                true
            }
            AppCommand::CombineTabIntoTab {
                source_index,
                target_index,
            } => {
                super::tab_transfer::combine_tab_into_tab_command(self, source_index, target_index);
                true
            }
            AppCommand::CombineTabsIntoTab {
                source_indices,
                target_index,
            } => {
                super::tab_transfer::combine_tabs_into_tab_command(
                    self,
                    source_indices,
                    target_index,
                );
                true
            }
            AppCommand::PromoteViewToTab { view_id } => {
                super::tab_transfer::promote_view_to_tab_command(self, view_id);
                true
            }
            AppCommand::PromoteTabFilesToTabs { index } => {
                super::tab_transfer::promote_tab_files_to_tabs_command(self, index);
                true
            }
            AppCommand::NewTab => {
                workspace_controller::new_tab(self);
                true
            }
            AppCommand::OpenFile => {
                workspace_controller::open_file(self);
                true
            }
            AppCommand::OpenFileHere => {
                workspace_controller::open_file_here(self);
                true
            }
            AppCommand::OpenSearch => {
                open_search(self);
                true
            }
            AppCommand::OpenSearchAndReplace => {
                open_search_and_replace(self);
                true
            }
            AppCommand::OpenSettings => {
                self.open_settings();
                true
            }
            AppCommand::OpenTextHistory => {
                super::open_text_history(self);
                true
            }
            AppCommand::OpenUserManual => {
                workspace_controller::open_user_manual(self);
                true
            }
            AppCommand::CloseSearch => {
                close_search(self);
                true
            }
            AppCommand::ToggleSearch => {
                toggle_search(self);
                true
            }
            AppCommand::SetSearchQuery { query } => {
                set_search_query(self, query);
                true
            }
            AppCommand::SetSearchReplacement { replacement } => {
                set_search_replacement(self, replacement);
                true
            }
            AppCommand::SetSearchReplaceOpen { open } => {
                set_search_replace_open(self, open);
                true
            }
            AppCommand::SetSearchScope { scope } => {
                set_search_scope(self, scope);
                true
            }
            AppCommand::SetSearchMode { mode } => {
                set_search_mode(self, mode);
                true
            }
            AppCommand::SetSearchMatchCase { enabled } => {
                set_search_match_case(self, enabled);
                true
            }
            AppCommand::SetSearchWholeWord { enabled } => {
                set_search_whole_word(self, enabled);
                true
            }
            AppCommand::FocusSearchResultFile { match_index } => {
                super::focus_search_result_file_command(self, match_index)
            }
            AppCommand::ActivateSearchMatch { match_index } => {
                super::activate_search_match_command(self, match_index)
            }
            AppCommand::UndoActiveBufferTextOperation => {
                self.undo_active_buffer_text_operation();
                true
            }
            AppCommand::RedoActiveBufferTextOperation => {
                self.redo_active_buffer_text_operation();
                true
            }
            AppCommand::NextSearchMatch => select_next_search_match(self),
            AppCommand::PreviousSearchMatch => select_previous_search_match(self),
            AppCommand::ReplaceCurrentMatch => self.replace_current_search_match(),
            AppCommand::ReplaceAllMatches => self.replace_all_search_matches(),
            AppCommand::ReorderTab {
                from_index,
                to_index,
            } => super::reorder_tab_command(self, from_index, to_index),
            AppCommand::ReorderDisplayTab {
                from_index,
                to_index,
            } => super::reorder_display_tab_command(self, from_index, to_index),
            AppCommand::RequestCloseTab { index } => super::request_close_tab(self, index),
            AppCommand::ResizeSplit { path, ratio } => {
                super::resize_split_command(self, path, ratio)
            }
            AppCommand::ReopenBufferWithEncoding {
                tab_index,
                encoding_name,
            } => super::reopen_buffer_with_encoding_command(self, tab_index, &encoding_name),
            AppCommand::SaveFile => {
                workspace_controller::save_file(self);
                true
            }
            AppCommand::SaveAllFiles => {
                workspace_controller::save_all_files(self);
                true
            }
            AppCommand::SaveFileAs => {
                workspace_controller::save_file_as(self);
                true
            }
            AppCommand::SaveFileWithEncoding {
                tab_index,
                encoding_name,
            } => super::save_file_with_encoding_command(self, tab_index, &encoding_name),
            AppCommand::SaveConflictOverwrite { tab_index, view_id } => {
                super::save_conflict_overwrite_command(self, tab_index, view_id)
            }
            AppCommand::ReloadBufferFromDisk { tab_index, view_id } => {
                super::reload_buffer_from_disk_command(self, tab_index, view_id)
            }
            AppCommand::SaveConflictAsCopy { tab_index, view_id } => {
                super::save_conflict_as_copy_command(self, tab_index, view_id)
            }
            AppCommand::SplitActiveView {
                axis,
                new_view_first,
                ratio,
            } => super::split_active_view_command(self, axis, new_view_first, ratio),
        }
    }
}
