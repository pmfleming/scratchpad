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
mod worker;

#[cfg(test)]
mod tests;

use helpers::{cursor_range_from_char_range, selection_char_range};
use worker::{SearchRequest, SearchResult, SearchTargetSnapshot, spawn_search_worker};

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
    Searching,
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
    pub(crate) range: Range<usize>,
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
    pub(crate) total_match_count: usize,
    pub(crate) entries: Vec<SearchResultEntry>,
    pub(crate) active: bool,
}

#[derive(Clone)]
pub(crate) struct SearchProgress {
    pub(crate) searching: bool,
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
    pub(crate) result_groups: Vec<SearchResultGroup>,
    pub(crate) focus_target: Option<SearchFocusTarget>,
    pub(crate) dirty: bool,
    pub(crate) requested_generation: u64,
    pub(crate) applied_generation: u64,
    pub(crate) searching: bool,
    pub(crate) status: SearchStatus,
    pub(crate) freshness: SearchFreshness,
    pub(crate) previous_active_match: Option<SearchMatch>,
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
            result_groups: Vec::new(),
            focus_target: None,
            dirty: false,
            requested_generation: 0,
            applied_generation: 0,
            searching: false,
            status: SearchStatus::Idle,
            freshness: SearchFreshness::Fresh,
            previous_active_match: None,
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
        self.result_groups.clear();
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
        self.status = SearchStatus::Searching;
        self.freshness = SearchFreshness::Stale;
        self.previous_active_match = self
            .active_match_index
            .and_then(|index| self.matches.get(index).cloned());
        // Keep old result_groups visible until new results arrive (avoids flicker).
        // Only clear the active match index so highlights don't point at stale data.
        self.active_match_index = None;
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
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if self.search_state.query != query {
            self.search_state.query = query;
            self.mark_search_dirty();
            self.refresh_search_state();
        }
    }

    pub fn search_replacement(&self) -> &str {
        &self.search_state.replacement
    }

    pub fn set_search_replacement(&mut self, replacement: impl Into<String>) {
        self.search_state.replacement = replacement.into();
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
            self.mark_search_dirty();
            self.refresh_search_state();
        }
    }

    pub(crate) fn search_mode(&self) -> SearchMode {
        self.search_state.mode
    }

    pub(crate) fn set_search_mode(&mut self, mode: SearchMode) {
        if self.search_state.mode != mode {
            self.search_state.mode = mode;
            self.mark_search_dirty();
            self.refresh_search_state();
        }
    }

    pub fn search_match_case(&self) -> bool {
        self.search_state.match_case
    }

    pub fn set_search_match_case(&mut self, enabled: bool) {
        if self.search_state.match_case != enabled {
            self.search_state.match_case = enabled;
            self.mark_search_dirty();
            self.refresh_search_state();
        }
    }

    pub fn search_whole_word(&self) -> bool {
        self.search_state.whole_word
    }

    pub fn set_search_whole_word(&mut self, enabled: bool) {
        if self.search_state.whole_word != enabled {
            self.search_state.whole_word = enabled;
            self.mark_search_dirty();
            self.refresh_search_state();
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
            searching: self.search_state.searching,
            displayed_match_count: self.search_state.displayed_match_count,
            total_match_count: self.search_state.total_match_count,
            status: self.search_state.status.clone(),
            freshness: self.search_state.freshness,
        }
    }

    pub(crate) fn search_result_groups(&self) -> &[SearchResultGroup] {
        &self.search_state.result_groups
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

    pub fn select_next_search_match(&mut self) -> bool {
        if !self.search_replace_availability().allows_actions() {
            return false;
        }
        let Some(index) = search::next_match_index(
            self.search_state.matches.len(),
            self.search_state.active_match_index,
        ) else {
            return false;
        };
        self.activate_search_match(index)
    }

    pub fn select_previous_search_match(&mut self) -> bool {
        if !self.search_replace_availability().allows_actions() {
            return false;
        }
        let Some(index) = search::previous_match_index(
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

    pub fn replace_current_search_match(&mut self) -> bool {
        if !self.search_replace_availability().allows_actions() {
            return false;
        }
        let Some(index) = self.search_state.active_match_index else {
            return false;
        };
        let Some(search_match) = self.search_state.matches.get(index).cloned() else {
            return false;
        };
        if !self.activate_search_match(index) {
            return false;
        }

        let replacement = self.search_state.replacement.clone();
        let replacement_char_count = replacement.chars().count();
        let active_buffer_id = search_match.buffer_id;
        let previous_selection = self
            .active_tab()
            .and_then(|tab| tab.view(search_match.view_id))
            .and_then(|view| view.cursor_range)
            .unwrap_or_else(|| cursor_range_from_char_range(search_match.range.clone()));
        let replacement_range =
            search_match.range.start..search_match.range.start + replacement_char_count;
        let next_selection = cursor_range_from_char_range(replacement_range.clone());

        let replacements = vec![(search_match.range.clone(), replacement)];
        if self
            .replace_ranges_in_active_buffer(
                search_match.view_id,
                active_buffer_id,
                &replacements,
                previous_selection,
                next_selection,
                "Search replace failed for the active match.",
            )
            .is_none()
        {
            return false;
        }

        self.refresh_search_state();
        self.select_next_active_buffer_match_from(replacement_range.start);
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
