use super::worker::{SearchFileIdentity, SearchTargetSnapshot};
use super::{ReplacementTargetPlan, SearchMatch};
use crate::app::domain::{BufferId, EditorViewState, SearchHighlightState, ViewId, WorkspaceTab};
use crate::app::services::search::SearchError;
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
    mut replacement_for_match: impl FnMut(&SearchMatch) -> Result<String, SearchError>,
) -> Result<Vec<ReplacementTargetPlan>, SearchError> {
    let mut targets = Vec::new();
    for group in matches.chunk_by(same_replacement_target) {
        // slice::chunk_by yields only nonempty groups.
        let current = &group[0];
        let mut replacements = Vec::with_capacity(group.len());
        for matched in group.iter().rev() {
            let replacement = replacement_for_match(matched)?;
            replacements.push((matched.range.clone(), replacement));
        }
        targets.push(ReplacementTargetPlan {
            tab_index: current.tab_index,
            view_id: current.view_id,
            buffer_id: current.buffer_id,
            buffer_label: current.buffer_label.to_string(),
            target_revision: current.target_revision,
            target_identity: current.target_identity.clone(),
            replacements,
        });
    }
    Ok(targets)
}

pub(super) fn collect_search_targets_for_views<'a>(
    tab_index: usize,
    tab: &WorkspaceTab,
    tab_label: &str,
    search_range: Option<Range<usize>>,
    prioritized_buffer_id: Option<BufferId>,
    views: impl IntoIterator<Item = &'a EditorViewState>,
) -> Vec<SearchTargetSnapshot> {
    let mut targets_by_file = HashMap::with_capacity(tab.buffers().count());
    for view in views {
        if let Some(target) =
            build_search_target_from_view(tab_index, tab, view, tab_label, search_range.clone())
        {
            targets_by_file
                .entry(target.file_identity.clone())
                .or_insert(target);
        }
    }

    let mut ordered_files = ordered_unique_file_identities(tab);
    rotate_prioritized_file(&mut ordered_files, tab, prioritized_buffer_id);
    ordered_files
        .into_iter()
        .filter_map(|file| targets_by_file.remove(&file))
        .collect()
}

pub(super) fn fallback_selection_for_target(target: &ReplacementTargetPlan) -> CursorRange {
    cursor_range_from_char_range(first_document_order_replacement(target).0.clone())
}

pub(super) fn next_selection_for_target(target: &ReplacementTargetPlan) -> CursorRange {
    let (range, replacement) = first_document_order_replacement(target);
    let replacement_len = replacement.chars().count();
    cursor_range_from_char_range(range.start..range.start + replacement_len)
}

pub(super) fn first_document_order_replacement(
    target: &ReplacementTargetPlan,
) -> &(Range<usize>, String) {
    target
        .replacements
        .last()
        .expect("replacement target requires at least one replacement")
}

pub(super) fn build_search_target(
    tab_index: usize,
    tab: &WorkspaceTab,
    view_id: ViewId,
    tab_label: &str,
    search_range: Option<Range<usize>>,
) -> Option<SearchTargetSnapshot> {
    let view = tab.layout.view(view_id)?;
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

fn ordered_unique_file_identities(tab: &WorkspaceTab) -> Vec<SearchFileIdentity> {
    let mut seen_files = HashSet::with_capacity(tab.layout.views.len());
    let mut ordered_files = Vec::with_capacity(tab.layout.views.len());
    for view in &tab.layout.views {
        let Some(buffer) = tab.buffer_by_id(view.buffer_id) else {
            continue;
        };
        let identity = file_identity_for_buffer(buffer);
        if seen_files.insert(identity.clone()) {
            ordered_files.push(identity);
        }
    }
    ordered_files
}

fn rotate_prioritized_file(
    ordered_files: &mut [SearchFileIdentity],
    tab: &WorkspaceTab,
    prioritized_buffer_id: Option<BufferId>,
) {
    let Some(prioritized_buffer_id) = prioritized_buffer_id else {
        return;
    };
    let Some(buffer) = tab.buffer_by_id(prioritized_buffer_id) else {
        return;
    };
    let prioritized_file = file_identity_for_buffer(buffer);
    if let Some(index) = ordered_files
        .iter()
        .position(|identity| *identity == prioritized_file)
    {
        ordered_files.rotate_left(index);
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
        file_identity: file_identity_for_buffer(buffer),
        tab_index,
        view_id: view.id,
        buffer_id: view.buffer_id,
        tab_label: tab_label.to_owned(),
        buffer_label: buffer.display_name(),
        document_snapshot,
        search_range,
    })
}

fn file_identity_for_buffer(buffer: &crate::app::domain::BufferState) -> SearchFileIdentity {
    buffer
        .path
        .clone()
        .map(SearchFileIdentity::Path)
        .unwrap_or(SearchFileIdentity::Untitled(buffer.id))
}

#[cfg(test)]
mod tests {
    use super::collect_search_targets_for_views;
    use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
    use std::path::PathBuf;

    #[test]
    fn target_collection_searches_one_file_once_across_many_views() {
        let mut tab = WorkspaceTab::new(BufferState::new(
            "many.md".to_owned(),
            "plan".to_owned(),
            None,
        ));
        for _ in 0..8 {
            assert!(tab.split_active_view(SplitAxis::Vertical).is_some());
        }

        let targets =
            collect_search_targets_for_views(0, &tab, "tab", None, None, tab.layout.views.iter());

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].buffer_id, tab.buffers.buffer.id);
    }

    #[test]
    fn target_collection_searches_same_saved_path_once() {
        let path = PathBuf::from("shared.md");
        let first = BufferState::new(
            "shared.md".to_owned(),
            "plan".to_owned(),
            Some(path.clone()),
        );
        let second = BufferState::new("shared.md".to_owned(), "plan".to_owned(), Some(path));
        let mut tab = WorkspaceTab::new(first);
        assert!(
            tab.open_buffer_as_split(second, SplitAxis::Vertical, true, 0.5)
                .is_some()
        );

        let targets =
            collect_search_targets_for_views(0, &tab, "tab", None, None, tab.layout.views.iter());

        assert_eq!(targets.len(), 1);
    }
}
