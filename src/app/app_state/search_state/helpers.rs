use super::worker::SearchTargetSnapshot;
use super::{ReplacementTargetPlan, SearchMatch};
use crate::app::domain::{BufferId, EditorViewState, SearchHighlightState, ViewId, WorkspaceTab};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub(super) fn cursor_range_from_char_range(range: Range<usize>) -> CursorRange {
    CursorRange::two(range.start, range.end)
}

pub(super) fn selection_char_range(cursor_range: CursorRange) -> Option<std::ops::Range<usize>> {
    let (start, end) = cursor_range.sorted_indices();
    (start < end).then_some(start..end)
}

pub(super) fn search_highlight_state_for_view(
    tab_index: usize,
    buffer_id: BufferId,
    matches: &[SearchMatch],
    active_match_index: Option<usize>,
) -> SearchHighlightState {
    let mut ranges = Vec::new();
    let mut active_range_index = None;

    for (match_index, search_match) in matches.iter().enumerate() {
        if search_match.tab_index != tab_index || search_match.buffer_id != buffer_id {
            continue;
        }
        if Some(match_index) == active_match_index {
            active_range_index = Some(ranges.len());
        }
        ranges.push(search_match.range.clone());
    }

    SearchHighlightState {
        ranges,
        active_range_index,
    }
}

pub(super) fn build_replacement_targets(
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

pub(super) fn collect_search_targets_for_views<'a>(
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

pub(super) fn fallback_selection_for_target(target: &ReplacementTargetPlan) -> CursorRange {
    cursor_range_from_char_range(target.replacements[0].0.clone())
}

pub(super) fn next_selection_for_target(target: &ReplacementTargetPlan) -> CursorRange {
    let range = &target.replacements[0].0;
    let replacement_len = target.replacements[0].1.chars().count();
    cursor_range_from_char_range(range.start..range.start + replacement_len)
}

pub(super) fn build_search_target(
    tab_index: usize,
    tab: &WorkspaceTab,
    view_id: ViewId,
    tab_label: &str,
    search_range: Option<Range<usize>>,
) -> Option<SearchTargetSnapshot> {
    let view = tab.view(view_id)?;
    build_search_target_from_view(tab_index, tab, view, tab_label, search_range)
}

pub(super) fn first_match_index(
    matches: &[SearchMatch],
    mut predicate: impl FnMut(&SearchMatch) -> bool,
) -> Option<usize> {
    matches.iter().position(&mut predicate)
}

pub(super) fn matches_buffer(
    search_match: &SearchMatch,
    tab_index: usize,
    buffer_id: BufferId,
) -> bool {
    search_match.tab_index == tab_index && search_match.buffer_id == buffer_id
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
