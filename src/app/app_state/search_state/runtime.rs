use super::helpers::{cursor_range_from_char_range, search_highlight_state_for_view};
use super::worker::{SearchRequest, SearchResult, SearchTargetSnapshot, process_search_request};
use super::{
    ReplacementPlan, ReplacementTargetPlan, ScratchpadApp, SearchFocusTarget, SearchFreshness,
    SearchMatch, SearchScope, SearchStatus,
};
use crate::app::domain::{
    BufferId, CursorRevealMode, EditorViewState, SearchHighlightState, ViewId, WorkspaceTab,
};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::collections::{HashMap, HashSet};
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

    pub(super) fn replace_ranges_in_active_buffer(
        &mut self,
        view_id: ViewId,
        buffer_id: BufferId,
        replacements: &[(Range<usize>, String)],
        previous_selection: CursorRange,
        next_selection: CursorRange,
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
                    view.request_cursor_reveal(CursorRevealMode::Center);
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
        self.search_state.active_match_index = self.preferred_active_match_index(
            &matches,
            self.search_state.previous_active_match.as_ref(),
        );
        self.search_state.matches = matches;
        self.search_state.total_match_count = self.search_state.matches.len();
        self.search_state.displayed_match_count = displayed_match_count;
        self.search_state.result_groups = result_groups;
        self.search_state.searching = false;
        self.search_state.previous_active_match = None;
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
        if let Some(view) = self.active_view_mut() {
            view.pending_cursor_range = Some(cursor_range_from_char_range(search_range));
            view.request_cursor_reveal(CursorRevealMode::Center);
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
            for view in &mut tab.views {
                view.search_highlights.ranges.clear();
                view.search_highlights.active_range_index = None;
            }
        }
    }

    pub(crate) fn replace_all_search_matches_in_scope(&mut self) -> bool {
        let Some(plan) = self.build_replace_all_plan() else {
            return false;
        };
        if plan.total_match_count == 0 {
            return false;
        }

        if plan.scope == SearchScope::ActiveBuffer && plan.targets.len() == 1 {
            return self.replace_all_in_active_buffer(&plan);
        }

        let replaced = self.replace_all_in_multiple_buffers(&plan);
        if replaced {
            self.set_info_status(format!(
                "Replaced {} matches across {} buffers.",
                plan.total_match_count,
                plan.affected_buffer_count()
            ));
        }
        replaced
    }

    fn replace_all_in_active_buffer(&mut self, plan: &ReplacementPlan) -> bool {
        let target = &plan.targets[0];
        let previous_selection = self
            .active_tab()
            .and_then(|tab| tab.view(target.view_id))
            .and_then(|view| view.cursor_range)
            .unwrap_or_else(|| cursor_range_from_char_range(target.replacements[0].0.clone()));
        let next_selection = cursor_range_from_char_range(
            target.replacements[0].0.start
                ..target.replacements[0].0.start + target.replacements[0].1.chars().count(),
        );
        let Some(buffer_label) = self.replace_ranges_in_active_buffer(
            target.view_id,
            target.buffer_id,
            &target.replacements,
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
            plan.total_match_count, buffer_label
        ));
        true
    }

    fn replace_all_in_multiple_buffers(&mut self, plan: &ReplacementPlan) -> bool {
        let snapshot = self.capture_transaction_snapshot();
        let mut affected_items = Vec::with_capacity(plan.targets.len());

        for target in &plan.targets {
            if !self.apply_replacement_target(target) {
                self.set_error_status(
                    "Search replace-all failed before all targets could be updated.",
                );
                return false;
            }
            affected_items.push(target.buffer_label.clone());
        }

        self.record_transaction(
            "Replace all matches",
            affected_items.clone(),
            Some(format!(
                "Replaced {} matches across {} buffers.",
                plan.total_match_count,
                plan.affected_buffer_count()
            )),
            snapshot,
        );
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.refresh_search_state();
        true
    }

    fn build_replace_all_plan(&self) -> Option<ReplacementPlan> {
        if self.search_state.matches.is_empty() {
            return None;
        }

        Some(ReplacementPlan {
            scope: self.search_state.scope,
            total_match_count: self.search_state.matches.len(),
            targets: build_replacement_targets(
                &self.search_state.matches,
                &self.search_state.replacement,
            ),
        })
    }

    fn apply_replacement_target(&mut self, target: &ReplacementTargetPlan) -> bool {
        let Some(tab) = self.tabs_mut().get_mut(target.tab_index) else {
            return false;
        };
        let previous_selection = tab
            .view(target.view_id)
            .and_then(|view| view.cursor_range)
            .unwrap_or_else(|| fallback_selection_for_target(target));
        let next_selection = next_selection_for_target(target);
        let Some(buffer) = tab.buffer_by_id_mut(target.buffer_id) else {
            return false;
        };
        if buffer
            .replace_char_ranges_with_undo(&target.replacements, previous_selection, next_selection)
            .is_err()
        {
            return false;
        }
        if let Some(view) = tab.view_mut(target.view_id) {
            view.cursor_range = Some(next_selection);
            view.pending_cursor_range = Some(next_selection);
            view.request_cursor_reveal(CursorRevealMode::Center);
        }
        self.finalize_tab_buffer_mutation(target.tab_index, target.buffer_id);
        true
    }

    fn finalize_tab_buffer_mutation(&mut self, tab_index: usize, buffer_id: BufferId) {
        let tab = &mut self.tabs_mut()[tab_index];
        let has_control_chars = tab
            .buffer_by_id(buffer_id)
            .map(|buffer| buffer.artifact_summary.has_control_chars())
            .unwrap_or(false);
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer.is_dirty = true;
        }
        for view in &mut tab.views {
            if view.buffer_id == buffer_id && !has_control_chars {
                view.show_control_chars = false;
            }
        }
        let _ = tab;
        self.note_settings_toml_edit(tab_index);
    }
}

fn build_replacement_targets(
    matches: &[SearchMatch],
    replacement: &str,
) -> Vec<ReplacementTargetPlan> {
    let mut targets = Vec::new();
    let mut start = 0;
    while start < matches.len() {
        let current_match = &matches[start];
        let mut end = start + 1;
        while end < matches.len() && same_replacement_target(&matches[end], current_match) {
            end += 1;
        }

        let replacements = matches[start..end]
            .iter()
            .rev()
            .map(|search_match| (search_match.range.clone(), replacement.to_owned()))
            .collect();

        targets.push(ReplacementTargetPlan {
            tab_index: current_match.tab_index,
            view_id: current_match.view_id,
            buffer_id: current_match.buffer_id,
            buffer_label: current_match.buffer_label.clone(),
            replacements,
        });
        start = end;
    }
    targets
}

fn collect_search_targets_for_views<'a>(
    tab_index: usize,
    tab: &WorkspaceTab,
    tab_label: &str,
    search_range: Option<Range<usize>>,
    prioritized_buffer_id: Option<BufferId>,
    views: impl IntoIterator<Item = &'a EditorViewState>,
) -> Vec<SearchTargetSnapshot> {
    let mut targets_by_buffer = HashMap::with_capacity(tab.views.len());
    for view in views {
        if targets_by_buffer.contains_key(&view.buffer_id) {
            continue;
        }
        if let Some(target) =
            build_search_target_from_view(tab_index, tab, view, tab_label, search_range.clone())
        {
            targets_by_buffer.insert(view.buffer_id, target);
        }
    }

    let mut ordered_buffer_ids = ordered_unique_buffer_ids(tab);
    rotate_prioritized_buffer_id(&mut ordered_buffer_ids, prioritized_buffer_id);
    ordered_buffer_ids
        .into_iter()
        .filter_map(|buffer_id| targets_by_buffer.remove(&buffer_id))
        .collect()
}

fn ordered_unique_buffer_ids(tab: &WorkspaceTab) -> Vec<BufferId> {
    let mut seen_buffer_ids = HashSet::with_capacity(tab.views.len());
    let mut ordered_buffer_ids = Vec::with_capacity(tab.views.len());
    for view in &tab.views {
        if seen_buffer_ids.insert(view.buffer_id) {
            ordered_buffer_ids.push(view.buffer_id);
        }
    }
    ordered_buffer_ids
}

fn rotate_prioritized_buffer_id(
    ordered_buffer_ids: &mut [BufferId],
    prioritized_buffer_id: Option<BufferId>,
) {
    let Some(prioritized_buffer_id) = prioritized_buffer_id else {
        return;
    };
    if let Some(index) = ordered_buffer_ids
        .iter()
        .position(|buffer_id| *buffer_id == prioritized_buffer_id)
    {
        ordered_buffer_ids.rotate_left(index);
    }
}

fn same_replacement_target(left: &SearchMatch, right: &SearchMatch) -> bool {
    left.tab_index == right.tab_index && left.buffer_id == right.buffer_id
}

fn fallback_selection_for_target(target: &ReplacementTargetPlan) -> CursorRange {
    cursor_range_from_char_range(target.replacements[0].0.clone())
}

fn next_selection_for_target(target: &ReplacementTargetPlan) -> CursorRange {
    let range = &target.replacements[0].0;
    let replacement_len = target.replacements[0].1.chars().count();
    cursor_range_from_char_range(range.start..range.start + replacement_len)
}

fn build_search_target(
    tab_index: usize,
    tab: &WorkspaceTab,
    view_id: ViewId,
    tab_label: &str,
    search_range: Option<Range<usize>>,
) -> Option<SearchTargetSnapshot> {
    let view = tab.view(view_id)?;
    build_search_target_from_view(tab_index, tab, view, tab_label, search_range)
}

fn build_search_target_from_view(
    tab_index: usize,
    tab: &WorkspaceTab,
    view: &EditorViewState,
    tab_label: &str,
    search_range: Option<Range<usize>>,
) -> Option<SearchTargetSnapshot> {
    let buffer = tab.buffer_by_id(view.buffer_id)?;
    let document_snapshot = buffer.document_snapshot();
    let search_range = search_range.map(|range| document_snapshot.normalize_char_range(range));
    Some(SearchTargetSnapshot {
        tab_index,
        view_id: view.id,
        buffer_id: view.buffer_id,
        tab_label: tab_label.to_owned(),
        buffer_label: buffer.display_name(),
        document_snapshot,
        search_range,
    })
}

fn first_match_index(
    matches: &[SearchMatch],
    mut predicate: impl FnMut(&SearchMatch) -> bool,
) -> Option<usize> {
    matches.iter().position(&mut predicate)
}

fn matches_buffer(search_match: &SearchMatch, tab_index: usize, buffer_id: BufferId) -> bool {
    search_match.tab_index == tab_index && search_match.buffer_id == buffer_id
}
