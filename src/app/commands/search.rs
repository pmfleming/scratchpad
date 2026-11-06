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
        SearchCommand::Open => open_search(app),
        SearchCommand::OpenAndReplace => open_search_and_replace(app),
        SearchCommand::Close => close_search(app),
        SearchCommand::Toggle => toggle_search(app),
        SearchCommand::SetSearchQuery { query } => set_search_query(app, query),
        SearchCommand::SetSearchReplacement { replacement } => {
            set_search_replacement(app, replacement);
        }
        SearchCommand::SetSearchReplaceOpen { open } => set_search_replace_open(app, open),
        SearchCommand::SetSearchScope { scope } => set_search_scope(app, scope),
        SearchCommand::SetSearchMode { mode } => set_search_mode(app, mode),
        SearchCommand::SetSearchMatchCase { enabled } => set_search_match_case(app, enabled),
        SearchCommand::SetSearchWholeWord { enabled } => set_search_whole_word(app, enabled),
        SearchCommand::FocusSearchResultFile { match_index } => {
            return focus_search_result(app, match_index, focus_search_result_file_at);
        }
        SearchCommand::ActivateSearchMatch { match_index } => {
            return focus_search_result(app, match_index, activate_search_match_at);
        }
        SearchCommand::NextSearchMatch => return select_next_search_match(app),
        SearchCommand::PreviousSearchMatch => return select_previous_search_match(app),
        SearchCommand::ReplaceCurrentMatch => {
            return crate::app::app_state::search_replace::replace_current_search_match(app);
        }
        SearchCommand::ReplaceAllMatches => return app.replace_all_search_matches(),
    }
    true
}

fn focus_search_result(
    app: &mut ScratchpadApp,
    index: usize,
    select: fn(&mut ScratchpadApp, usize) -> bool,
) -> bool {
    let changed = select(app, index);
    if changed {
        workspace_accessors::request_focus_for_active_view(app);
    }
    changed
}
