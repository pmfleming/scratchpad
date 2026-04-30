use super::fragments::search_target_ranges;
use super::helpers::SearchResultAccumulator;
use super::{SearchMatch, SearchResultGroup, SearchStatus};
use crate::app::capacity_metrics;
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
use std::time::Instant;

const SEARCH_TARGET_PARALLELISM_CAP: usize = 4;
const SEARCH_TARGET_PARALLELISM_MIN_TARGETS: usize = 4;
const INTRA_BUFFER_PARALLELISM_CAP: usize = 4;

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

struct TargetSearchOutcome {
    target_index: usize,
    target: SearchTargetSnapshot,
    ranges: Vec<Range<usize>>,
}

pub(super) fn spawn_search_worker(
    latest_generation: Arc<AtomicU64>,
) -> (Sender<SearchRequest>, Receiver<SearchResult>) {
    let (request_tx, request_rx) = mpsc::channel::<SearchRequest>();
    let (result_tx, result_rx) = mpsc::channel::<SearchResult>();
    thread::spawn(move || {
        while let Ok(mut request) = request_rx.recv() {
            let mut coalesced_queue_depth = 1usize;
            while let Ok(next_request) = request_rx.try_recv() {
                request = next_request;
                coalesced_queue_depth += 1;
            }
            capacity_metrics::record_search_request(request.targets.len(), coalesced_queue_depth);
            let started_at = Instant::now();
            let partial_tx = result_tx.clone();
            let mut partial_failed = false;
            let mut partial_emit = move |partial: SearchResult| {
                if partial_failed {
                    return;
                }
                if partial_tx.send(partial).is_err() {
                    partial_failed = true;
                }
            };
            if let Some(result) = process_search_request_with_partials(
                request,
                &latest_generation,
                Some(&mut partial_emit),
            ) && result_tx.send(result).is_err()
            {
                break;
            }
            capacity_metrics::record_search_worker_active(started_at.elapsed());
        }
    });
    (request_tx, result_rx)
}

pub(super) fn process_search_request(
    request: SearchRequest,
    latest_generation: &AtomicU64,
) -> Option<SearchResult> {
    process_search_request_with_partials(request, latest_generation, None)
}

pub(super) fn process_search_request_with_partials(
    request: SearchRequest,
    latest_generation: &AtomicU64,
    mut partial_emit: Option<&mut dyn FnMut(SearchResult)>,
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

    let target_count = request.targets.len();
    let single_threaded = search_target_parallelism(target_count) <= 1;
    let mut results = SearchResultAccumulator::default();

    if single_threaded {
        // Stream partial cumulative results after each target finishes so the
        // UI can show the first useful matches before the full scan completes.
        let SearchRequest {
            generation,
            query,
            options,
            targets,
        } = request;
        let intra_parallelism = intra_buffer_parallelism();
        for (index, target) in targets.into_iter().enumerate() {
            if latest_generation.load(Ordering::Relaxed) != generation {
                return None;
            }
            let ranges = search_target_ranges(
                &target.document_snapshot,
                target.search_range.clone(),
                &query,
                options,
                generation,
                latest_generation,
                intra_parallelism,
            )?;
            if !ranges.is_empty() {
                results.push_target_matches(&target, &ranges);
            }
            // Skip partial after the very last target -- the caller will send
            // the final result immediately afterwards.
            if let Some(emit) = partial_emit.as_deref_mut()
                && index + 1 < target_count
                && latest_generation.load(Ordering::Relaxed) == generation
            {
                emit(results.partial_snapshot(generation));
            }
        }
    } else {
        let SearchRequest {
            generation,
            query,
            options,
            targets,
        } = request;
        let target_count = targets.len();
        let worker_count = search_target_parallelism(target_count);
        let indexed_targets = targets.into_iter().enumerate().collect::<Vec<_>>();
        let query_arc = Arc::<str>::from(query);
        let chunk_size = indexed_targets.len().div_ceil(worker_count);
        let mut indexed_iter = indexed_targets.into_iter();
        let (outcome_tx, outcome_rx) = mpsc::channel::<TargetSearchOutcome>();
        let stale = std::sync::atomic::AtomicBool::new(false);

        let stream_ok = thread::scope(|scope| -> Option<()> {
            for _ in 0..worker_count {
                let chunk = indexed_iter.by_ref().take(chunk_size).collect::<Vec<_>>();
                if chunk.is_empty() {
                    break;
                }
                let query = query_arc.clone();
                let tx = outcome_tx.clone();
                let stale_ref = &stale;
                scope.spawn(move || {
                    for (target_index, target) in chunk {
                        if stale_ref.load(Ordering::Relaxed) {
                            return;
                        }
                        if latest_generation.load(Ordering::Relaxed) != generation {
                            stale_ref.store(true, Ordering::Relaxed);
                            return;
                        }
                        let Some(ranges) = search_target_ranges(
                            &target.document_snapshot,
                            target.search_range.clone(),
                            &query,
                            options,
                            generation,
                            latest_generation,
                            1,
                        ) else {
                            stale_ref.store(true, Ordering::Relaxed);
                            return;
                        };
                        if tx
                            .send(TargetSearchOutcome {
                                target_index,
                                target,
                                ranges,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                });
            }
            // Drop the outer tx so the receiver terminates once all workers
            // have finished.
            drop(outcome_tx);

            let mut next_index = 0usize;
            let mut pending: HashMap<usize, TargetSearchOutcome> = HashMap::new();
            while let Ok(outcome) = outcome_rx.recv() {
                pending.insert(outcome.target_index, outcome);
                while let Some(outcome) = pending.remove(&next_index) {
                    if !outcome.ranges.is_empty() {
                        results.push_target_matches(&outcome.target, &outcome.ranges);
                    }
                    next_index += 1;
                    if let Some(emit) = partial_emit.as_deref_mut()
                        && next_index < target_count
                        && latest_generation.load(Ordering::Relaxed) == generation
                    {
                        emit(results.partial_snapshot(generation));
                    }
                }
            }

            if stale.load(Ordering::Relaxed)
                && latest_generation.load(Ordering::Relaxed) != generation
            {
                return None;
            }
            Some(())
        });
        stream_ok?;
    }

    let mut result = results.finish(generation);
    result.status = if result.matches.is_empty() {
        SearchStatus::NoMatches
    } else {
        SearchStatus::Ready
    };
    Some(result)
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

fn intra_buffer_parallelism() -> usize {
    thread::available_parallelism()
        .map(|p| p.get().min(INTRA_BUFFER_PARALLELISM_CAP))
        .unwrap_or(1)
        .max(1)
}
