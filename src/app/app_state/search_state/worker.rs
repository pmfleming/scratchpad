use super::{SearchMatch, SearchResultEntry, SearchResultGroup, SearchStatus};
use crate::app::domain::buffer::PieceTreeLite;
use crate::app::domain::{BufferId, ViewId};
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
    pub(super) search_text: String,
    pub(super) search_offset: usize,
    pub(super) preview_tree: PieceTreeLite,
}

#[derive(Default)]
struct SearchResultAccumulator {
    matches: Vec<SearchMatch>,
    result_groups: Vec<SearchResultGroup>,
    group_lookup: HashMap<(usize, BufferId), usize>,
    displayed_match_count: usize,
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

        let mut entries = Vec::with_capacity(ranges.len().min(remaining_capacity));
        for (offset, range) in ranges.iter().take(remaining_capacity).enumerate() {
            let (line_number, column_number, preview) =
                target.preview_tree.preview_for_match(range);
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
    let mut results = SearchResultAccumulator::default();

    for target in request.targets {
        let outcome = search::search_text_interruptible(
            &target.search_text,
            &request.query,
            request.options,
            || latest_generation.load(Ordering::Relaxed) == request.generation,
        )?;
        if let Some(error) = outcome.error {
            return Some(SearchResult {
                generation: request.generation,
                matches: Vec::new(),
                result_groups: Vec::new(),
                displayed_match_count: 0,
                status: SearchStatus::InvalidQuery(error.message().to_owned()),
            });
        }
        let ranges = outcome
            .matches
            .into_iter()
            .map(|range| range.start + target.search_offset..range.end + target.search_offset)
            .collect::<Vec<_>>();
        if ranges.is_empty() {
            continue;
        }
        results.push_target_matches(&target, &ranges);
    }

    let mut result = results.finish(request.generation);
    result.status = if result.matches.is_empty() {
        SearchStatus::NoMatches
    } else {
        SearchStatus::Ready
    };
    Some(result)
}
