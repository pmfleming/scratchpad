use super::{SearchMatch, SearchResult, SearchResultGroup, SearchStatus, SearchTargetSnapshot};
use crate::app::domain::BufferId;
use std::{collections::HashMap, ops::Range};

#[derive(Default)]
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
        if ranges.is_empty() {
            return;
        }
        let buffer_label: std::sync::Arc<str> = target.buffer_label.as_str().into();
        let start_index = self.matches.len();
        self.matches
            .extend(ranges.iter().cloned().map(|range| SearchMatch {
                tab_index: target.tab_index,
                view_id: target.view_id,
                buffer_id: target.buffer_id,
                buffer_label: buffer_label.clone(),
                target_revision: target.document_snapshot.revision(),
                matched_text: None,
                range,
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
        let status = if self.matches.is_empty() {
            SearchStatus::NoMatches
        } else {
            SearchStatus::Ready
        };
        SearchResult {
            generation,
            matches: self.matches,
            result_groups: self.result_groups,
            displayed_match_count,
            status,
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

#[cfg(test)]
mod tests {
    use super::{SearchResultAccumulator, SearchStatus};
    use crate::app::app_state::search_state::helpers::build_search_target;
    use crate::app::domain::{BufferState, WorkspaceTab};
    #[test]
    fn accumulator_groups_all_matches_without_eager_preview_entries() {
        let text = (0..280).map(|_| "plan").collect::<Vec<_>>().join("\n");
        let ranges = text
            .match_indices("plan")
            .map(|(start, value)| start..start + value.len())
            .collect::<Vec<_>>();
        let tab = WorkspaceTab::new(BufferState::new("many.md".to_owned(), text, None));
        let target = build_search_target(0, &tab, tab.layout.active_view_id, "many.md", None)
            .expect("target");
        let mut accumulator = SearchResultAccumulator::default();

        accumulator.push_target_matches(&target, &[]);
        assert!(accumulator.result_groups.is_empty());
        assert!(matches!(
            SearchResultAccumulator::default().finish(1).status,
            SearchStatus::NoMatches
        ));
        accumulator.push_target_matches(&target, &ranges);
        let result = accumulator.finish(1);

        assert!(matches!(result.status, SearchStatus::Ready));
        assert_eq!(result.matches.len(), 280);
        assert_eq!(result.displayed_match_count, 280);
        assert_eq!(result.result_groups.len(), 1);
        assert_eq!(result.result_groups[0].first_match_index, 0);
        assert_eq!(result.result_groups[0].total_match_count, 280);
    }
}
