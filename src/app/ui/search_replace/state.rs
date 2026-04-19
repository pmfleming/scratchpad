use crate::app::app_state::{
    ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchProgress, SearchReplaceAvailability,
    SearchResultGroup, SearchScope, SearchScopeOrigin, SearchStatus,
};
use crate::app::services::search::SearchMode;
use eframe::egui;

#[derive(Default)]
pub(super) struct SearchStripActions {
    pub(super) close_requested: bool,
    pub(super) next_requested: bool,
    pub(super) previous_requested: bool,
    pub(super) replace_current_requested: bool,
    pub(super) replace_all_requested: bool,
    pub(super) selected_match_index: Option<usize>,
}

pub(super) struct SearchStripState {
    pub(super) query: String,
    pub(super) replacement: String,
    pub(super) replace_open: bool,
    pub(super) scope: SearchScope,
    pub(super) scope_origin: SearchScopeOrigin,
    pub(super) mode: SearchMode,
    pub(super) match_case: bool,
    pub(super) whole_word: bool,
    pub(super) match_count: usize,
    pub(super) progress: SearchProgressSnapshot,
    pub(super) result_groups: Vec<SearchResultGroup>,
    pub(super) replace_availability: SearchReplaceAvailability,
    requested_focus: Option<SearchFocusTarget>,
    retained_focus: Option<SearchFocusTarget>,
}

pub(super) struct SearchProgressSnapshot {
    pub(super) searching: bool,
    pub(super) displayed_match_count: usize,
    pub(super) total_match_count: usize,
    pub(super) status: SearchStatus,
    pub(super) freshness: SearchFreshness,
}

impl SearchStripState {
    pub(super) fn from_app(app: &mut ScratchpadApp) -> Self {
        let match_count = app.search_match_count();
        let progress = app.search_progress();
        let requested_focus = app.take_search_focus_target();

        Self {
            query: app.search_query().to_owned(),
            replacement: app.search_replacement().to_owned(),
            replace_open: app.search_replace_open(),
            scope: app.search_scope(),
            scope_origin: app.search_scope_origin(),
            mode: app.search_mode(),
            match_case: app.search_match_case(),
            whole_word: app.search_whole_word(),
            match_count,
            progress: SearchProgressSnapshot::from_progress(progress),
            result_groups: app.search_result_groups().to_vec(),
            replace_availability: app.search_replace_availability(),
            requested_focus,
            retained_focus: requested_focus,
        }
    }

    pub(super) fn sync_focus(
        &mut self,
        response: &egui::Response,
        focus_target: SearchFocusTarget,
    ) {
        if self.requested_focus == Some(focus_target) {
            response.request_focus();
            self.retained_focus = Some(focus_target);
        } else if response.has_focus() {
            self.retained_focus = Some(focus_target);
        }
    }

    pub(super) fn target_focus(&self) -> SearchFocusTarget {
        self.retained_focus.unwrap_or(SearchFocusTarget::FindInput)
    }
}

impl SearchProgressSnapshot {
    fn from_progress(progress: SearchProgress) -> Self {
        Self {
            searching: progress.searching,
            displayed_match_count: progress.displayed_match_count,
            total_match_count: progress.total_match_count,
            status: progress.status,
            freshness: progress.freshness,
        }
    }
}
