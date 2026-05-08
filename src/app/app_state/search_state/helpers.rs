use super::worker::{SearchFileIdentity, SearchResult, SearchTargetSnapshot};
use super::{ReplacementTargetPlan, SearchMatch, SearchResultGroup, SearchStatus};
use crate::app::domain::{BufferId, EditorViewState, SearchHighlightState, ViewId, WorkspaceTab};
use crate::app::services::search::SearchError;
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

#[derive(Default, Clone)]
pub(super) struct SearchResultAccumulator {
    matches: Vec<SearchMatch>,
    result_groups: Vec<SearchResultGroup>,
    group_lookup: HashMap<(usize, BufferId), usize>,
}

impl SearchResultAccumulator {
    pub(super) fn push_target_matches(
        &mut self,
        target: &SearchTargetSnapshot,
        ranges: &[Range<usize>],
    ) {
        let start_index = self.matches.len();
        self.matches.extend(ranges.iter().cloned().map(|range| {
            SearchMatch {
                tab_index: target.tab_index,
                view_id: target.view_id,
                buffer_id: target.buffer_id,
                buffer_label: target.buffer_label.clone(),
                target_revision: target.document_snapshot.revision(),
                matched_text: target
                    .document_snapshot
                    .piece_tree()
                    .extract_range(range.clone()),
                range,
            }
        }));

        let group_index =
            if let Some(index) = self.group_lookup.get(&(target.tab_index, target.buffer_id)) {
                *index
            } else {
                let index = self.result_groups.len();
                self.result_groups.push(SearchResultGroup {
                    tab_index: target.tab_index,
                    buffer_id: target.buffer_id,
                    buffer_label: target.buffer_label.clone(),
                    tab_label: target.tab_label.clone(),
                    first_match_index: start_index,
                    total_match_count: 0,
                    active: false,
                });
                self.group_lookup
                    .insert((target.tab_index, target.buffer_id), index);
                index
            };

        let group = &mut self.result_groups[group_index];
        group.total_match_count += ranges.len();
    }

    pub(super) fn finish(self, generation: u64) -> SearchResult {
        let displayed_match_count = self.match_count();
        SearchResult {
            generation,
            matches: self.matches,
            result_groups: self.result_groups,
            displayed_match_count,
            status: SearchStatus::NoMatches,
        }
    }

    pub(super) fn partial_snapshot(
        &self,
        generation: u64,
        scanned_targets: usize,
        total_targets: usize,
    ) -> SearchResult {
        SearchResult {
            generation,
            matches: self.matches.clone(),
            result_groups: self.result_groups.clone(),
            displayed_match_count: self.match_count(),
            status: SearchStatus::Searching {
                scanned_targets,
                total_targets,
            },
        }
    }

    pub(super) fn match_count(&self) -> usize {
        self.matches.len()
    }
}

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
            .map(|search_match| {
                Ok((
                    search_match.range.clone(),
                    replacement_for_match(search_match)?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let expected_matches = matches[start..end]
            .iter()
            .rev()
            .map(|search_match| {
                (
                    search_match.range.clone(),
                    search_match.matched_text.clone(),
                )
            })
            .collect();

        targets.push(ReplacementTargetPlan {
            tab_index: current_match.tab_index,
            view_id: current_match.view_id,
            buffer_id: current_match.buffer_id,
            buffer_label: current_match.buffer_label.clone(),
            target_revision: current_match.target_revision,
            expected_matches,
            replacements,
        });
        start = end;
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

fn ordered_unique_file_identities(tab: &WorkspaceTab) -> Vec<SearchFileIdentity> {
    let mut seen_files = HashSet::with_capacity(tab.views.len());
    let mut ordered_files = Vec::with_capacity(tab.views.len());
    for view in &tab.views {
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
    use super::*;
    use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
    use std::path::PathBuf;

    #[test]
    fn accumulator_groups_all_matches_without_eager_preview_entries() {
        let text = (0..280).map(|_| "plan").collect::<Vec<_>>().join("\n");
        let ranges = text
            .match_indices("plan")
            .map(|(start, value)| start..start + value.len())
            .collect::<Vec<_>>();
        let tab = WorkspaceTab::new(BufferState::new("many.md".to_owned(), text, None));
        let target =
            build_search_target(0, &tab, tab.active_view_id, "many.md", None).expect("target");
        let mut accumulator = SearchResultAccumulator::default();

        accumulator.push_target_matches(&target, &ranges);
        let result = accumulator.finish(1);

        assert_eq!(result.matches.len(), 280);
        assert_eq!(result.displayed_match_count, 280);
        assert_eq!(result.result_groups.len(), 1);
        assert_eq!(result.result_groups[0].first_match_index, 0);
        assert_eq!(result.result_groups[0].total_match_count, 280);
    }

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
            collect_search_targets_for_views(0, &tab, "tab", None, None, tab.views.iter());

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].buffer_id, tab.buffer.id);
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
            collect_search_targets_for_views(0, &tab, "tab", None, None, tab.views.iter());

        assert_eq!(targets.len(), 1);
    }
}
