use super::{SearchRequest, SearchResult, SearchTargetSnapshot};
use crate::app::app_state::search_state::SearchStatus;
use crate::app::app_state::search_state::fragments::search_target_ranges;
use crate::app::app_state::search_state::helpers::SearchResultAccumulator;
use crate::app::services::search::SearchProgram;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

const SEARCH_TARGET_PARALLELISM_CAP: usize = 4;
const SEARCH_TARGET_PARALLELISM_MIN_TARGETS: usize = 4;
const SEARCH_TARGET_PARALLELISM_MIN_CHARS: usize = 8 * 1024 * 1024;
const INTRA_BUFFER_PARALLELISM_CAP: usize = 4;

struct TargetSearchOutcome {
    target_index: usize,
    target: SearchTargetSnapshot,
    ranges: Vec<Range<usize>>,
}

pub(crate) fn process_search_request(
    request: SearchRequest,
    latest_generation: &AtomicU64,
) -> Option<SearchResult> {
    process_search_request_with_partials(request, latest_generation, None)
}

pub(crate) fn process_search_request_with_partials(
    request: SearchRequest,
    latest_generation: &AtomicU64,
    mut partial_emit: Option<&mut dyn FnMut(SearchResult)>,
) -> Option<SearchResult> {
    let generation = request.generation;
    let total_targets = request.targets.len();
    let program = match SearchProgram::compile(&request.query, request.options) {
        Ok(program) => program,
        Err(error) => return Some(invalid_query_result(generation, error.message())),
    };

    let target_count = request.targets.len();
    let total_target_chars = request
        .targets
        .iter()
        .map(|target| target.document_snapshot.len_chars())
        .sum::<usize>();
    let mut results = SearchResultAccumulator::default();
    if search_target_parallelism(target_count, total_target_chars) <= 1 {
        process_search_targets_serial(
            request,
            &program,
            latest_generation,
            &mut results,
            total_targets,
            partial_emit,
        )?;
    } else {
        process_search_targets_parallel(
            request,
            program,
            latest_generation,
            &mut results,
            total_targets,
            &mut partial_emit,
        )?;
    }

    Some(final_search_result(results, generation))
}

fn process_search_targets_serial(
    request: SearchRequest,
    program: &SearchProgram,
    latest_generation: &AtomicU64,
    results: &mut SearchResultAccumulator,
    total_targets: usize,
    mut partial_emit: Option<&mut dyn FnMut(SearchResult)>,
) -> Option<()> {
    let SearchRequest {
        generation,
        targets,
        ..
    } = request;
    let target_count = targets.len();
    let intra_parallelism = intra_buffer_parallelism();
    let mut last_emitted_match_count = 0usize;

    for (index, target) in targets.into_iter().enumerate() {
        if latest_generation.load(Ordering::Relaxed) != generation {
            return None;
        }
        let ranges = search_target_ranges(
            &target.document_snapshot,
            target.search_range.clone(),
            program,
            generation,
            latest_generation,
            intra_parallelism,
        )?;
        push_target_matches(results, &target, &ranges);

        if let Some(emit) = partial_emit.as_deref_mut()
            && index + 1 < target_count
            && latest_generation.load(Ordering::Relaxed) == generation
            && should_publish_partial(results.match_count(), last_emitted_match_count, index)
        {
            emit(results.partial_snapshot(generation, index + 1, total_targets));
            last_emitted_match_count = results.match_count();
        }
    }
    Some(())
}

fn process_search_targets_parallel(
    request: SearchRequest,
    program: SearchProgram,
    latest_generation: &AtomicU64,
    results: &mut SearchResultAccumulator,
    total_targets: usize,
    partial_emit: &mut Option<&mut dyn FnMut(SearchResult)>,
) -> Option<()> {
    let SearchRequest {
        generation,
        targets,
        ..
    } = request;
    let target_count = targets.len();
    let total_target_chars = targets
        .iter()
        .map(|target| target.document_snapshot.len_chars())
        .sum::<usize>();
    let worker_count = search_target_parallelism(target_count, total_target_chars);
    let indexed_targets = targets.into_iter().enumerate().collect::<Vec<_>>();
    let program = Arc::new(program);
    let chunk_size = indexed_targets.len().div_ceil(worker_count);
    let mut indexed_iter = indexed_targets.into_iter();
    let (outcome_tx, outcome_rx) = mpsc::channel::<TargetSearchOutcome>();
    let stale = AtomicBool::new(false);
    let mut last_emitted_match_count = 0usize;

    thread::scope(|scope| -> Option<()> {
        for _ in 0..worker_count {
            let chunk = indexed_iter.by_ref().take(chunk_size).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            let program = program.clone();
            let tx = outcome_tx.clone();
            let stale_ref = &stale;
            scope.spawn(move || {
                search_target_chunk(chunk, program, latest_generation, generation, stale_ref, tx);
            });
        }
        drop(outcome_tx);

        collect_parallel_outcomes(
            outcome_rx,
            results,
            generation,
            total_targets,
            target_count,
            &mut last_emitted_match_count,
            partial_emit,
        );

        if stale.load(Ordering::Relaxed) && latest_generation.load(Ordering::Relaxed) != generation
        {
            return None;
        }
        Some(())
    })
}

fn search_target_chunk(
    chunk: Vec<(usize, SearchTargetSnapshot)>,
    program: Arc<SearchProgram>,
    latest_generation: &AtomicU64,
    generation: u64,
    stale: &AtomicBool,
    outcome_tx: mpsc::Sender<TargetSearchOutcome>,
) {
    for (target_index, target) in chunk {
        if stale.load(Ordering::Relaxed) {
            return;
        }
        if latest_generation.load(Ordering::Relaxed) != generation {
            stale.store(true, Ordering::Relaxed);
            return;
        }
        let Some(ranges) = search_target_ranges(
            &target.document_snapshot,
            target.search_range.clone(),
            &program,
            generation,
            latest_generation,
            1,
        ) else {
            stale.store(true, Ordering::Relaxed);
            return;
        };
        if outcome_tx
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
}

fn collect_parallel_outcomes(
    outcome_rx: mpsc::Receiver<TargetSearchOutcome>,
    results: &mut SearchResultAccumulator,
    generation: u64,
    total_targets: usize,
    target_count: usize,
    last_emitted_match_count: &mut usize,
    partial_emit: &mut Option<&mut dyn FnMut(SearchResult)>,
) {
    let mut next_index = 0usize;
    let mut pending: HashMap<usize, TargetSearchOutcome> = HashMap::new();
    while let Ok(outcome) = outcome_rx.recv() {
        pending.insert(outcome.target_index, outcome);
        while let Some(outcome) = pending.remove(&next_index) {
            push_target_matches(results, &outcome.target, &outcome.ranges);
            next_index += 1;
            if let Some(emit) = partial_emit.as_deref_mut()
                && next_index < target_count
                && should_publish_partial(
                    results.match_count(),
                    *last_emitted_match_count,
                    next_index.saturating_sub(1),
                )
            {
                emit(results.partial_snapshot(generation, next_index, total_targets));
                *last_emitted_match_count = results.match_count();
            }
        }
    }
}

fn push_target_matches(
    results: &mut SearchResultAccumulator,
    target: &SearchTargetSnapshot,
    ranges: &[Range<usize>],
) {
    if !ranges.is_empty() {
        results.push_target_matches(target, ranges);
    }
}

fn invalid_query_result(generation: u64, message: &str) -> SearchResult {
    SearchResult {
        generation,
        matches: Vec::new(),
        result_groups: Vec::new(),
        displayed_match_count: 0,
        status: SearchStatus::InvalidQuery(message.to_owned()),
    }
}

fn final_search_result(results: SearchResultAccumulator, generation: u64) -> SearchResult {
    let mut result = results.finish(generation);
    result.status = if result.matches.is_empty() {
        SearchStatus::NoMatches
    } else {
        SearchStatus::Ready
    };
    result
}

fn should_publish_partial(
    current_match_count: usize,
    last_emitted_match_count: usize,
    completed_target_index: usize,
) -> bool {
    const PARTIAL_MATCH_DELTA: usize = 256;
    completed_target_index == 0
        || current_match_count >= last_emitted_match_count.saturating_add(PARTIAL_MATCH_DELTA)
}

fn search_target_parallelism(target_count: usize, total_target_chars: usize) -> usize {
    if target_count < SEARCH_TARGET_PARALLELISM_MIN_TARGETS
        || total_target_chars < SEARCH_TARGET_PARALLELISM_MIN_CHARS
    {
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
