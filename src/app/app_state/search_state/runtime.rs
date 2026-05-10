use super::helpers::{
    build_search_target, collect_search_targets_for_views,
    collect_search_targets_for_views_with_seen, first_match_index, matches_buffer,
};
use super::worker::{
    SearchFileIdentity, SearchRequest, SearchResult, SearchTargetSnapshot, process_search_request,
};
use super::{
    ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchMatch, SearchScope, SearchStatus,
};
use crate::app::domain::BufferId;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
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
            self.clear_inactive_search_state();
            return;
        }
        if !self.search_state.dirty {
            return;
        }

        if self.search_state.scope == SearchScope::SelectionOnly
            && self.active_search_selection_range().is_none()
        {
            self.set_selection_only_search_error();
            return;
        }

        self.submit_search_request();
        self.search_state.dirty = false;
    }

    pub(crate) fn mark_search_dirty(&mut self) {
        if self.search_state.open {
            self.search_state.dirty = true;
            if !matches!(self.search_state.status, SearchStatus::Idle) {
                self.search_state.freshness = SearchFreshness::Stale;
            }
        }
    }

    fn submit_search_request(&mut self) {
        let generation = self.search_state.requested_generation.saturating_add(1);
        let targets = self.collect_search_targets(self.search_state.scope);
        let request = self.search_state.build_request(generation, targets);
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
        let SearchResult {
            generation,
            matches,
            displayed_match_count,
            result_groups,
            status,
        } = result;
        let is_partial = matches!(status, SearchStatus::Searching { .. });
        self.search_state.active_match_index = self.preferred_active_match_index(
            &matches,
            self.search_state.previous_active_match.as_ref(),
        );
        self.search_state.matches = matches;
        self.search_state.total_match_count = self.search_state.matches.len();
        self.search_state.displayed_match_count = displayed_match_count;
        self.search_state.result_groups = Arc::from(result_groups);
        self.search_state.searching = is_partial;
        if !is_partial {
            self.search_state.previous_active_match = None;
        }
        self.search_state.applied_generation = generation;
        self.search_state.status = status;
        self.search_state.freshness = SearchFreshness::Fresh;
        self.refresh_search_visual_state();
    }

    #[doc(hidden)]
    pub fn profile_build_search_request(&self, scope: SearchScope, query: &str) -> usize {
        let generation = self.search_state.requested_generation.saturating_add(1);
        let targets = self.collect_search_targets(scope);
        let request = SearchRequest {
            generation,
            query: query.to_owned(),
            options: self.search_state.search_options(),
            targets,
        };
        request
            .targets
            .iter()
            .map(|target| target.document_snapshot.document_length().chars)
            .sum::<usize>()
            + request.query.len()
    }

    fn collect_search_targets(&self, scope: SearchScope) -> Vec<SearchTargetSnapshot> {
        match scope {
            SearchScope::SelectionOnly => self
                .active_search_target(self.active_search_selection_range())
                .into_iter()
                .collect(),
            SearchScope::ActiveBuffer => self.active_search_target(None).into_iter().collect(),
            SearchScope::ActiveWorkspaceTab => self.collect_active_tab_search_targets(),
            SearchScope::AllOpenTabs => {
                let active_tab_index = self.active_tab_index();
                let mut seen_files = HashSet::<SearchFileIdentity>::new();
                (0..self.tabs().len())
                    .map(|offset| (active_tab_index + offset) % self.tabs().len().max(1))
                    .flat_map(|tab_index| {
                        let prioritized_buffer_id = (tab_index == active_tab_index)
                            .then(|| {
                                self.active_tab()
                                    .and_then(|tab| tab.active_view())
                                    .map(|view| view.buffer_id)
                            })
                            .flatten();
                        self.collect_search_targets_for_tab_with_seen(
                            tab_index,
                            prioritized_buffer_id,
                            None,
                            &mut seen_files,
                        )
                    })
                    .collect()
            }
        }
    }

    fn clear_inactive_search_state(&mut self) {
        self.search_state.clear_inactive_results();
        self.search_state.status = SearchStatus::Idle;
        self.search_state.freshness = SearchFreshness::Fresh;
        self.clear_search_highlights();
    }

    fn set_selection_only_search_error(&mut self) {
        self.search_state.searching = false;
        self.search_state.status =
            SearchStatus::Error("Selection-only search requires an active selection.".to_owned());
        self.search_state.freshness = SearchFreshness::Fresh;
        self.search_state.clear_match_results();
        self.search_state.dirty = false;
        self.clear_search_highlights();
    }

    fn collect_active_tab_search_targets(&self) -> Vec<SearchTargetSnapshot> {
        self.collect_search_targets_for_tab(
            self.active_tab_index(),
            self.active_tab()
                .and_then(|tab| tab.active_view())
                .map(|view| view.buffer_id),
            None,
        )
    }

    fn active_search_target(
        &self,
        search_range: Option<Range<usize>>,
    ) -> Option<SearchTargetSnapshot> {
        let tab_index = self.active_tab_index();
        let tab_label = self.search_tab_label(tab_index);
        let tab = self.active_tab()?;
        build_search_target(tab_index, tab, tab.active_view_id, &tab_label, search_range)
    }

    fn collect_search_targets_for_tab(
        &self,
        tab_index: usize,
        prioritized_buffer_id: Option<BufferId>,
        search_range: Option<Range<usize>>,
    ) -> Vec<SearchTargetSnapshot> {
        let Some(tab) = self.tabs().get(tab_index) else {
            return Vec::new();
        };
        let tab_label = self.search_tab_label(tab_index);
        collect_search_targets_for_views(
            tab_index,
            tab,
            &tab_label,
            search_range,
            prioritized_buffer_id,
            tab.ordered_view_ids_in_layout_order()
                .into_iter()
                .filter_map(|view_id| tab.view(view_id))
                .chain(tab.views.iter()),
        )
    }

    fn collect_search_targets_for_tab_with_seen(
        &self,
        tab_index: usize,
        prioritized_buffer_id: Option<BufferId>,
        search_range: Option<Range<usize>>,
        seen_files: &mut HashSet<SearchFileIdentity>,
    ) -> Vec<SearchTargetSnapshot> {
        let Some(tab) = self.tabs().get(tab_index) else {
            return Vec::new();
        };
        let tab_label = self.search_tab_label(tab_index);
        collect_search_targets_for_views_with_seen(
            tab_index,
            tab,
            &tab_label,
            search_range,
            prioritized_buffer_id,
            tab.ordered_view_ids_in_layout_order()
                .into_iter()
                .filter_map(|view_id| tab.view(view_id))
                .chain(tab.views.iter()),
            seen_files,
        )
    }

    fn search_tab_label(&self, tab_index: usize) -> String {
        self.display_tab_name_at_slot(self.slot_for_workspace_index(tab_index))
            .unwrap_or_else(|| format!("Tab {}", tab_index + 1))
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
            && let Some(index) =
                first_match_index(matches, |search_match| search_match == previous_active)
        {
            return Some(index);
        }

        if let Some((active_tab_index, active_buffer_id)) = self.active_buffer_identity()
            && let Some(index) = first_match_index(matches, |search_match| {
                matches_buffer(search_match, active_tab_index, active_buffer_id)
            })
        {
            return Some(index);
        }

        first_match_index(matches, |search_match| {
            search_match.tab_index == self.active_tab_index()
        })
        .or(Some(0))
    }

    pub(super) fn search_is_active(&self) -> bool {
        self.search_state.open && !self.search_state.query.is_empty()
    }
}
