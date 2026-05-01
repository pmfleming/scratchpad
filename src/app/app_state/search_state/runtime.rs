use super::helpers::{
    build_search_target, collect_search_targets_for_views, cursor_range_from_char_range,
    first_match_index, matches_buffer, search_highlight_state_for_view,
};
use super::worker::{SearchRequest, SearchResult, SearchTargetSnapshot, process_search_request};
use super::{
    ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchMatch, SearchScope, SearchStatus,
};
use crate::app::domain::{
    BufferId, CursorRevealMode, EditorViewState, SearchHighlightState, ViewId,
};
use crate::app::ui::scrolling::{ScrollAlign, ScrollIntent};
use eframe::egui;
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
        let is_partial = matches!(status, SearchStatus::Searching);
        self.search_state.active_match_index = self.preferred_active_match_index(
            &matches,
            self.search_state.previous_active_match.as_ref(),
        );
        self.search_state.matches = matches;
        self.search_state.total_match_count = self.search_state.matches.len();
        self.search_state.displayed_match_count = displayed_match_count;
        self.search_state.result_groups = result_groups;
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
            SearchScope::AllOpenTabs => (0..self.tabs().len())
                .flat_map(|tab_index| self.collect_search_targets_for_tab(tab_index, None, None))
                .collect(),
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

    fn active_buffer_identity(&self) -> Option<(usize, BufferId)> {
        let active_tab_index = self.active_tab_index();
        let active_buffer_id = self.active_tab()?.active_view()?.buffer_id;
        Some((active_tab_index, active_buffer_id))
    }

    fn active_buffer_match_index_at_or_after(&self, minimum_start: usize) -> Option<usize> {
        let (active_tab_index, active_buffer_id) = self.active_buffer_identity()?;
        first_match_index(&self.search_state.matches, |search_match| {
            matches_buffer(search_match, active_tab_index, active_buffer_id)
                && search_match.range.start >= minimum_start
        })
        .or_else(|| {
            first_match_index(&self.search_state.matches, |search_match| {
                matches_buffer(search_match, active_tab_index, active_buffer_id)
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
        let cursor_range = cursor_range_from_char_range(search_range.clone());
        let active_tab_index = self.active_tab_index();
        let Some(view_id) = self.active_tab().map(|tab| tab.active_view_id) else {
            return;
        };
        if let Some((buffer, view)) = self.tabs_mut()[active_tab_index].buffer_and_view_mut(view_id)
        {
            view.set_pending_cursor_range_anchored(buffer, cursor_range);
            if request_search_reveal_intent(view, &search_range) {
                view.request_cursor_reveal(CursorRevealMode::KeepHorizontalVisible);
            } else {
                view.request_cursor_reveal(CursorRevealMode::Center);
            }
        }
    }

    pub(super) fn refresh_search_visual_state(&mut self) {
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

    pub(crate) fn select_next_active_buffer_match_from(&mut self, minimum_start: usize) {
        self.set_active_search_index(self.active_buffer_match_index_at_or_after(minimum_start));
    }

    pub(super) fn select_first_match_in_active_buffer(&mut self) {
        self.set_active_search_index(self.active_buffer_match_index_at_or_after(0));
    }

    fn sync_search_result_group_activity(&mut self) {
        let active_match_index = self.search_state.active_match_index;
        for group in &mut self.search_state.result_groups {
            group.active = false;
            for entry in &mut group.entries {
                entry.active = Some(entry.match_index) == active_match_index;
                group.active |= entry.active;
            }
        }
    }

    fn apply_search_highlights(&mut self) {
        if !self.search_is_active() {
            self.clear_search_highlights();
            return;
        }

        if self.search_state.searching {
            self.clear_search_highlights();
            return;
        }

        if !matches!(
            self.search_state.status,
            SearchStatus::Ready | SearchStatus::NoMatches
        ) {
            self.clear_search_highlights();
            return;
        }

        let active_tab_index = self.active_tab_index();
        let highlights = self.search_highlights_for_tab(active_tab_index);
        let replacement_preview = self.search_replacement_preview();

        self.clear_search_highlights();
        let Some(tab) = self.tabs_mut().get_mut(active_tab_index) else {
            return;
        };

        for (view_id, highlights) in highlights {
            if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
                view.set_search_highlights_anchored(buffer, highlights);
                view.set_search_replacement_preview(replacement_preview.clone());
            }
        }
    }

    fn search_replacement_preview(&self) -> Option<String> {
        (self.search_state.replace_open && self.search_state.status == SearchStatus::Ready)
            .then(|| self.search_state.replacement.clone())
    }

    fn search_highlights_for_tab(&self, tab_index: usize) -> Vec<(ViewId, SearchHighlightState)> {
        self.tabs()
            .get(tab_index)
            .map(|tab| {
                tab.views
                    .iter()
                    .map(|view| {
                        (
                            view.id,
                            search_highlight_state_for_view(
                                tab_index,
                                view.buffer_id,
                                &self.search_state.matches,
                                self.search_state.active_match_index,
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn clear_search_highlights(&mut self) {
        for tab in self.tabs_mut() {
            let mut anchors_to_release = Vec::new();
            for view in &mut tab.views {
                for anchor in view.clear_search_highlights_for_release() {
                    anchors_to_release.push((view.buffer_id, anchor));
                }
            }
            for (buffer_id, anchor) in anchors_to_release {
                if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
                    buffer
                        .document_mut()
                        .piece_tree_mut()
                        .release_anchor(anchor);
                }
            }
        }
    }
}

fn request_search_reveal_intent(view: &mut EditorViewState, search_range: &Range<usize>) -> bool {
    let Some(snapshot) = view.latest_display_snapshot.as_ref() else {
        return false;
    };
    let Some(y) = snapshot.pixel_y_for_char_offset(search_range.start as u32) else {
        return false;
    };
    let row_height = snapshot
        .row_height()
        .max(view.scroll.metrics().row_height)
        .max(1.0);
    view.request_intent(ScrollIntent::Reveal {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, y), egui::vec2(1.0, row_height)),
        align_y: Some(ScrollAlign::Center),
        align_x: None,
    });
    true
}
