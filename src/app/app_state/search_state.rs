use super::ScratchpadApp;
use crate::app::services::search::{SearchMode, SearchOptions};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

mod api;
#[cfg(test)]
mod api_tests;
mod fragments;
mod helpers;
mod replace;
mod runtime;
mod types;
mod visual;
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
    pub(crate) open: bool,
    pub(crate) replace_open: bool,
    pub(crate) query: String,
    pub(crate) replacement: String,
    pub(crate) scope: SearchScope,
    pub(crate) scope_origin: SearchScopeOrigin,
    pub(crate) mode: SearchMode,
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
    pub(crate) active_match_index: Option<usize>,
    pub(crate) matches: Vec<SearchMatch>,
    pub(crate) total_match_count: usize,
    pub(crate) displayed_match_count: usize,
    pub(crate) result_groups: Arc<[SearchResultGroup]>,
    pub(crate) focus_target: Option<SearchFocusTarget>,
    pub(crate) dirty: bool,
    pub(crate) requested_generation: u64,
    pub(crate) applied_generation: u64,
    pub(crate) searching: bool,
    pub(crate) status: SearchStatus,
    pub(crate) freshness: SearchFreshness,
    pub(crate) previous_active_match: Option<SearchMatch>,
    pub(crate) pending_replace_all_confirmation: Option<ReplaceAllConfirmation>,
    preview_cache: std::collections::HashMap<SearchPreviewCacheKey, SearchResultEntry>,
    preview_cache_order: std::collections::VecDeque<SearchPreviewCacheKey>,
    latest_generation: Arc<AtomicU64>,
    request_tx: Sender<SearchRequest>,
    result_rx: Receiver<SearchResult>,
}

impl Default for SearchState {
    fn default() -> Self {
        let latest_generation = Arc::new(AtomicU64::new(0));
        let (request_tx, result_rx) = spawn_search_worker(latest_generation.clone());
        Self {
            open: false,
            replace_open: false,
            query: String::new(),
            replacement: String::new(),
            scope: SearchScope::ActiveBuffer,
            scope_origin: SearchScopeOrigin::ActiveContextDefault,
            mode: SearchMode::PlainText,
            match_case: false,
            whole_word: false,
            active_match_index: None,
            matches: Vec::new(),
            total_match_count: 0,
            displayed_match_count: 0,
            result_groups: Arc::from(Vec::<SearchResultGroup>::new()),
            focus_target: None,
            dirty: false,
            requested_generation: 0,
            applied_generation: 0,
            searching: false,
            status: SearchStatus::Idle,
            freshness: SearchFreshness::Fresh,
            previous_active_match: None,
            pending_replace_all_confirmation: None,
            preview_cache: std::collections::HashMap::new(),
            preview_cache_order: std::collections::VecDeque::new(),
            latest_generation,
            request_tx,
            result_rx,
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
        self.open = true;
        self.replace_open = matches!(focus_target, SearchFocusTarget::ReplaceInput);
        self.focus_target = Some(focus_target);
        if self.query.is_empty() {
            self.scope = default_scope;
            self.scope_origin = scope_origin;
        }
    }

    fn close(&mut self) {
        self.open = false;
        self.replace_open = false;
        self.focus_target = None;
        self.clear_inactive_results();
        self.status = SearchStatus::Idle;
        self.freshness = SearchFreshness::Fresh;
    }

    fn clear_match_results(&mut self) {
        self.active_match_index = None;
        self.matches.clear();
        self.total_match_count = 0;
        self.displayed_match_count = 0;
        self.result_groups = Arc::from(Vec::<SearchResultGroup>::new());
        self.preview_cache.clear();
        self.preview_cache_order.clear();
    }

    fn clear_replace_all_confirmation(&mut self) {
        self.pending_replace_all_confirmation = None;
    }

    fn clear_inactive_results(&mut self) {
        self.clear_match_results();
        self.dirty = false;
        self.searching = false;
        self.previous_active_match = None;
        self.applied_generation = 0;
    }

    fn begin_request(&mut self, generation: u64) {
        self.requested_generation = generation;
        self.latest_generation.store(generation, Ordering::Relaxed);
        self.searching = true;
        self.status = SearchStatus::Searching {
            scanned_targets: 0,
            total_targets: 0,
        };
        self.freshness = SearchFreshness::Stale;
        self.previous_active_match = self
            .active_match_index
            .and_then(|index| self.matches.get(index).cloned());
        self.clear_match_results();
        self.clear_replace_all_confirmation();
    }

    fn search_options(&self) -> SearchOptions {
        SearchOptions {
            mode: self.mode,
            match_case: self.match_case,
            whole_word: self.whole_word,
        }
    }

    fn build_request(&self, generation: u64, targets: Vec<SearchTargetSnapshot>) -> SearchRequest {
        SearchRequest {
            generation,
            query: self.query.clone(),
            options: self.search_options(),
            targets,
        }
    }
}
