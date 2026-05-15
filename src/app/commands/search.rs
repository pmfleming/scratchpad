use super::SearchCommand;
use crate::app::app_state::search_controller::{
    activate_search_match_at, close_search, focus_search_result_file_at, open_search,
    open_search_and_replace, select_next_search_match, select_previous_search_match,
    set_search_match_case, set_search_mode, set_search_query, set_search_replace_open,
    set_search_replacement, set_search_scope, set_search_whole_word, toggle_search,
};
use crate::app::app_state::{ScratchpadApp, workspace::accessors as workspace_accessors};

pub(super) fn handle_search_command(app: &mut ScratchpadApp, command: SearchCommand) -> bool {
    match command {
        SearchCommand::Open => {
            open_search(app);
            true
        }
        SearchCommand::OpenAndReplace => {
            open_search_and_replace(app);
            true
        }
        SearchCommand::Close => {
            close_search(app);
            true
        }
        SearchCommand::Toggle => {
            toggle_search(app);
            true
        }
        SearchCommand::SetSearchQuery { query } => {
            set_search_query(app, query);
            true
        }
        SearchCommand::SetSearchReplacement { replacement } => {
            set_search_replacement(app, replacement);
            true
        }
        SearchCommand::SetSearchReplaceOpen { open } => {
            set_search_replace_open(app, open);
            true
        }
        SearchCommand::SetSearchScope { scope } => {
            set_search_scope(app, scope);
            true
        }
        SearchCommand::SetSearchMode { mode } => {
            set_search_mode(app, mode);
            true
        }
        SearchCommand::SetSearchMatchCase { enabled } => {
            set_search_match_case(app, enabled);
            true
        }
        SearchCommand::SetSearchWholeWord { enabled } => {
            set_search_whole_word(app, enabled);
            true
        }
        SearchCommand::FocusSearchResultFile { match_index } => {
            focus_search_result_file(app, match_index)
        }
        SearchCommand::ActivateSearchMatch { match_index } => {
            activate_search_match(app, match_index)
        }
        SearchCommand::NextSearchMatch => select_next_search_match(app),
        SearchCommand::PreviousSearchMatch => select_previous_search_match(app),
        SearchCommand::ReplaceCurrentMatch => {
            crate::app::app_state::search_replace::replace_current_search_match(app)
        }
        SearchCommand::ReplaceAllMatches => app.replace_all_search_matches(),
    }
}

fn focus_search_result_file(app: &mut ScratchpadApp, match_index: usize) -> bool {
    if focus_search_result_file_at(app, match_index) {
        workspace_accessors::request_focus_for_active_view(app);
        return true;
    }
    false
}

fn activate_search_match(app: &mut ScratchpadApp, match_index: usize) -> bool {
    if activate_search_match_at(app, match_index) {
        workspace_accessors::request_focus_for_active_view(app);
        return true;
    }
    false
}
