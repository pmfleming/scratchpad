use super::results::SearchResultAccumulator;
use super::{SearchRequest, SearchResult, SearchTargetSnapshot};
use crate::app::app_state::search_state::SearchStatus;
use crate::app::app_state::search_state::fragments::{
    intra_buffer_parallelism, search_target_ranges,
};
use crate::app::services::search::SearchProgram;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;

use crate::app::app_state::search_state::SEARCH_RESULT_LIMIT_MESSAGE;

const SEARCH_TARGET_PARALLELISM_CAP: usize = 8;
const SEARCH_TARGET_PARALLELISM_MIN_TARGETS: usize = 4;

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
        Ok(program) => program.with_result_limit(request.result_limit),
        Err(error) => return Some(invalid_query_result(generation, error.message())),
    };

    let overflow_targets = (request.result_limit != usize::MAX).then(|| request.targets.clone());
    let limit = request.result_limit;
    let target_count = request.targets.len();
    let mut results = SearchResultAccumulator::default();
    let processed = if search_target_parallelism(target_count) <= 1 {
        process_search_targets_serial(
            request,
            &program,
            latest_generation,
            &mut results,
            total_targets,
            partial_emit,
        )
    } else {
        process_search_targets_parallel(
            request,
            &program,
            latest_generation,
            &mut results,
            total_targets,
            &mut partial_emit,
        )
    };
    if latest_generation.load(Ordering::Relaxed) != generation {
        return None;
    }
    if program.result_limit_exceeded() {
        let program = SearchProgram::compile(program.query(), program.options()).ok()?;
        let mut previews = SearchResultAccumulator::default();
        let mut remaining = limit;
        for target in overflow_targets.unwrap_or_default() {
            if remaining == 0 {
                break;
            }
            let ranges = super::super::fragments::search_target_prefix(
                &target.document_snapshot,
                target.search_range.clone(),
                &program,
                generation,
                latest_generation,
                remaining,
            )?;
            remaining -= ranges.len();
            previews.push_target_matches(&target, &ranges);
        }
        let mut result = previews.finish(generation);
        result.status = SearchStatus::Error(SEARCH_RESULT_LIMIT_MESSAGE.into());
        return Some(result);
    }
    processed?;

    (latest_generation.load(Ordering::Relaxed) == generation).then(|| results.finish(generation))
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
        if !program.reserve_result_matches(ranges.len()) {
            return Some(());
        }
        results.push_target_matches(&target, &ranges);

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
    program: &SearchProgram,
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
    let worker_count = search_target_parallelism(target_count);
    let chunk_size = target_count.div_ceil(worker_count);
    let mut indexed_iter = targets.into_iter().enumerate();
    let (outcome_tx, outcome_rx) = mpsc::channel::<TargetSearchOutcome>();
    let stale = AtomicBool::new(false);
    let mut last_emitted_match_count = 0usize;

    thread::scope(|scope| -> Option<()> {
        for _ in 0..worker_count {
            let chunk = indexed_iter.by_ref().take(chunk_size).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
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
    program: &SearchProgram,
    latest_generation: &AtomicU64,
    generation: u64,
    stale: &AtomicBool,
    outcome_tx: mpsc::Sender<TargetSearchOutcome>,
) {
    for (target_index, target) in chunk {
        if stale.load(Ordering::Relaxed) || program.result_limit_exceeded() {
            return;
        }
        if latest_generation.load(Ordering::Relaxed) != generation {
            stale.store(true, Ordering::Relaxed);
            return;
        }
        let Some(ranges) = search_target_ranges(
            &target.document_snapshot,
            target.search_range.clone(),
            program,
            generation,
            latest_generation,
            1,
        ) else {
            stale.store(true, Ordering::Relaxed);
            return;
        };
        if !program.reserve_result_matches(ranges.len()) {
            return;
        }
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
            results.push_target_matches(&outcome.target, &outcome.ranges);
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

fn invalid_query_result(generation: u64, message: &str) -> SearchResult {
    SearchResult {
        generation,
        matches: Vec::new(),
        result_groups: Vec::new(),
        displayed_match_count: 0,
        status: SearchStatus::InvalidQuery(message.to_owned()),
    }
}

fn should_publish_partial(
    current_match_count: usize,
    last_emitted_match_count: usize,
    completed_target_index: usize,
) -> bool {
    const MINIMUM_PARTIAL_MATCH_DELTA: usize = 64;
    // Each partial owns a complete prefix. Geometric growth bounds cumulative
    // cloning to linear work while preserving the first available match and final result.
    completed_target_index == 0
        || (last_emitted_match_count == 0 && current_match_count > 0)
        || current_match_count
            >= last_emitted_match_count
                .saturating_add(last_emitted_match_count.max(MINIMUM_PARTIAL_MATCH_DELTA))
}

fn search_target_parallelism(target_count: usize) -> usize {
    if target_count < SEARCH_TARGET_PARALLELISM_MIN_TARGETS {
        return 1;
    }

    thread::available_parallelism()
        .map_or(1, |parallelism| {
            parallelism.get().min(SEARCH_TARGET_PARALLELISM_CAP)
        })
        .min(target_count)
}

#[cfg(test)]
mod partial_tests {
    use super::*;
    use crate::app::app_state::search_state::worker::SearchFileIdentity;
    use crate::app::domain::TextDocument;
    use crate::app::services::search::SearchOptions;

    fn request(count: usize) -> SearchRequest {
        let text = TextDocument::new("needle ".repeat(50)).snapshot();
        let empty = TextDocument::new("no matches".into()).snapshot();
        SearchRequest {
            result_limit: crate::app::app_state::search_state::MAX_SEARCH_RESULTS,
            generation: 1,
            query: "needle".into(),
            options: SearchOptions::default(),
            targets: (0..count)
                .map(|index| SearchTargetSnapshot {
                    file_identity: SearchFileIdentity::Untitled(index as u64),
                    tab_index: index,
                    view_id: index as u64,
                    buffer_id: index as u64,
                    tab_label: format!("tab-{index}"),
                    buffer_label: format!("file-{index}"),
                    document_snapshot: if index == 0 {
                        empty.clone()
                    } else {
                        text.clone()
                    },
                    search_range: None,
                })
                .collect(),
        }
    }

    #[test]
    fn streamed_prefixes_deliver_first_matches_and_bound_copying_in_both_paths() {
        for count in [3, 200] {
            let current = AtomicU64::new(1);
            let mut prefixes = Vec::new();
            let result = process_search_request_with_partials(
                request(count),
                &current,
                Some(&mut |result| {
                    let SearchStatus::Searching {
                        scanned_targets, ..
                    } = result.status
                    else {
                        panic!("partial status");
                    };
                    assert_eq!(result.matches.len(), scanned_targets.saturating_sub(1) * 50);
                    prefixes.push((scanned_targets, result.matches.len()));
                }),
            )
            .unwrap();
            assert_eq!(
                prefixes.iter().find(|(_, matches)| *matches > 0),
                Some(&(2, 50))
            );
            assert_eq!(result.matches.len(), (count - 1) * 50);
            assert!(
                prefixes.iter().map(|(_, matches)| matches).sum::<usize>()
                    <= 2 * result.matches.len()
            );
            assert!(matches!(result.status, SearchStatus::Ready));
            for (index, matched) in result.matches.iter().enumerate() {
                assert_eq!(matched.tab_index, 1 + index / 50);
                assert_eq!(matched.range, (index % 50) * 7..(index % 50) * 7 + 6);
            }
        }
    }

    #[test]
    fn cancelling_after_a_partial_does_not_publish_a_completed_result() {
        let current = AtomicU64::new(1);
        assert!(
            process_search_request_with_partials(
                request(200),
                &current,
                Some(&mut |result| {
                    if !result.matches.is_empty() {
                        current.store(2, Ordering::Relaxed);
                    }
                })
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use crate::app::app_state::search_state::worker::SearchFileIdentity;
    use crate::app::domain::TextDocument;
    use crate::app::services::search::SearchOptions;

    #[test]
    fn recommended_limit_bounds_previews_and_unlimited_preserves_all_matches() {
        use crate::app::app_state::search_state::MAX_SEARCH_RESULTS;
        for (count, limit) in [
            (1, MAX_SEARCH_RESULTS),
            (4, MAX_SEARCH_RESULTS),
            (1, usize::MAX),
            (4, usize::MAX),
        ] {
            let document =
                TextDocument::new("needle ".repeat(MAX_SEARCH_RESULTS / count + 1)).snapshot();
            let request = SearchRequest {
                result_limit: limit,
                generation: 1,
                query: "needle".into(),
                options: SearchOptions::default(),
                targets: (0..count)
                    .map(|index| SearchTargetSnapshot {
                        file_identity: SearchFileIdentity::Untitled(index as u64),
                        tab_index: index,
                        view_id: index as u64,
                        buffer_id: index as u64,
                        tab_label: "tab".into(),
                        buffer_label: "buffer".into(),
                        document_snapshot: document.clone(),
                        search_range: None,
                    })
                    .collect(),
            };
            let result = process_search_request_with_partials(
                request,
                &AtomicU64::new(1),
                Some(&mut |partial| assert!(partial.matches.len() <= limit)),
            )
            .unwrap();
            if limit == usize::MAX {
                assert!(matches!(result.status, SearchStatus::Ready));
                assert_eq!(
                    result.matches.len(),
                    (MAX_SEARCH_RESULTS / count + 1) * count
                );
            } else {
                assert!(matches!(result.status, SearchStatus::Error(_)));
                assert_eq!(result.matches.len(), limit);
                assert!(!result.result_groups.is_empty());
            }
        }
    }
}
