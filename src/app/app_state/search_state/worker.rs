use super::{SearchMatch, SearchResultEntry, SearchResultGroup, SearchStatus};
use crate::app::domain::{BufferId, DocumentSnapshot, ViewId};
use crate::app::services::search::{self, SearchOptions};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::thread;

const SEARCH_RESULT_LIMIT: usize = 200;
const SEARCH_TARGET_PARALLELISM_CAP: usize = 4;
const SEARCH_TARGET_PARALLELISM_MIN_TARGETS: usize = 4;

pub(super) struct SearchRequest {
    pub(super) generation: u64,
    pub(super) query: String,
    pub(super) options: SearchOptions,
    pub(super) targets: Vec<SearchTargetSnapshot>,
}

pub(super) struct SearchResult {
    pub(super) generation: u64,
    pub(super) matches: Vec<SearchMatch>,
    pub(super) result_groups: Vec<SearchResultGroup>,
    pub(super) displayed_match_count: usize,
    pub(super) status: SearchStatus,
}

pub(super) struct SearchTargetSnapshot {
    pub(super) tab_index: usize,
    pub(super) view_id: ViewId,
    pub(super) buffer_id: BufferId,
    pub(super) tab_label: String,
    pub(super) buffer_label: String,
    pub(super) document_snapshot: DocumentSnapshot,
    pub(super) search_range: Option<Range<usize>>,
}

#[derive(Default)]
struct SearchResultAccumulator {
    matches: Vec<SearchMatch>,
    result_groups: Vec<SearchResultGroup>,
    group_lookup: HashMap<(usize, BufferId), usize>,
    displayed_match_count: usize,
}

struct TargetSearchOutcome {
    target_index: usize,
    target: SearchTargetSnapshot,
    ranges: Vec<Range<usize>>,
}

impl SearchResultAccumulator {
    fn push_target_matches(&mut self, target: &SearchTargetSnapshot, ranges: &[Range<usize>]) {
        let start_index = self.matches.len();
        self.matches
            .extend(ranges.iter().cloned().map(|range| SearchMatch {
                tab_index: target.tab_index,
                view_id: target.view_id,
                buffer_id: target.buffer_id,
                buffer_label: target.buffer_label.clone(),
                range,
            }));

        let entries = self.build_entries(target, ranges, start_index);
        if entries.is_empty() {
            return;
        }

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
                    total_match_count: 0,
                    entries: Vec::new(),
                    active: false,
                });
                self.group_lookup
                    .insert((target.tab_index, target.buffer_id), index);
                index
            };

        let group = &mut self.result_groups[group_index];
        group.total_match_count += ranges.len();
        group.entries.extend(entries);
    }

    fn build_entries(
        &mut self,
        target: &SearchTargetSnapshot,
        ranges: &[Range<usize>],
        start_index: usize,
    ) -> Vec<SearchResultEntry> {
        let remaining_capacity = SEARCH_RESULT_LIMIT.saturating_sub(self.displayed_match_count);
        if remaining_capacity == 0 {
            return Vec::new();
        }

        let preview_rows = target
            .document_snapshot
            .previews_for_matches(ranges, remaining_capacity);
        let mut entries = Vec::with_capacity(preview_rows.len());
        for (offset, (line_number, column_number, preview)) in preview_rows.into_iter().enumerate()
        {
            entries.push(SearchResultEntry {
                match_index: start_index + offset,
                buffer_id: target.buffer_id,
                buffer_label: target.buffer_label.clone(),
                line_number,
                column_number,
                preview,
                active: false,
            });
        }
        self.displayed_match_count += entries.len();
        entries
    }

    fn finish(self, generation: u64) -> SearchResult {
        SearchResult {
            generation,
            matches: self.matches,
            result_groups: self.result_groups,
            displayed_match_count: self.displayed_match_count,
            status: SearchStatus::NoMatches,
        }
    }
}

pub(super) fn spawn_search_worker(
    latest_generation: Arc<AtomicU64>,
) -> (Sender<SearchRequest>, Receiver<SearchResult>) {
    let (request_tx, request_rx) = mpsc::channel::<SearchRequest>();
    let (result_tx, result_rx) = mpsc::channel::<SearchResult>();
    thread::spawn(move || {
        while let Ok(mut request) = request_rx.recv() {
            while let Ok(next_request) = request_rx.try_recv() {
                request = next_request;
            }
            if let Some(result) = process_search_request(request, &latest_generation)
                && result_tx.send(result).is_err()
            {
                break;
            }
        }
    });
    (request_tx, result_rx)
}

pub(super) fn process_search_request(
    request: SearchRequest,
    latest_generation: &AtomicU64,
) -> Option<SearchResult> {
    let generation = request.generation;
    if let Some(error) = search::validate_search_query(&request.query, request.options) {
        return Some(SearchResult {
            generation,
            matches: Vec::new(),
            result_groups: Vec::new(),
            displayed_match_count: 0,
            status: SearchStatus::InvalidQuery(error.message().to_owned()),
        });
    }

    let target_results = process_search_targets(request, latest_generation)?;
    let mut results = SearchResultAccumulator::default();
    for target_result in target_results {
        if target_result.ranges.is_empty() {
            continue;
        }
        results.push_target_matches(&target_result.target, &target_result.ranges);
    }

    let mut result = results.finish(generation);
    result.status = if result.matches.is_empty() {
        SearchStatus::NoMatches
    } else {
        SearchStatus::Ready
    };
    Some(result)
}

fn process_search_targets(
    request: SearchRequest,
    latest_generation: &AtomicU64,
) -> Option<Vec<TargetSearchOutcome>> {
    let SearchRequest {
        generation,
        query,
        options,
        targets,
    } = request;
    let worker_count = search_target_parallelism(targets.len());
    let indexed_targets = targets.into_iter().enumerate().collect::<Vec<_>>();

    if worker_count <= 1 {
        return process_search_target_chunk(
            indexed_targets,
            generation,
            &query,
            options,
            latest_generation,
        );
    }

    let query = Arc::<str>::from(query);
    let chunk_size = indexed_targets.len().div_ceil(worker_count);
    let mut indexed_iter = indexed_targets.into_iter();
    let mut results = Vec::new();

    thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..worker_count {
            let chunk = indexed_iter.by_ref().take(chunk_size).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            let query = query.clone();
            handles.push(scope.spawn(move || {
                process_search_target_chunk(chunk, generation, &query, options, latest_generation)
            }));
        }

        for handle in handles {
            let mut chunk_results = handle.join().expect("search target worker panicked")?;
            results.append(&mut chunk_results);
        }
        Some(())
    })?;

    results.sort_by_key(|result| result.target_index);
    Some(results)
}

fn process_search_target_chunk(
    indexed_targets: Vec<(usize, SearchTargetSnapshot)>,
    generation: u64,
    query: &str,
    options: SearchOptions,
    latest_generation: &AtomicU64,
) -> Option<Vec<TargetSearchOutcome>> {
    let mut outcomes = Vec::with_capacity(indexed_targets.len());

    for (target_index, target) in indexed_targets {
        if latest_generation.load(Ordering::Relaxed) != generation {
            return None;
        }

        let (search_text, search_offset) = target
            .document_snapshot
            .search_text_cow(target.search_range.clone());
        let outcome =
            search::search_text_interruptible(search_text.as_ref(), query, options, || {
                latest_generation.load(Ordering::Relaxed) == generation
            })?;
        debug_assert!(outcome.error.is_none());
        let ranges = outcome
            .matches
            .into_iter()
            .map(|range| range.start + search_offset..range.end + search_offset)
            .collect::<Vec<_>>();
        outcomes.push(TargetSearchOutcome {
            target_index,
            target,
            ranges,
        });
    }

    Some(outcomes)
}

fn search_target_parallelism(target_count: usize) -> usize {
    if target_count < SEARCH_TARGET_PARALLELISM_MIN_TARGETS {
        return 1;
    }

    thread::available_parallelism()
        .map(|parallelism| parallelism.get().min(SEARCH_TARGET_PARALLELISM_CAP))
        .unwrap_or(1)
        .min(target_count)
}

#[cfg(test)]
mod tests {
    use super::{SearchRequest, SearchTargetSnapshot, process_search_request};
    use crate::app::app_state::SearchStatus;
    use crate::app::domain::{BufferState, DocumentSnapshot};
    use crate::app::services::search::SearchOptions;
    use std::sync::atomic::AtomicU64;

    fn snapshot(text: &str) -> DocumentSnapshot {
        BufferState::new("search.txt".to_owned(), text.to_owned(), None).document_snapshot()
    }

    #[test]
    fn process_search_request_preserves_target_order_with_parallel_fanout() {
        let request = SearchRequest {
            generation: 1,
            query: "needle".to_owned(),
            options: SearchOptions::default(),
            targets: (0..8)
                .map(|index| SearchTargetSnapshot {
                    tab_index: 0,
                    view_id: index as u64 + 1,
                    buffer_id: index as u64 + 1,
                    tab_label: "Tab 1".to_owned(),
                    buffer_label: format!("buffer_{index}.txt"),
                    document_snapshot: snapshot(&format!("needle {index}\nneedle {index}")),
                    search_range: None,
                })
                .collect(),
        };

        let latest_generation = AtomicU64::new(1);
        let result = process_search_request(request, &latest_generation).expect("search result");

        assert_eq!(result.status, SearchStatus::Ready);
        assert_eq!(result.matches.len(), 16);
        assert_eq!(result.result_groups.len(), 8);
        assert_eq!(result.result_groups[0].buffer_label, "buffer_0.txt");
        assert_eq!(result.result_groups[7].buffer_label, "buffer_7.txt");
    }
}
