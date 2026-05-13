use super::ScratchpadApp;
use crate::app::services::search::{SearchMode, SearchOptions};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub(crate) mod api;
#[cfg(test)]
mod api_tests;
mod fragments;
mod helpers;
pub(crate) mod replace;
pub(crate) mod runtime;
mod types;
pub(crate) mod visual;
mod worker;

pub use types::SearchScope;
pub(crate) use types::{
    ReplaceAllConfirmation, ReplacementPlan, ReplacementTargetPlan, SearchFocusTarget,
    SearchFreshness, SearchMatch, SearchProgress, SearchReplaceAvailability, SearchResultEntry,
    SearchResultGroup, SearchScopeOrigin, SearchStatus,
};
use worker::{SearchRequest, SearchResult, SearchTargetSnapshot, spawn_search_worker};

const SEARCH_PREVIEW_CACHE_LIMIT: usize = 1024;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct SearchPreviewCacheKey {
    generation: u64,
    match_index: usize,
}

pub(crate) struct SearchState {
    pub(crate) panel: SearchPanelState,
    pub(crate) query: SearchQueryState,
    pub(crate) results: SearchResultsState,
    pub(crate) runtime: SearchRuntimeState,
    pub(crate) replace: SearchReplaceState,
    preview: SearchPreviewCacheState,
}

pub(crate) struct SearchPanelState {
    pub(crate) open: bool,
    pub(crate) replace_open: bool,
    pub(crate) focus_target: Option<SearchFocusTarget>,
}

impl Default for SearchPanelState {
    fn default() -> Self {
        Self {
            open: false,
            replace_open: false,
            focus_target: None,
        }
    }
}

pub(crate) struct SearchQueryState {
    pub(crate) query: String,
    pub(crate) replacement: String,
    pub(crate) scope: SearchScope,
    pub(crate) scope_origin: SearchScopeOrigin,
    pub(crate) mode: SearchMode,
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
}

impl Default for SearchQueryState {
    fn default() -> Self {
        Self {
            query: String::new(),
            replacement: String::new(),
            scope: SearchScope::ActiveBuffer,
            scope_origin: SearchScopeOrigin::ActiveContextDefault,
            mode: SearchMode::PlainText,
            match_case: false,
            whole_word: false,
        }
    }
}

pub(crate) struct SearchResultsState {
    pub(crate) active_match_index: Option<usize>,
    pub(crate) matches: Vec<SearchMatch>,
    pub(crate) total_match_count: usize,
    pub(crate) displayed_match_count: usize,
    pub(crate) result_groups: Arc<[SearchResultGroup]>,
    pub(crate) previous_active_match: Option<SearchMatch>,
}

impl Default for SearchResultsState {
    fn default() -> Self {
        Self {
            active_match_index: None,
            matches: Vec::new(),
            total_match_count: 0,
            displayed_match_count: 0,
            result_groups: Arc::from(Vec::<SearchResultGroup>::new()),
            previous_active_match: None,
        }
    }
}

pub(crate) struct SearchRuntimeState {
    pub(crate) dirty: bool,
    pub(crate) requested_generation: u64,
    pub(crate) applied_generation: u64,
    pub(crate) searching: bool,
    pub(crate) status: SearchStatus,
    pub(crate) freshness: SearchFreshness,
    latest_generation: Arc<AtomicU64>,
    request_tx: Sender<SearchRequest>,
    result_rx: Receiver<SearchResult>,
}

impl Default for SearchRuntimeState {
    fn default() -> Self {
        let latest_generation = Arc::new(AtomicU64::new(0));
        let (request_tx, result_rx) = spawn_search_worker(latest_generation.clone());
        Self {
            dirty: false,
            requested_generation: 0,
            applied_generation: 0,
            searching: false,
            status: SearchStatus::Idle,
            freshness: SearchFreshness::Fresh,
            latest_generation,
            request_tx,
            result_rx,
        }
    }
}

#[derive(Default)]
pub(crate) struct SearchReplaceState {
    pub(crate) pending_replace_all_confirmation: Option<ReplaceAllConfirmation>,
}

#[derive(Default)]
struct SearchPreviewCacheState {
    entries: std::collections::HashMap<SearchPreviewCacheKey, SearchResultEntry>,
    order: std::collections::VecDeque<SearchPreviewCacheKey>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            panel: SearchPanelState::default(),
            query: SearchQueryState::default(),
            results: SearchResultsState::default(),
            runtime: SearchRuntimeState::default(),
            replace: SearchReplaceState::default(),
            preview: SearchPreviewCacheState::default(),
        }
    }
}

impl SearchState {
    fn show_with_focus(
        &mut self,
        focus_target: SearchFocusTarget,
        default_scope: SearchScope,
        scope_origin: SearchScopeOrigin,
    ) {
        self.panel.open = true;
        self.panel.replace_open = matches!(focus_target, SearchFocusTarget::ReplaceInput);
        self.panel.focus_target = Some(focus_target);
        if self.query.query.is_empty() {
            self.query.scope = default_scope;
            self.query.scope_origin = scope_origin;
        }
    }

    pub(crate) fn open(&self) -> bool {
        self.panel.open
    }

    pub(crate) fn query(&self) -> &str {
        &self.query.query
    }

    pub(crate) fn replacement(&self) -> &str {
        &self.query.replacement
    }

    pub(crate) fn replace_open(&self) -> bool {
        self.panel.replace_open
    }

    fn set_replace_open(&mut self, open: bool) {
        self.panel.replace_open = open;
        self.panel.focus_target = Some(if open {
            SearchFocusTarget::ReplaceInput
        } else {
            SearchFocusTarget::FindInput
        });
        self.clear_replace_all_confirmation();
    }

    fn set_query(&mut self, query: impl Into<String>) -> bool {
        let query = query.into();
        if self.query.query == query {
            return false;
        }
        self.query.query = query;
        self.clear_replace_all_confirmation();
        true
    }

    fn set_replacement(&mut self, replacement: impl Into<String>) -> bool {
        let replacement = replacement.into();
        if self.query.replacement == replacement {
            return false;
        }
        self.query.replacement = replacement;
        self.clear_replace_all_confirmation();
        true
    }

    pub(crate) fn scope(&self) -> SearchScope {
        self.query.scope
    }

    pub(crate) fn scope_origin(&self) -> SearchScopeOrigin {
        self.query.scope_origin
    }

    pub(crate) fn mode(&self) -> SearchMode {
        self.query.mode
    }

    pub(crate) fn match_case(&self) -> bool {
        self.query.match_case
    }

    pub(crate) fn whole_word(&self) -> bool {
        self.query.whole_word
    }

    pub(crate) fn match_count(&self) -> usize {
        self.results.total_match_count
    }

    pub(crate) fn active_match_index(&self) -> Option<usize> {
        self.results.active_match_index
    }

    pub(crate) fn result_groups_snapshot(&self) -> Arc<[SearchResultGroup]> {
        self.results.result_groups.clone()
    }

    pub(crate) fn replace_availability(&self) -> SearchReplaceAvailability {
        if !self.panel.open || self.query.query.is_empty() {
            return SearchReplaceAvailability::Disabled;
        }
        if self.runtime.searching || self.runtime.freshness == SearchFreshness::Stale {
            return SearchReplaceAvailability::Disabled;
        }
        match &self.runtime.status {
            SearchStatus::InvalidQuery(message) | SearchStatus::Error(message) => {
                SearchReplaceAvailability::Blocked(message.clone())
            }
            SearchStatus::Ready if self.results.total_match_count > 0 => {
                SearchReplaceAvailability::Allowed
            }
            _ => SearchReplaceAvailability::Disabled,
        }
    }

    pub(crate) fn progress(&self) -> SearchProgress {
        SearchProgress {
            scanned_targets: match self.runtime.status {
                SearchStatus::Searching {
                    scanned_targets, ..
                } => scanned_targets,
                _ => 0,
            },
            target_count: match self.runtime.status {
                SearchStatus::Searching { total_targets, .. } => total_targets,
                _ => 0,
            },
            displayed_match_count: self.results.displayed_match_count,
            total_match_count: self.results.total_match_count,
            status: self.runtime.status.clone(),
            freshness: self.runtime.freshness,
        }
    }

    fn set_scope_with_origin(&mut self, scope: SearchScope, origin: SearchScopeOrigin) -> bool {
        if self.query.scope == scope && self.query.scope_origin == origin {
            return false;
        }
        self.query.scope = scope;
        self.query.scope_origin = origin;
        self.clear_replace_all_confirmation();
        true
    }

    fn set_mode(&mut self, mode: SearchMode) -> bool {
        if self.query.mode == mode {
            return false;
        }
        self.query.mode = mode;
        self.clear_replace_all_confirmation();
        true
    }

    fn set_match_case(&mut self, enabled: bool) -> bool {
        if self.query.match_case == enabled {
            return false;
        }
        self.query.match_case = enabled;
        self.clear_replace_all_confirmation();
        true
    }

    fn set_whole_word(&mut self, enabled: bool) -> bool {
        if self.query.whole_word == enabled {
            return false;
        }
        self.query.whole_word = enabled;
        self.clear_replace_all_confirmation();
        true
    }

    fn close(&mut self) {
        self.panel.open = false;
        self.panel.replace_open = false;
        self.panel.focus_target = None;
        self.clear_inactive_results();
        self.runtime.status = SearchStatus::Idle;
        self.runtime.freshness = SearchFreshness::Fresh;
    }

    fn clear_match_results(&mut self) {
        self.results.active_match_index = None;
        self.results.matches.clear();
        self.results.total_match_count = 0;
        self.results.displayed_match_count = 0;
        self.results.result_groups = Arc::from(Vec::<SearchResultGroup>::new());
        self.preview.entries.clear();
        self.preview.order.clear();
    }

    fn clear_replace_all_confirmation(&mut self) {
        self.replace.pending_replace_all_confirmation = None;
    }

    fn clear_inactive_results(&mut self) {
        self.clear_match_results();
        self.runtime.dirty = false;
        self.runtime.searching = false;
        self.results.previous_active_match = None;
        self.runtime.applied_generation = 0;
    }

    fn begin_request(&mut self, generation: u64) {
        self.runtime.requested_generation = generation;
        self.runtime
            .latest_generation
            .store(generation, Ordering::Relaxed);
        self.runtime.searching = true;
        self.runtime.status = SearchStatus::Searching {
            scanned_targets: 0,
            total_targets: 0,
        };
        self.runtime.freshness = SearchFreshness::Stale;
        self.results.previous_active_match = self
            .results
            .active_match_index
            .and_then(|index| self.results.matches.get(index).cloned());
        self.clear_match_results();
        self.clear_replace_all_confirmation();
    }

    fn search_options(&self) -> SearchOptions {
        SearchOptions {
            mode: self.query.mode,
            match_case: self.query.match_case,
            whole_word: self.query.whole_word,
        }
    }

    fn build_request(&self, generation: u64, targets: Vec<SearchTargetSnapshot>) -> SearchRequest {
        SearchRequest {
            generation,
            query: self.query.query.clone(),
            options: self.search_options(),
            targets,
        }
    }
}
