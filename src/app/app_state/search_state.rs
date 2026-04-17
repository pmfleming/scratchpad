use super::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferId, ViewId};
use crate::app::services::search::{self, SearchOptions};
use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

mod helpers;
mod runtime;
mod worker;

#[cfg(test)]
mod tests;

use helpers::cursor_range_from_char_range;
use worker::{SearchRequest, SearchResult, SearchTargetSnapshot, spawn_search_worker};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchScope {
    #[default]
    ActiveBuffer,
    ActiveWorkspaceTab,
    AllOpenTabs,
}

impl SearchScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::ActiveBuffer => "Active File",
            Self::ActiveWorkspaceTab => "Current Tab",
            Self::AllOpenTabs => "All Open Tabs",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchFocusTarget {
    FindInput,
    ReplaceInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchMatch {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_id: BufferId,
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
    pub(crate) tab_label: String,
    pub(crate) entries: Vec<SearchResultEntry>,
}

pub(crate) struct SearchProgress {
    pub(crate) searching: bool,
    pub(crate) displayed_match_count: usize,
    pub(crate) total_match_count: usize,
}

pub(crate) struct SearchState {
    pub(crate) open: bool,
    pub(crate) replace_open: bool,
    pub(crate) query: String,
    pub(crate) replacement: String,
    pub(crate) scope: SearchScope,
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
    pub(crate) searching: bool,
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
            searching: false,
            previous_active_match: None,
            latest_generation,
            request_tx,
            result_rx,
        }
    }
}

impl SearchState {
    fn show_with_focus(&mut self, focus_target: SearchFocusTarget) {
        self.open = true;
        self.replace_open = matches!(focus_target, SearchFocusTarget::ReplaceInput);
        self.focus_target = Some(focus_target);
    }

    fn close(&mut self) {
        self.open = false;
        self.replace_open = false;
        self.focus_target = None;
        self.clear_inactive_results();
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
    }

    fn begin_request(&mut self, generation: u64) {
        self.requested_generation = generation;
        self.latest_generation.store(generation, Ordering::Relaxed);
        self.searching = true;
        self.previous_active_match = self
            .active_match_index
            .and_then(|index| self.matches.get(index).cloned());
        self.clear_match_results();
    }

    fn search_options(&self) -> SearchOptions {
        SearchOptions {
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
    fn open_search_with_focus(&mut self, focus_target: SearchFocusTarget) {
        self.activate_workspace_surface();
        self.search_state.show_with_focus(focus_target);
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

    pub fn set_search_scope(&mut self, scope: SearchScope) {
        if self.search_state.scope != scope {
            self.search_state.scope = scope;
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

    pub fn poll_search(&mut self) {
        self.refresh_search_state();
    }

    pub(crate) fn search_progress(&self) -> SearchProgress {
        SearchProgress {
            searching: self.search_state.searching,
            displayed_match_count: self.search_state.displayed_match_count,
            total_match_count: self.search_state.total_match_count,
        }
    }

    pub(crate) fn search_result_groups(&self) -> &[SearchResultGroup] {
        &self.search_state.result_groups
    }

    pub(crate) fn activate_search_match_at(&mut self, index: usize) -> bool {
        self.activate_search_match(index)
    }

    pub fn select_next_search_match(&mut self) -> bool {
        let Some(index) = search::next_match_index(
            self.search_state.matches.len(),
            self.search_state.active_match_index,
        ) else {
            return false;
        };
        self.activate_search_match(index)
    }

    pub fn select_previous_search_match(&mut self) -> bool {
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
        self.set_active_search_index(Some(index));
        true
    }

    pub fn replace_current_search_match(&mut self) -> bool {
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

    pub fn replace_all_search_matches_in_active_buffer(&mut self) -> bool {
        let Some((active_view_id, active_buffer_id, matches)) = self.active_buffer_match_context()
        else {
            return false;
        };
        if matches.is_empty() {
            return false;
        }

        let replacement = self.search_state.replacement.clone();
        let previous_selection = self
            .active_tab()
            .and_then(|tab| tab.view(active_view_id))
            .and_then(|view| view.cursor_range)
            .unwrap_or_else(|| cursor_range_from_char_range(matches[0].clone()));
        let first_replacement_range =
            matches[0].start..matches[0].start + replacement.chars().count();
        let next_selection = cursor_range_from_char_range(first_replacement_range.clone());
        let replacements = matches
            .iter()
            .rev()
            .map(|range| (range.clone(), replacement.clone()))
            .collect::<Vec<_>>();

        let Some(buffer_label) = self.replace_ranges_in_active_buffer(
            active_view_id,
            active_buffer_id,
            &replacements,
            previous_selection,
            next_selection,
            "Search replace-all failed for the active buffer.",
        ) else {
            return false;
        };

        self.refresh_search_state();
        self.select_first_match_in_active_buffer();
        self.set_info_status(format!(
            "Replaced {} matches in {}.",
            matches.len(),
            buffer_label
        ));
        true
    }
}
