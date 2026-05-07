use super::{
    SEARCH_PREVIEW_CACHE_LIMIT, ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchMatch,
    SearchPreviewCacheKey, SearchProgress, SearchReplaceAvailability, SearchResultEntry,
    SearchResultGroup, SearchScope, SearchScopeOrigin, SearchStatus,
};
use crate::app::commands::AppCommand;
use crate::app::services::search::{self, SearchMode};
use std::ops::Range;
use std::sync::Arc;

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

    pub(super) fn activate_search_match(&mut self, index: usize) -> bool {
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
            .and_then(super::helpers::selection_char_range)
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
