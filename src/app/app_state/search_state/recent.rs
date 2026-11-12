use super::{ScratchpadApp, SearchState};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RecentSearch {
    pub query: String,
    pub regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
}
impl SearchState {
    pub(crate) fn trim_recent_searches(&mut self, limit: usize) {
        self.recent.truncate(limit.min(100));
    }
}
pub(crate) fn load(app: &mut ScratchpadApp) {
    let limit = app.state.app_settings.search.recent_searches.min(100);
    let path = app
        .state
        .persistence
        .session_store
        .root()
        .join("recent-searches.json");
    if limit == 0 {
        let _ = std::fs::remove_file(path);
        app.state.search_state.recent.clear();
        return;
    }
    if app.state.search_state.recent.is_empty()
        && let Ok(bytes) = std::fs::read(path)
        && let Ok(recent) = serde_json::from_slice::<Vec<RecentSearch>>(&bytes)
    {
        app.state.search_state.recent = recent;
    }
    app.state.search_state.trim_recent_searches(limit);
}
pub(crate) fn commit(app: &mut ScratchpadApp) {
    let state = &mut app.state.search_state;
    let limit = app.state.app_settings.search.recent_searches.min(100);
    if !state.query.query.is_empty() && limit > 0 {
        let recent = RecentSearch {
            query: state.query.query.clone(),
            regex: state.query.mode == crate::app::services::search::SearchMode::Regex,
            match_case: state.query.match_case,
            whole_word: state.query.whole_word,
        };
        state.recent.retain(|item| *item != recent);
        state.recent.insert(0, recent);
    }
    state.trim_recent_searches(limit);
    let path = app
        .state
        .persistence
        .session_store
        .root()
        .join("recent-searches.json");
    if let Ok(bytes) = serde_json::to_vec(&state.recent)
        && let Err(error) = crate::app::services::store_io::write_atomic(&path, &bytes)
    {
        log::warn!("Could not save recent searches: {error}");
    }
}
