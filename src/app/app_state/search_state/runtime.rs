use super::helpers::{cursor_range_from_char_range, search_highlight_state_for_view};
use super::worker::{SearchResult, SearchTargetSnapshot, process_search_request};
use super::{ScratchpadApp, SearchFocusTarget, SearchMatch, SearchScope};
use crate::app::domain::{BufferId, SearchHighlightState, ViewId};
use crate::app::services::search;
use eframe::egui;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::atomic::AtomicU64;

impl ScratchpadApp {
    pub(crate) fn refresh_search_view_state(&mut self) {
        if !self.search_is_active() {
            self.clear_search_highlights();
            return;
        }
        self.refresh_search_visual_state();
    }

    pub(crate) fn take_search_focus_target(&mut self) -> Option<SearchFocusTarget> {
        self.search_state.focus_target.take()
    }

    pub(crate) fn request_search_focus(&mut self, target: SearchFocusTarget) {
        self.search_state.focus_target = Some(target);
    }

    pub(crate) fn refresh_search_state(&mut self) {
        self.poll_search_results();
        if !self.search_is_active() {
            self.search_state.clear_inactive_results();
            self.clear_search_highlights();
            return;
        }
        if !self.search_state.dirty {
            return;
        }

        self.submit_search_request();
        self.search_state.dirty = false;
    }

    pub(super) fn active_buffer_match_context(
        &self,
    ) -> Option<(ViewId, BufferId, Vec<Range<usize>>)> {
        let tab = self.active_tab()?;
        let active_view = tab.active_view()?;
        let buffer = tab.active_buffer();
        let matches = search::find_matches(
            buffer.text(),
            &self.search_state.query,
            self.search_state.search_options(),
        );
        Some((active_view.id, active_view.buffer_id, matches))
    }

    pub(super) fn replace_ranges_in_active_buffer(
        &mut self,
        view_id: ViewId,
        buffer_id: BufferId,
        replacements: &[(Range<usize>, String)],
        previous_selection: egui::text::CCursorRange,
        next_selection: egui::text::CCursorRange,
        error_message: &str,
    ) -> Option<String> {
        let active_tab_index = self.active_tab_index();
        let buffer_label = self.active_buffer_transaction_label()?;
        let transaction_snapshot = self.capture_transaction_snapshot();

        let replaced = {
            let tab = &mut self.tabs_mut()[active_tab_index];
            let buffer = tab.buffer_by_id_mut(buffer_id)?;
            if buffer
                .replace_char_ranges_with_undo(replacements, previous_selection, next_selection)
                .is_err()
            {
                false
            } else {
                if let Some(view) = tab.view_mut(view_id) {
                    view.cursor_range = Some(next_selection);
                    view.pending_cursor_range = Some(next_selection);
                }
                true
            }
        };
        if !replaced {
            self.set_error_status(error_message);
            return None;
        }

        self.finalize_active_buffer_text_mutation(
            active_tab_index,
            buffer_id,
            buffer_label.clone(),
            transaction_snapshot,
        );
        Some(buffer_label)
    }

    pub(crate) fn mark_search_dirty(&mut self) {
        if self.search_state.open {
            self.search_state.dirty = true;
        }
    }

    fn submit_search_request(&mut self) {
        let generation = self.search_state.requested_generation.saturating_add(1);
        let request = self.search_state.build_request(
            generation,
            self.collect_search_targets(self.search_state.scope),
        );
        self.search_state.begin_request(generation);
        self.clear_search_highlights();

        if let Err(error) = self.search_state.request_tx.send(request) {
            let latest_generation = AtomicU64::new(generation);
            if let Some(result) = process_search_request(error.0, &latest_generation) {
                self.apply_search_result(result);
            }
        }
    }

    fn poll_search_results(&mut self) {
        let mut latest_result = None;
        while let Ok(result) = self.search_state.result_rx.try_recv() {
            if result.generation == self.search_state.requested_generation {
                latest_result = Some(result);
            }
        }
        if let Some(result) = latest_result {
            self.apply_search_result(result);
        }
    }

    fn apply_search_result(&mut self, result: SearchResult) {
        self.search_state.active_match_index = self.preferred_active_match_index(
            &result.matches,
            self.search_state.previous_active_match.as_ref(),
        );
        self.search_state.matches = result.matches;
        self.search_state.total_match_count = self.search_state.matches.len();
        self.search_state.displayed_match_count = result.displayed_match_count;
        self.search_state.result_groups = result.result_groups;
        self.search_state.searching = false;
        self.search_state.previous_active_match = None;
        self.refresh_search_visual_state();
    }

    fn collect_search_targets(&self, scope: SearchScope) -> Vec<SearchTargetSnapshot> {
        match scope {
            SearchScope::ActiveBuffer => self.active_search_target().into_iter().collect(),
            SearchScope::ActiveWorkspaceTab => {
                self.collect_search_targets_for_tab(self.active_tab_index())
            }
            SearchScope::AllOpenTabs => (0..self.tabs().len())
                .flat_map(|tab_index| self.collect_search_targets_for_tab(tab_index))
                .collect(),
        }
    }

    fn active_search_target(&self) -> Option<SearchTargetSnapshot> {
        let tab_index = self.active_tab_index();
        let view_id = self.active_tab()?.active_view_id;
        let tab_label = self.search_tab_label(tab_index);
        self.search_target_for_view(tab_index, view_id, &tab_label)
    }

    fn collect_search_targets_for_tab(&self, tab_index: usize) -> Vec<SearchTargetSnapshot> {
        let Some(tab) = self.tabs().get(tab_index) else {
            return Vec::new();
        };

        let mut seen_buffer_ids = HashSet::new();
        let mut ordered_view_ids = Vec::with_capacity(tab.views.len());
        ordered_view_ids.push(tab.active_view_id);
        ordered_view_ids.extend(
            tab.views
                .iter()
                .map(|view| view.id)
                .filter(|view_id| *view_id != tab.active_view_id),
        );
        let tab_label = self.search_tab_label(tab_index);
        let mut targets = Vec::new();

        for view_id in ordered_view_ids {
            let Some(view) = tab.view(view_id) else {
                continue;
            };
            if !seen_buffer_ids.insert(view.buffer_id) {
                continue;
            }
            let Some(target) = self.search_target_for_view(tab_index, view_id, &tab_label) else {
                continue;
            };
            targets.push(target);
        }
        targets
    }

    fn search_tab_label(&self, tab_index: usize) -> String {
        self.display_tab_name_at_slot(self.slot_for_workspace_index(tab_index))
            .unwrap_or_else(|| format!("Tab {}", tab_index + 1))
    }

    fn search_target_for_view(
        &self,
        tab_index: usize,
        view_id: ViewId,
        tab_label: &str,
    ) -> Option<SearchTargetSnapshot> {
        let tab = self.tabs().get(tab_index)?;
        let view = tab.view(view_id)?;
        let buffer = tab.buffer_by_id(view.buffer_id)?;
        Some(SearchTargetSnapshot {
            tab_index,
            view_id,
            buffer_id: view.buffer_id,
            tab_label: tab_label.to_owned(),
            buffer_label: buffer.name.clone(),
            text: buffer.text().to_owned(),
        })
    }

    fn preferred_active_match_index(
        &self,
        matches: &[SearchMatch],
        previous_active: Option<&SearchMatch>,
    ) -> Option<usize> {
        if matches.is_empty() {
            return None;
        }
        if let Some(previous_active) = previous_active
            && let Some(index) = matches
                .iter()
                .position(|search_match| search_match == previous_active)
        {
            return Some(index);
        }

        let active_buffer_id = self
            .active_tab()
            .and_then(|tab| tab.active_view())
            .map(|view| view.buffer_id);
        if let Some(active_buffer_id) = active_buffer_id
            && let Some(index) = matches.iter().position(|search_match| {
                search_match.tab_index == self.active_tab_index()
                    && search_match.buffer_id == active_buffer_id
            })
        {
            return Some(index);
        }

        matches
            .iter()
            .position(|search_match| search_match.tab_index == self.active_tab_index())
            .or(Some(0))
    }

    fn active_buffer_identity(&self) -> Option<(usize, BufferId)> {
        let active_tab_index = self.active_tab_index();
        let active_buffer_id = self.active_tab()?.active_view()?.buffer_id;
        Some((active_tab_index, active_buffer_id))
    }

    fn active_buffer_match_index_at_or_after(&self, minimum_start: usize) -> Option<usize> {
        let (active_tab_index, active_buffer_id) = self.active_buffer_identity()?;
        self.search_state
            .matches
            .iter()
            .position(|search_match| {
                search_match.tab_index == active_tab_index
                    && search_match.buffer_id == active_buffer_id
                    && search_match.range.start >= minimum_start
            })
            .or_else(|| {
                self.search_state.matches.iter().position(|search_match| {
                    search_match.tab_index == active_tab_index
                        && search_match.buffer_id == active_buffer_id
                })
            })
    }

    fn active_search_match_range(&self) -> Option<Range<usize>> {
        self.search_state
            .active_match_index
            .and_then(|index| self.search_state.matches.get(index))
            .map(|search_match| search_match.range.clone())
    }

    fn sync_active_search_cursor(&mut self) {
        let Some(search_range) = self.active_search_match_range() else {
            return;
        };
        if let Some(view) = self.active_view_mut() {
            view.pending_cursor_range = Some(cursor_range_from_char_range(search_range));
        }
    }

    fn refresh_search_visual_state(&mut self) {
        self.sync_search_result_group_activity();
        self.apply_search_highlights();
    }

    pub(super) fn set_active_search_index(&mut self, index: Option<usize>) {
        self.search_state.active_match_index = index;
        self.sync_active_search_cursor();
        self.refresh_search_visual_state();
    }

    fn search_is_active(&self) -> bool {
        self.search_state.open && !self.search_state.query.is_empty()
    }

    pub(super) fn select_next_active_buffer_match_from(&mut self, minimum_start: usize) {
        self.set_active_search_index(self.active_buffer_match_index_at_or_after(minimum_start));
    }

    pub(super) fn select_first_match_in_active_buffer(&mut self) {
        self.set_active_search_index(self.active_buffer_match_index_at_or_after(0));
    }

    fn sync_search_result_group_activity(&mut self) {
        let active_match_index = self.search_state.active_match_index;
        for group in &mut self.search_state.result_groups {
            for entry in &mut group.entries {
                entry.active = Some(entry.match_index) == active_match_index;
            }
        }
    }

    fn apply_search_highlights(&mut self) {
        if !self.search_is_active() {
            self.clear_search_highlights();
            return;
        }

        let active_tab_index = self.active_tab_index();
        let highlights = self
            .tabs()
            .get(active_tab_index)
            .map(|tab| {
                tab.views
                    .iter()
                    .map(|view| {
                        (
                            view.id,
                            search_highlight_state_for_view(
                                active_tab_index,
                                view.buffer_id,
                                &self.search_state.matches,
                                self.search_state.active_match_index,
                            ),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        self.clear_search_highlights();
        let Some(tab) = self.tabs_mut().get_mut(active_tab_index) else {
            return;
        };

        for (view_id, highlights) in highlights {
            if let Some(view) = tab.view_mut(view_id) {
                view.search_highlights = highlights;
            }
        }
    }

    pub(super) fn clear_search_highlights(&mut self) {
        for tab in self.tabs_mut() {
            for view in &mut tab.views {
                view.search_highlights = SearchHighlightState::default();
            }
        }
    }
}
