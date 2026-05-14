use super::{
    SEARCH_PREVIEW_CACHE_LIMIT, ScratchpadApp, SearchFocusTarget, SearchMatch,
    SearchPreviewCacheKey, SearchResultEntry, SearchScope, SearchScopeOrigin,
};
use crate::app::commands::AppCommand;
use crate::app::services::search::{self, SearchMode};
use std::ops::Range;

fn default_search_scope_and_origin(app: &ScratchpadApp) -> (SearchScope, SearchScopeOrigin) {
    if app.active_search_selection_range().is_some() {
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

fn open_search_with_focus(app: &mut ScratchpadApp, focus_target: SearchFocusTarget) {
    app.activate_workspace_surface();
    let (default_scope, scope_origin) = default_search_scope_and_origin(app);
    app.state
        .search_state
        .show_with_focus(focus_target, default_scope, scope_origin);
    app.mark_search_dirty();
    app.refresh_search_state();
}

pub(crate) fn open_search(app: &mut ScratchpadApp) {
    open_search_with_focus(app, SearchFocusTarget::FindInput);
}

pub(crate) fn open_search_and_replace(app: &mut ScratchpadApp) {
    open_search_with_focus(app, SearchFocusTarget::ReplaceInput);
}

pub(crate) fn close_search(app: &mut ScratchpadApp) {
    app.state.search_state.close();
    app.clear_search_highlights();
    app.request_focus_for_active_view();
}

pub(crate) fn toggle_search(app: &mut ScratchpadApp) {
    if app.state.search_state.open() {
        close_search(app);
    } else {
        open_search(app);
    }
}

pub(crate) fn set_search_replace_open(app: &mut ScratchpadApp, open: bool) {
    app.state.search_state.set_replace_open(open);
    if app.state.search_state.panel.open && !app.state.search_state.query.query.is_empty() {
        app.refresh_search_visual_state();
    }
}

fn after_search_param_change(app: &mut ScratchpadApp) {
    app.state.search_state.clear_replace_all_confirmation();
    app.mark_search_dirty();
    app.refresh_search_state();
}

pub(crate) fn set_search_query(app: &mut ScratchpadApp, query: impl Into<String>) {
    if app.state.search_state.set_query(query) {
        after_search_param_change(app);
    }
}

pub(crate) fn set_search_replacement(app: &mut ScratchpadApp, replacement: impl Into<String>) {
    if app.state.search_state.set_replacement(replacement)
        && app.state.search_state.panel.open
        && !app.state.search_state.query.query.is_empty()
    {
        app.refresh_search_visual_state();
    }
}

pub(crate) fn set_search_scope(app: &mut ScratchpadApp, scope: SearchScope) {
    set_search_scope_with_origin(app, scope, SearchScopeOrigin::Manual);
}

pub(crate) fn set_search_scope_with_origin(
    app: &mut ScratchpadApp,
    scope: SearchScope,
    origin: SearchScopeOrigin,
) {
    if app.state.search_state.set_scope_with_origin(scope, origin) {
        after_search_param_change(app);
    }
}

pub(crate) fn set_search_mode(app: &mut ScratchpadApp, mode: SearchMode) {
    if app.state.search_state.set_mode(mode) {
        after_search_param_change(app);
    }
}

pub(crate) fn set_search_match_case(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.search_state.set_match_case(enabled) {
        after_search_param_change(app);
    }
}

pub(crate) fn set_search_whole_word(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.search_state.set_whole_word(enabled) {
        after_search_param_change(app);
    }
}

pub(crate) fn focus_search_result_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
    let Some(search_match) = app.state.search_state.results.matches.get(index).cloned() else {
        return false;
    };
    focus_search_match(app, search_match)
}

pub(crate) fn activate_search_match_at(app: &mut ScratchpadApp, index: usize) -> bool {
    activate_search_match(app, index)
}

pub(crate) fn select_next_search_match(app: &mut ScratchpadApp) -> bool {
    select_search_match_via(app, search::next_match_index)
}

pub(crate) fn select_previous_search_match(app: &mut ScratchpadApp) -> bool {
    select_search_match_via(app, search::previous_match_index)
}

fn select_search_match_via(
    app: &mut ScratchpadApp,
    pick: impl FnOnce(usize, Option<usize>) -> Option<usize>,
) -> bool {
    if !app
        .state
        .search_state
        .replace_availability()
        .allows_actions()
    {
        return false;
    }
    let Some(index) = pick(
        app.state.search_state.results.matches.len(),
        app.state.search_state.results.active_match_index,
    ) else {
        return false;
    };
    activate_search_match(app, index)
}

pub(super) fn activate_search_match(app: &mut ScratchpadApp, index: usize) -> bool {
    let Some(search_match) = app.state.search_state.results.matches.get(index).cloned() else {
        return false;
    };
    if !focus_search_match(app, search_match) {
        return false;
    }

    app.set_active_search_index(Some(index));
    true
}

fn focus_search_match(app: &mut ScratchpadApp, search_match: SearchMatch) -> bool {
    let preserve_session_clean = !app.tab_manager.session_dirty;

    if search_match.tab_index != app.tab_manager.active_tab_index {
        app.handle_command(AppCommand::ActivateTab {
            index: search_match.tab_index,
        });
        app.state.pending_editor_focus = None;
    }
    let destination_slot = app.slot_for_workspace_index(search_match.tab_index);
    app.select_only_tab_slot(destination_slot);
    if app
        .tab_manager
        .active_tab()
        .is_some_and(|tab| tab.active_view_id != search_match.view_id)
    {
        app.handle_command(AppCommand::ActivateView {
            view_id: search_match.view_id,
        });
        app.state.pending_editor_focus = None;
    }
    if preserve_session_clean {
        app.tab_manager.clear_session_dirty();
    }
    true
}

impl ScratchpadApp {
    pub fn open_search(&mut self) {
        open_search(self);
    }

    pub fn open_search_and_replace(&mut self) {
        open_search_and_replace(self);
    }

    pub fn toggle_search(&mut self) {
        toggle_search(self);
    }

    pub fn search_open(&self) -> bool {
        self.state.search_state.open()
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) {
        set_search_query(self, query);
    }

    pub fn set_search_replacement(&mut self, replacement: impl Into<String>) {
        set_search_replacement(self, replacement);
    }

    pub fn set_search_scope(&mut self, scope: SearchScope) {
        set_search_scope(self, scope);
    }

    pub fn search_match_count(&self) -> usize {
        self.state.search_state.match_count()
    }

    pub fn search_active_match_index(&self) -> Option<usize> {
        self.state.search_state.active_match_index()
    }

    pub fn poll_search(&mut self) {
        self.refresh_search_state();
    }

    pub(crate) fn search_result_entry_at(
        &mut self,
        match_index: usize,
    ) -> Option<SearchResultEntry> {
        let key = SearchPreviewCacheKey {
            generation: self.state.search_state.runtime.applied_generation,
            match_index,
        };
        if let Some(entry) = self.state.search_state.preview.entries.get(&key).cloned() {
            self.touch_search_preview_cache_key(key);
            return Some(entry_with_active_state(
                entry,
                match_index,
                self.state.search_state.results.active_match_index,
            ));
        }

        let search_match = self
            .state
            .search_state
            .results
            .matches
            .get(match_index)?
            .clone();
        let (line_number, column_number, preview) = self
            .tab_manager
            .tabs
            .as_slice()
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
            active: Some(match_index) == self.state.search_state.results.active_match_index,
        };
        self.store_search_preview_cache_entry(key, entry.clone());
        Some(entry)
    }

    fn store_search_preview_cache_entry(
        &mut self,
        key: SearchPreviewCacheKey,
        entry: SearchResultEntry,
    ) {
        self.touch_search_preview_cache_key(key.clone());
        self.state.search_state.preview.entries.insert(key, entry);
        while self.state.search_state.preview.order.len() > SEARCH_PREVIEW_CACHE_LIMIT {
            let Some(expired) = self.state.search_state.preview.order.pop_front() else {
                break;
            };
            self.state.search_state.preview.entries.remove(&expired);
        }
    }

    fn touch_search_preview_cache_key(&mut self, key: SearchPreviewCacheKey) {
        self.state
            .search_state
            .preview
            .order
            .retain(|existing| existing != &key);
        self.state.search_state.preview.order.push_back(key);
    }

    pub fn replace_all_search_matches(&mut self) -> bool {
        if !self
            .state
            .search_state
            .replace_availability()
            .allows_actions()
        {
            return false;
        }
        self.replace_all_search_matches_in_scope()
    }

    pub(crate) fn active_search_selection_range(&self) -> Option<Range<usize>> {
        self.tab_manager
            .active_tab()?
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
