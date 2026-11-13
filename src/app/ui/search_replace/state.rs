use crate::app::app_state::workspace::editing as workspace_editing;
use crate::app::app_state::{
    ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchProgress, SearchReplaceAvailability,
    SearchResultGroup, SearchScope, SearchStatus, search_runtime,
};
use crate::app::services::search::SearchMode;
use eframe::egui;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct SearchStripActions {
    pub(super) close_requested: bool,
    pub(super) next_requested: bool,
    pub(super) previous_requested: bool,
    pub(super) undo_requested: bool,
    pub(super) redo_requested: bool,
    pub(super) replace_current_requested: bool,
    pub(super) replace_all_requested: bool,
    pub(super) cancel_review: bool,
    pub(super) settings_requested: bool,
    pub(super) undo_all: Option<(u64, usize)>,
    pub(super) recent_index: Option<usize>,
    pub(super) recapture_selection: bool,
    pub(super) selected_match_index: Option<usize>,
}

pub(super) struct SearchStripState {
    pub(super) query: String,
    pub(super) replacement: String,
    pub(super) replace_open: bool,
    pub(super) scope: SearchScope,
    pub(super) mode: SearchMode,
    pub(super) match_case: bool,
    pub(super) whole_word: bool,
    pub(super) preserve_case: bool,
    pub(super) recent: Vec<crate::app::app_state::search_state::recent::RecentSearch>,
    pub(super) match_count: usize,
    pub(super) active_match_index: Option<usize>,
    pub(super) progress: SearchProgressSnapshot,
    pub(super) result_groups: Arc<[SearchResultGroup]>,
    pub(super) replace_availability: SearchReplaceAvailability,
    pub(super) can_undo_text_operation: bool,
    pub(super) can_redo_text_operation: bool,
    requested_focus: Option<SearchFocusTarget>,
    retained_focus: Option<SearchFocusTarget>,
}

pub(super) struct SearchProgressSnapshot {
    pub(super) searching: bool,
    pub(super) scanned_targets: usize,
    pub(super) target_count: usize,
    pub(super) status: SearchStatus,
    pub(super) freshness: SearchFreshness,
}

impl SearchStripState {
    pub(super) fn from_app(app: &mut ScratchpadApp) -> Self {
        let requested_focus = search_runtime::take_search_focus_target(app);
        let search_state = &app.state.search_state;
        let match_count = search_state.match_count();
        let progress = search_state.progress();

        Self {
            query: search_state.query().to_owned(),
            replacement: search_state.replacement().to_owned(),
            replace_open: search_state.replace_open(),
            scope: search_state.scope(),
            mode: search_state.mode(),
            match_case: search_state.match_case(),
            whole_word: search_state.whole_word(),
            preserve_case: search_state.query.preserve_case,
            recent: search_state.recent.clone(),
            match_count,
            active_match_index: search_state.active_match_index(),
            progress: SearchProgressSnapshot::from_progress(progress),
            result_groups: search_state.result_groups_snapshot(),
            replace_availability: search_state.replace_availability(),
            can_undo_text_operation: workspace_editing::active_buffer_can_undo_text_operation(app),
            can_redo_text_operation: workspace_editing::active_buffer_can_redo_text_operation(app),
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
            searching: matches!(progress.status, SearchStatus::Searching { .. }),
            scanned_targets: progress.scanned_targets,
            target_count: progress.target_count,
            status: progress.status,
            freshness: progress.freshness,
        }
    }
}
