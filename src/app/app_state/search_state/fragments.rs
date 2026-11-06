use crate::app::capacity_metrics;
use crate::app::domain::DocumentSnapshot;
use crate::app::domain::buffer::{DocumentChunk, DocumentChunks};
use crate::app::services::search::{self, SearchMode, SearchProgram};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

// Keep fragment windows near the piece-tree leaf target so edited buffers are
// more likely to expose each chunk as a borrowed span instead of a flattened
// range allocation.
pub(super) const SEARCH_FRAGMENT_CHUNK_CHARS: usize = 256 * 1024;
const INTRA_BUFFER_PARALLELISM_MIN_CHUNKS: usize = 4;

struct FragmentSearchContext<'a> {
    snapshot: &'a DocumentSnapshot,
    program: &'a SearchProgram,
    generation: u64,
    latest_generation: &'a AtomicU64,
    intra_parallelism: usize,
}

pub(super) fn search_target_ranges(
    snapshot: &DocumentSnapshot,
    search_range: Option<Range<usize>>,
    program: &SearchProgram,
    generation: u64,
    latest_generation: &AtomicU64,
    intra_parallelism: usize,
) -> Option<Vec<Range<usize>>> {
    let normalized = search_range
        .map(|range| snapshot.normalize_char_range(range))
        .unwrap_or(0..snapshot.document_length().chars);

    if snapshot.piece_tree().metrics().pieces <= 1 {
        let (text, _) = snapshot.try_search_text(Some(normalized.clone())).ok()?;
        let outcome = search::search_program_interruptible(&text, program, || {
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

    let context = FragmentSearchContext {
        snapshot,
        program,
        generation,
        latest_generation,
        intra_parallelism,
    };
    search_fragmented(context, normalized)
}

struct FragmentSearchPlan<'a> {
    chunks: DocumentChunks<'a>,
    final_boundary: Option<usize>,
}

fn fragment_search_plan<'a>(
    context: &FragmentSearchContext<'a>,
    range: Range<usize>,
) -> FragmentSearchPlan<'a> {
    if range.is_empty() || context.program.query().is_empty() {
        return FragmentSearchPlan {
            chunks: context.snapshot.chunk_iter_for_range(0..0, 1, 0, 0),
            final_boundary: None,
        };
    }

    let whole_word_context = usize::from(context.program.options().whole_word);
    let (chunk_chars, leading_overlap, trailing_overlap, final_boundary) =
        match context.program.options().mode {
            SearchMode::PlainText => {
                let query_chars = context.program.query().chars().count().max(1);
                let overlap = query_chars + whole_word_context;
                (
                    SEARCH_FRAGMENT_CHUNK_CHARS.max(query_chars.saturating_mul(4)),
                    overlap,
                    overlap,
                    None,
                )
            }
            SearchMode::Regex => {
                let leading_overlap = 1 + whole_word_context;
                let trailing_overlap = context
                    .program
                    .max_match_chars()
                    .saturating_add(leading_overlap);
                (
                    SEARCH_FRAGMENT_CHUNK_CHARS.max(trailing_overlap.max(1)),
                    leading_overlap,
                    trailing_overlap,
                    Some(range.end),
                )
            }
        };
    FragmentSearchPlan {
        chunks: context.snapshot.chunk_iter_for_range(
            range,
            chunk_chars,
            leading_overlap,
            trailing_overlap,
        ),
        final_boundary,
    }
}

fn search_fragmented(
    context: FragmentSearchContext<'_>,
    range: Range<usize>,
) -> Option<Vec<Range<usize>>> {
    let mut plan = fragment_search_plan(&context, range);
    let final_boundary = plan.final_boundary;
    let mut matches = Vec::new();
    loop {
        if context.latest_generation.load(Ordering::Relaxed) != context.generation {
            return None;
        }
        // Bound lookahead so planning does not evict the entire file before matching.
        let chunks: Vec<_> = plan.chunks.by_ref().take(8).collect();
        if chunks.is_empty() {
            break;
        }
        capacity_metrics::record_search_chunks(chunks.len());
        matches.extend(process_chunks_concurrent(
            chunks,
            context.intra_parallelism,
            context.generation,
            context.latest_generation,
            |chunk| {
                let (window_text, window_offset) = context
                    .snapshot
                    .try_search_text(Some(chunk.window_range.clone()))
                    .ok()?;
                let outcome = search::search_program_interruptible(
                    window_text.as_ref(),
                    context.program,
                    || context.latest_generation.load(Ordering::Relaxed) == context.generation,
                )?;
                debug_assert!(outcome.error.is_none());
                Some(
                    outcome
                        .matches
                        .into_iter()
                        .filter_map(|matched| {
                            let global_start = window_offset + matched.start;
                            let global_end = window_offset + matched.end;
                            let in_core = global_start >= chunk.core_range.start
                                && global_start < chunk.core_range.end;
                            let at_final_boundary = final_boundary.is_some_and(|end| {
                                chunk.core_range.end == end && global_start == end
                            });
                            (in_core || at_final_boundary).then_some(global_start..global_end)
                        })
                        .collect(),
                )
            },
        )?);
    }
    Some(matches)
}

fn process_chunks_concurrent(
    chunks: Vec<DocumentChunk>,
    intra_parallelism: usize,
    generation: u64,
    latest_generation: &AtomicU64,
    process: impl Fn(&DocumentChunk) -> Option<Vec<Range<usize>>> + Sync,
) -> Option<Vec<Range<usize>>> {
    if chunks.is_empty() {
        return Some(Vec::new());
    }

    let workers = intra_parallelism.min(chunks.len()).max(1);
    if workers == 1 || chunks.len() < INTRA_BUFFER_PARALLELISM_MIN_CHUNKS {
        return process_chunk_batch(
            &chunks,
            generation,
            latest_generation,
            &AtomicBool::new(false),
            &process,
        )
        .map(|matches| matches.into_iter().flatten().collect());
    }

    capacity_metrics::record_search_intra_buffer_workers(workers);
    let stale = AtomicBool::new(false);
    let chunk_size = chunks.len().div_ceil(workers);
    let per_worker = thread::scope(|scope| {
        let handles = chunks
            .chunks(chunk_size)
            .map(|worker_chunks| {
                scope.spawn(|| {
                    process_chunk_batch(
                        worker_chunks,
                        generation,
                        latest_generation,
                        &stale,
                        &process,
                    )
                })
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok().flatten())
            .collect::<Vec<_>>()
    });

    generation_is_current(generation, latest_generation, &stale)
        .then(|| flatten_worker_matches(per_worker))
}

fn process_chunk_batch(
    chunks: &[DocumentChunk],
    generation: u64,
    latest_generation: &AtomicU64,
    stale: &AtomicBool,
    process: &impl Fn(&DocumentChunk) -> Option<Vec<Range<usize>>>,
) -> Option<Vec<Vec<Range<usize>>>> {
    let mut matches = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if !generation_is_current(generation, latest_generation, stale) {
            return None;
        }
        let Some(chunk_matches) = process(chunk) else {
            stale.store(true, Ordering::Relaxed);
            return None;
        };
        matches.push(chunk_matches);
    }
    Some(matches)
}

fn generation_is_current(
    generation: u64,
    latest_generation: &AtomicU64,
    stale: &AtomicBool,
) -> bool {
    !stale.load(Ordering::Relaxed) && latest_generation.load(Ordering::Relaxed) == generation
}

fn flatten_worker_matches(per_worker: Vec<Vec<Vec<Range<usize>>>>) -> Vec<Range<usize>> {
    let total = per_worker.iter().flatten().map(Vec::len).sum();
    let mut matches = Vec::with_capacity(total);
    for mut chunk_matches in per_worker.into_iter().flatten() {
        matches.append(&mut chunk_matches);
    }
    matches
}

#[cfg(test)]
mod batching_tests {
    #[test]
    fn bounded_planning_preserves_unicode_regex_and_nonoverlapping_plain_matches() {
        use super::*;
        use crate::app::domain::TextDocument;
        use crate::app::services::search::SearchOptions;
        let text = format!("{}\n{}", "aaéaaa 東京\n".repeat(200_000), "aaaaa");
        let snapshot = TextDocument::new(text.clone()).snapshot();
        assert!(snapshot.piece_tree().metrics().pieces > 8);
        for (query, mode) in [
            ("aaa", SearchMode::PlainText),
            ("東京", SearchMode::PlainText),
            (r"a{2,3}", SearchMode::Regex),
        ] {
            let program = SearchProgram::compile(
                query,
                SearchOptions {
                    mode,
                    match_case: true,
                    ..SearchOptions::default()
                },
            )
            .unwrap();
            let expected = search::search_program(&text, &program).matches;
            let generation = AtomicU64::new(1);
            for workers in [1, 4] {
                assert_eq!(
                    search_target_ranges(&snapshot, None, &program, 1, &generation, workers)
                        .unwrap(),
                    expected
                );
            }
            generation.store(2, Ordering::Relaxed);
            assert!(search_target_ranges(&snapshot, None, &program, 1, &generation, 4).is_none());
        }
    }
}
