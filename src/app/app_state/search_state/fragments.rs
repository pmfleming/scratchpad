use crate::app::capacity_metrics;
use crate::app::domain::DocumentSnapshot;
use crate::app::domain::buffer::DocumentChunk;
use crate::app::services::search::{self, SearchMode, SearchOptions};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

pub(super) const SEARCH_FRAGMENT_CHUNK_CHARS: usize = 64 * 1024;
const INTRA_BUFFER_PARALLELISM_MIN_CHUNKS: usize = 4;

#[allow(clippy::too_many_arguments)]
pub(super) fn search_target_ranges(
    snapshot: &DocumentSnapshot,
    search_range: Option<Range<usize>>,
    query: &str,
    options: SearchOptions,
    generation: u64,
    latest_generation: &AtomicU64,
    intra_parallelism: usize,
) -> Option<Vec<Range<usize>>> {
    let normalized = search_range
        .map(|range| snapshot.normalize_char_range(range))
        .unwrap_or(0..snapshot.document_length().chars);

    if let Some(text) = snapshot.piece_tree().borrow_range(normalized.clone()) {
        let outcome = search::search_text_interruptible(text, query, options, || {
            latest_generation.load(Ordering::Relaxed) == generation
        })?;
        debug_assert!(outcome.error.is_none());
        return Some(
            outcome
                .matches
                .into_iter()
                .map(|range| range.start + normalized.start..range.end + normalized.start)
                .collect(),
        );
    }

    if options.mode == SearchMode::PlainText {
        return search_fragmented_plain_text(
            snapshot,
            normalized,
            query,
            options,
            generation,
            latest_generation,
            intra_parallelism,
        );
    }

    let max_match_chars = search::regex_max_match_chars(query)
        .expect("unbounded regex queries should be rejected during validation");
    search_fragmented_bounded_regex(
        snapshot,
        normalized,
        query,
        options,
        max_match_chars,
        generation,
        latest_generation,
        intra_parallelism,
    )
}

#[allow(clippy::too_many_arguments)]
fn search_fragmented_plain_text(
    snapshot: &DocumentSnapshot,
    range: Range<usize>,
    query: &str,
    options: SearchOptions,
    generation: u64,
    latest_generation: &AtomicU64,
    intra_parallelism: usize,
) -> Option<Vec<Range<usize>>> {
    if range.is_empty() || query.is_empty() {
        return Some(Vec::new());
    }

    let query_chars = query.chars().count().max(1);
    let overlap_chars = query_chars + usize::from(options.whole_word);
    let chunk_chars = SEARCH_FRAGMENT_CHUNK_CHARS.max(query_chars.saturating_mul(4));
    let chunks = snapshot.chunks_for_range(range, chunk_chars, overlap_chars, overlap_chars);
    capacity_metrics::record_search_chunks(chunks.len());

    process_chunks_concurrent(
        chunks,
        intra_parallelism,
        generation,
        latest_generation,
        |chunk| {
            let (window_text, window_offset) =
                snapshot.search_text_cow(Some(chunk.window_range.clone()));
            let outcome =
                search::search_text_interruptible(window_text.as_ref(), query, options, || {
                    latest_generation.load(Ordering::Relaxed) == generation
                })?;
            debug_assert!(outcome.error.is_none());
            Some(
                outcome
                    .matches
                    .into_iter()
                    .filter_map(|matched| {
                        let global_start = window_offset + matched.start;
                        let global_end = window_offset + matched.end;
                        (global_start >= chunk.core_range.start
                            && global_start < chunk.core_range.end)
                            .then_some(global_start..global_end)
                    })
                    .collect(),
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn search_fragmented_bounded_regex(
    snapshot: &DocumentSnapshot,
    range: Range<usize>,
    query: &str,
    options: SearchOptions,
    max_match_chars: usize,
    generation: u64,
    latest_generation: &AtomicU64,
    intra_parallelism: usize,
) -> Option<Vec<Range<usize>>> {
    if range.is_empty() || query.is_empty() {
        return Some(Vec::new());
    }

    let context_chars = 1 + usize::from(options.whole_word);
    let overlap_chars = max_match_chars.saturating_add(context_chars);
    let chunk_chars = SEARCH_FRAGMENT_CHUNK_CHARS.max(overlap_chars.max(1));
    let range_end = range.end;
    let chunks = snapshot.chunks_for_range(range, chunk_chars, context_chars, overlap_chars);
    capacity_metrics::record_search_chunks(chunks.len());

    process_chunks_concurrent(
        chunks,
        intra_parallelism,
        generation,
        latest_generation,
        |chunk| {
            let (window_text, window_offset) =
                snapshot.search_text_cow(Some(chunk.window_range.clone()));
            let outcome =
                search::search_text_interruptible(window_text.as_ref(), query, options, || {
                    latest_generation.load(Ordering::Relaxed) == generation
                })?;
            debug_assert!(outcome.error.is_none());
            Some(
                outcome
                    .matches
                    .into_iter()
                    .filter_map(|matched| {
                        let global_start = window_offset + matched.start;
                        let global_end = window_offset + matched.end;
                        ((global_start >= chunk.core_range.start
                            && global_start < chunk.core_range.end)
                            || (chunk.core_range.end == range_end && global_start == range_end))
                            .then_some(global_start..global_end)
                    })
                    .collect(),
            )
        },
    )
}

fn process_chunks_concurrent(
    chunks: Vec<DocumentChunk>,
    intra_parallelism: usize,
    generation: u64,
    latest_generation: &AtomicU64,
    process: impl Fn(&DocumentChunk) -> Option<Vec<Range<usize>>> + Sync,
) -> Option<Vec<Range<usize>>> {
    let chunk_count = chunks.len();
    if chunk_count == 0 {
        return Some(Vec::new());
    }

    let workers = intra_parallelism.min(chunk_count).max(1);
    if workers == 1 || chunk_count < INTRA_BUFFER_PARALLELISM_MIN_CHUNKS {
        let mut matches = Vec::new();
        for chunk in &chunks {
            if latest_generation.load(Ordering::Relaxed) != generation {
                return None;
            }
            matches.extend(process(chunk)?);
        }
        return Some(matches);
    }

    capacity_metrics::record_search_intra_buffer_workers(workers);

    let stale = AtomicBool::new(false);
    let chunk_size = chunk_count.div_ceil(workers);
    let chunks_ref: &[DocumentChunk] = &chunks;
    let process_ref = &process;
    let stale_ref = &stale;
    let mut per_worker: Vec<Vec<Vec<Range<usize>>>> = Vec::with_capacity(workers);

    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker_idx in 0..workers {
            let start = worker_idx * chunk_size;
            if start >= chunk_count {
                break;
            }
            let end = (start + chunk_size).min(chunk_count);
            let h = scope.spawn(move || -> Vec<Vec<Range<usize>>> {
                let mut local: Vec<Vec<Range<usize>>> = Vec::with_capacity(end - start);
                for chunk in &chunks_ref[start..end] {
                    if stale_ref.load(Ordering::Relaxed) {
                        return local;
                    }
                    if latest_generation.load(Ordering::Relaxed) != generation {
                        stale_ref.store(true, Ordering::Relaxed);
                        return local;
                    }
                    match process_ref(chunk) {
                        Some(matches) => local.push(matches),
                        None => {
                            stale_ref.store(true, Ordering::Relaxed);
                            return local;
                        }
                    }
                }
                local
            });
            handles.push(h);
        }
        for h in handles {
            if let Ok(local) = h.join() {
                per_worker.push(local);
            }
        }
    });

    if stale.load(Ordering::Relaxed) || latest_generation.load(Ordering::Relaxed) != generation {
        return None;
    }

    let total: usize = per_worker
        .iter()
        .flat_map(|worker_matches| worker_matches.iter())
        .map(Vec::len)
        .sum();
    let mut all = Vec::with_capacity(total);
    for worker_matches in per_worker {
        for mut chunk_matches in worker_matches {
            all.append(&mut chunk_matches);
        }
    }
    Some(all)
}
