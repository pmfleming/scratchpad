use super::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferId, ViewId};
use crate::app::services::search::{self, SearchMode, SearchOptions};
use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

mod fragments;
mod helpers;
mod replace;
mod runtime;
mod visual;
mod worker;

use helpers::selection_char_range;
use worker::{SearchRequest, SearchResult, SearchTargetSnapshot, spawn_search_worker};

const SEARCH_PREVIEW_CACHE_LIMIT: usize = 1024;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct SearchPreviewCacheKey {
    generation: u64,
    match_index: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchScope {
    SelectionOnly,
    #[default]
    ActiveBuffer,
    ActiveWorkspaceTab,
    AllOpenTabs,
}

impl SearchScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::SelectionOnly => "Selection",
            Self::ActiveBuffer => "Active File",
            Self::ActiveWorkspaceTab => "Current Tab",
            Self::AllOpenTabs => "All Open Tabs",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchScopeOrigin {
    Manual,
    SelectionDefault,
    #[default]
    ActiveContextDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchFocusTarget {
    FindInput,
    ReplaceInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchStatus {
    Idle,
    Searching {
        scanned_targets: usize,
        total_targets: usize,
    },
    Ready,
    NoMatches,
    InvalidQuery(String),
    Error(String),
}

impl SearchStatus {
    pub(crate) fn message(&self) -> Option<&str> {
        match self {
            Self::InvalidQuery(message) | Self::Error(message) => Some(message.as_str()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchFreshness {
    #[default]
    Fresh,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchReplaceAvailability {
    Allowed,
    Disabled,
    Blocked(String),
}

impl SearchReplaceAvailability {
    pub(crate) fn allows_actions(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchMatch {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) target_revision: u64,
    pub(crate) range: Range<usize>,
    pub(crate) matched_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResultEntry {
    pub(crate) match_index: usize,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) line_number: usize,
    pub(crate) column_number: usize,
    pub(crate) preview: String,
    pub(crate) active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResultGroup {
    pub(crate) tab_index: usize,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) tab_label: String,
    pub(crate) first_match_index: usize,
    pub(crate) total_match_count: usize,
    pub(crate) active: bool,
}

#[derive(Clone)]
pub(crate) struct SearchProgress {
    pub(crate) scanned_targets: usize,
    pub(crate) target_count: usize,
    pub(crate) displayed_match_count: usize,
    pub(crate) total_match_count: usize,
    pub(crate) status: SearchStatus,
    pub(crate) freshness: SearchFreshness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplacementTargetPlan {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) target_revision: u64,
    pub(crate) expected_matches: Vec<(Range<usize>, String)>,
    pub(crate) replacements: Vec<(Range<usize>, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplacementPlan {
    pub(crate) scope: SearchScope,
    pub(crate) targets: Vec<ReplacementTargetPlan>,
    pub(crate) total_match_count: usize,
}

impl ReplacementPlan {
    pub(crate) fn affected_buffer_count(&self) -> usize {
        self.targets.len()
    }

    fn requires_confirmation(&self) -> bool {
        const HIGH_REPLACE_ALL_MATCH_COUNT: usize = 100;
        self.affected_buffer_count() > 1 || self.total_match_count > HIGH_REPLACE_ALL_MATCH_COUNT
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceAllConfirmation {
    pub(crate) scope: SearchScope,
    pub(crate) affected_buffer_count: usize,
    pub(crate) total_match_count: usize,
    pub(crate) replacement: String,
    requested_generation: u64,
}

impl ReplaceAllConfirmation {
    fn from_plan(plan: &ReplacementPlan, replacement: &str, requested_generation: u64) -> Self {
        Self {
            scope: plan.scope,
            affected_buffer_count: plan.affected_buffer_count(),
            total_match_count: plan.total_match_count,
            replacement: replacement.to_owned(),
            requested_generation,
        }
    }

    fn matches_plan(
        &self,
        plan: &ReplacementPlan,
        replacement: &str,
        requested_generation: u64,
    ) -> bool {
        self.scope == plan.scope
            && self.affected_buffer_count == plan.affected_buffer_count()
            && self.total_match_count == plan.total_match_count
            && self.replacement == replacement
            && self.requested_generation == requested_generation
    }
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

impl ScratchpadApp {
    fn default_search_scope_and_origin(&self) -> (SearchScope, SearchScopeOrigin) {
        if self.active_search_selection_range().is_some() {
            (
                SearchScope::SelectionOnly,
                SearchScopeOrigin::SelectionDefault,
            )
        } else {
            (
                SearchScope::ActiveBuffer,
                SearchScopeOrigin::ActiveContextDefault,
            )
        }
    }

    fn open_search_with_focus(&mut self, focus_target: SearchFocusTarget) {
        self.activate_workspace_surface();
        let (default_scope, scope_origin) = self.default_search_scope_and_origin();
        self.search_state
            .show_with_focus(focus_target, default_scope, scope_origin);
        self.mark_search_dirty();
        self.refresh_search_state();
    }

    pub fn open_search(&mut self) {
        self.open_search_with_focus(SearchFocusTarget::FindInput);
    }

    pub fn open_search_and_replace(&mut self) {
        self.open_search_with_focus(SearchFocusTarget::ReplaceInput);
    }

    pub fn close_search(&mut self) {
        self.search_state.close();
        self.clear_search_highlights();
        self.request_focus_for_active_view();
    }

    pub fn toggle_search(&mut self) {
        if self.search_open() {
            self.close_search();
        } else {
            self.open_search();
        }
    }

    pub fn search_open(&self) -> bool {
        self.search_state.open
    }

    pub fn search_query(&self) -> &str {
        &self.search_state.query
    }

    pub fn search_replace_open(&self) -> bool {
        self.search_state.replace_open
    }

    pub fn set_search_replace_open(&mut self, open: bool) {
        self.search_state.replace_open = open;
        self.search_state.focus_target = Some(if open {
            SearchFocusTarget::ReplaceInput
        } else {
            SearchFocusTarget::FindInput
        });
        self.search_state.clear_replace_all_confirmation();
        if self.search_state.open && !self.search_state.query.is_empty() {
            self.refresh_search_visual_state();
        }
    }

    /// Common post-update side effects for parameters that change query results
    /// (query, scope, mode, match_case, whole_word).
    fn after_search_param_change(&mut self) {
        self.search_state.clear_replace_all_confirmation();
        self.mark_search_dirty();
        self.refresh_search_state();
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if self.search_state.query != query {
            self.search_state.query = query;
            self.after_search_param_change();
        }
    }

    pub fn search_replacement(&self) -> &str {
        &self.search_state.replacement
    }

    pub fn set_search_replacement(&mut self, replacement: impl Into<String>) {
        let replacement = replacement.into();
        if self.search_state.replacement != replacement {
            self.search_state.replacement = replacement;
            self.search_state.clear_replace_all_confirmation();
            if self.search_state.open && !self.search_state.query.is_empty() {
                self.refresh_search_visual_state();
            }
        }
    }

    pub fn search_scope(&self) -> SearchScope {
        self.search_state.scope
    }

    pub(crate) fn search_scope_origin(&self) -> SearchScopeOrigin {
        self.search_state.scope_origin
    }

    pub fn set_search_scope(&mut self, scope: SearchScope) {
        self.set_search_scope_with_origin(scope, SearchScopeOrigin::Manual);
    }

    pub(crate) fn set_search_scope_with_origin(
        &mut self,
        scope: SearchScope,
        origin: SearchScopeOrigin,
    ) {
        if self.search_state.scope != scope || self.search_state.scope_origin != origin {
            self.search_state.scope = scope;
            self.search_state.scope_origin = origin;
            self.after_search_param_change();
        }
    }

    pub(crate) fn search_mode(&self) -> SearchMode {
        self.search_state.mode
    }

    pub(crate) fn set_search_mode(&mut self, mode: SearchMode) {
        if self.search_state.mode != mode {
            self.search_state.mode = mode;
            self.after_search_param_change();
        }
    }

    pub fn search_match_case(&self) -> bool {
        self.search_state.match_case
    }

    pub fn set_search_match_case(&mut self, enabled: bool) {
        if self.search_state.match_case != enabled {
            self.search_state.match_case = enabled;
            self.after_search_param_change();
        }
    }

    pub fn search_whole_word(&self) -> bool {
        self.search_state.whole_word
    }

    pub fn set_search_whole_word(&mut self, enabled: bool) {
        if self.search_state.whole_word != enabled {
            self.search_state.whole_word = enabled;
            self.after_search_param_change();
        }
    }

    pub fn search_match_count(&self) -> usize {
        self.search_state.total_match_count
    }

    pub fn search_active_match_index(&self) -> Option<usize> {
        self.search_state.active_match_index
    }

    pub(crate) fn search_replace_availability(&self) -> SearchReplaceAvailability {
        if !self.search_open() || self.search_state.query.is_empty() {
            return SearchReplaceAvailability::Disabled;
        }
        if self.search_state.searching || self.search_state.freshness == SearchFreshness::Stale {
            return SearchReplaceAvailability::Disabled;
        }
        match &self.search_state.status {
            SearchStatus::InvalidQuery(message) | SearchStatus::Error(message) => {
                SearchReplaceAvailability::Blocked(message.clone())
            }
            SearchStatus::Ready if self.search_state.total_match_count > 0 => {
                SearchReplaceAvailability::Allowed
            }
            _ => SearchReplaceAvailability::Disabled,
        }
    }

    pub fn poll_search(&mut self) {
        self.refresh_search_state();
    }

    pub(crate) fn search_progress(&self) -> SearchProgress {
        SearchProgress {
            scanned_targets: match self.search_state.status {
                SearchStatus::Searching {
                    scanned_targets, ..
                } => scanned_targets,
                _ => 0,
            },
            target_count: match self.search_state.status {
                SearchStatus::Searching { total_targets, .. } => total_targets,
                _ => 0,
            },
            displayed_match_count: self.search_state.displayed_match_count,
            total_match_count: self.search_state.total_match_count,
            status: self.search_state.status.clone(),
            freshness: self.search_state.freshness,
        }
    }

    pub(crate) fn search_result_groups_snapshot(&self) -> Arc<[SearchResultGroup]> {
        self.search_state.result_groups.clone()
    }

    pub(crate) fn search_result_entry_at(
        &mut self,
        match_index: usize,
    ) -> Option<SearchResultEntry> {
        let key = SearchPreviewCacheKey {
            generation: self.search_state.applied_generation,
            match_index,
        };
        if let Some(entry) = self.search_state.preview_cache.get(&key).cloned() {
            self.touch_search_preview_cache_key(key);
            return Some(entry_with_active_state(
                entry,
                match_index,
                self.search_state.active_match_index,
            ));
        }

        let search_match = self.search_state.matches.get(match_index)?.clone();
        let (line_number, column_number, preview) = self
            .tabs()
            .get(search_match.tab_index)?
            .buffer_by_id(search_match.buffer_id)?
            .preview_for_match(&search_match.range);
        let entry = SearchResultEntry {
            match_index,
            buffer_id: search_match.buffer_id,
            buffer_label: search_match.buffer_label,
            line_number,
            column_number,
            preview,
            active: Some(match_index) == self.search_state.active_match_index,
        };
        self.store_search_preview_cache_entry(key, entry.clone());
        Some(entry)
    }

    pub(crate) fn focus_search_result_file_at(&mut self, index: usize) -> bool {
        let Some(search_match) = self.search_state.matches.get(index).cloned() else {
            return false;
        };
        self.focus_search_match(search_match)
    }

    pub(crate) fn activate_search_match_at(&mut self, index: usize) -> bool {
        self.activate_search_match(index)
    }

    fn store_search_preview_cache_entry(
        &mut self,
        key: SearchPreviewCacheKey,
        entry: SearchResultEntry,
    ) {
        self.touch_search_preview_cache_key(key.clone());
        self.search_state.preview_cache.insert(key, entry);
        while self.search_state.preview_cache_order.len() > SEARCH_PREVIEW_CACHE_LIMIT {
            let Some(expired) = self.search_state.preview_cache_order.pop_front() else {
                break;
            };
            self.search_state.preview_cache.remove(&expired);
        }
    }

    fn touch_search_preview_cache_key(&mut self, key: SearchPreviewCacheKey) {
        self.search_state
            .preview_cache_order
            .retain(|existing| existing != &key);
        self.search_state.preview_cache_order.push_back(key);
    }

    pub fn select_next_search_match(&mut self) -> bool {
        self.select_search_match_via(search::next_match_index)
    }

    pub fn select_previous_search_match(&mut self) -> bool {
        self.select_search_match_via(search::previous_match_index)
    }

    fn select_search_match_via(
        &mut self,
        pick: impl FnOnce(usize, Option<usize>) -> Option<usize>,
    ) -> bool {
        if !self.search_replace_availability().allows_actions() {
            return false;
        }
        let Some(index) = pick(
            self.search_state.matches.len(),
            self.search_state.active_match_index,
        ) else {
            return false;
        };
        self.activate_search_match(index)
    }

    fn activate_search_match(&mut self, index: usize) -> bool {
        let Some(search_match) = self.search_state.matches.get(index).cloned() else {
            return false;
        };
        if !self.focus_search_match(search_match) {
            return false;
        }

        self.set_active_search_index(Some(index));
        true
    }

    fn focus_search_match(&mut self, search_match: SearchMatch) -> bool {
        let preserve_session_clean = !self.session_dirty();

        if search_match.tab_index != self.active_tab_index() {
            self.handle_command(AppCommand::ActivateTab {
                index: search_match.tab_index,
            });
            self.pending_editor_focus = None;
        }
        let destination_slot = self.slot_for_workspace_index(search_match.tab_index);
        self.select_only_tab_slot(destination_slot);
        if self
            .active_tab()
            .is_some_and(|tab| tab.active_view_id != search_match.view_id)
        {
            self.handle_command(AppCommand::ActivateView {
                view_id: search_match.view_id,
            });
            self.pending_editor_focus = None;
        }
        if preserve_session_clean {
            self.clear_session_dirty();
        }
        true
    }

    pub fn replace_all_search_matches(&mut self) -> bool {
        if !self.search_replace_availability().allows_actions() {
            return false;
        }
        self.replace_all_search_matches_in_scope()
    }

    pub(crate) fn active_search_selection_range(&self) -> Option<Range<usize>> {
        self.active_tab()?
            .active_view()
            .and_then(|view| view.cursor_range)
            .and_then(selection_char_range)
    }
}

fn entry_with_active_state(
    mut entry: SearchResultEntry,
    match_index: usize,
    active_match_index: Option<usize>,
) -> SearchResultEntry {
    entry.active = Some(match_index) == active_match_index;
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::{BufferState, WorkspaceTab};
    use std::ops::Range;

    #[test]
    fn lazy_preview_lookup_builds_and_reuses_cached_entry() {
        let mut app = app_with_search_text("alpha\nplan beta\nomega");
        seed_matches_for_plan_lines(&mut app, &[6..10]);
        app.search_state.active_match_index = Some(0);

        let entry = app.search_result_entry_at(0).expect("preview entry");

        assert_eq!(entry.line_number, 2);
        assert_eq!(entry.column_number, 1);
        assert!(entry.preview.contains("plan beta"));
        assert!(entry.active);
        assert_eq!(app.search_state.preview_cache.len(), 1);

        app.search_state.active_match_index = None;
        let cached = app.search_result_entry_at(0).expect("cached preview entry");

        assert_eq!(cached.line_number, entry.line_number);
        assert_eq!(cached.preview, entry.preview);
        assert!(!cached.active);
        assert_eq!(app.search_state.preview_cache.len(), 1);
    }

    #[test]
    fn lazy_preview_cache_evicts_least_recently_used_entry() {
        let text = (0..=SEARCH_PREVIEW_CACHE_LIMIT)
            .map(|_| "plan")
            .collect::<Vec<_>>()
            .join("\n");
        let ranges = text
            .match_indices("plan")
            .map(|(start, value)| start..start + value.len())
            .collect::<Vec<_>>();
        let mut app = app_with_search_text(&text);
        seed_matches_for_plan_lines(&mut app, &ranges);

        for index in 0..SEARCH_PREVIEW_CACHE_LIMIT {
            assert!(app.search_result_entry_at(index).is_some());
        }
        assert_eq!(
            app.search_state.preview_cache.len(),
            SEARCH_PREVIEW_CACHE_LIMIT
        );

        assert!(app.search_result_entry_at(0).is_some());
        assert!(
            app.search_result_entry_at(SEARCH_PREVIEW_CACHE_LIMIT)
                .is_some()
        );

        let generation = app.search_state.applied_generation;
        assert!(
            app.search_state
                .preview_cache
                .contains_key(&SearchPreviewCacheKey {
                    generation,
                    match_index: 0,
                })
        );
        assert!(
            !app.search_state
                .preview_cache
                .contains_key(&SearchPreviewCacheKey {
                    generation,
                    match_index: 1,
                })
        );
        assert_eq!(
            app.search_state.preview_cache.len(),
            SEARCH_PREVIEW_CACHE_LIMIT
        );
    }

    fn app_with_search_text(text: &str) -> ScratchpadApp {
        let mut app = ScratchpadApp::default();
        let tab = WorkspaceTab::new(BufferState::new(
            "search.md".to_owned(),
            text.to_owned(),
            None,
        ));
        app.tabs_mut()[0] = tab;
        app.search_state.applied_generation = 42;
        app
    }

    fn seed_matches_for_plan_lines(app: &mut ScratchpadApp, ranges: &[Range<usize>]) {
        let tab = &app.tabs()[0];
        let buffer = &tab.buffer;
        app.search_state.matches = ranges
            .iter()
            .cloned()
            .map(|range| SearchMatch {
                tab_index: 0,
                view_id: tab.active_view_id,
                buffer_id: buffer.id,
                buffer_label: buffer.display_name(),
                target_revision: buffer.document_revision(),
                matched_text: "plan".to_owned(),
                range,
            })
            .collect();
        app.search_state.total_match_count = app.search_state.matches.len();
        app.search_state.displayed_match_count = app.search_state.matches.len();
    }
}
