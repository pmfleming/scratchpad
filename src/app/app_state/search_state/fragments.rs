use crate::app::capacity_metrics;
use crate::app::domain::DocumentSnapshot;
use crate::app::domain::buffer::DocumentChunk;
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

    if let Some(text) = snapshot.piece_tree().borrow_range(normalized.clone()) {
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

struct FragmentSearchPlan {
    chunks: Vec<DocumentChunk>,
    final_boundary: Option<usize>,
}

fn fragment_search_plan(
    context: &FragmentSearchContext<'_>,
    range: Range<usize>,
) -> FragmentSearchPlan {
    if range.is_empty() || context.program.query().is_empty() {
        return FragmentSearchPlan {
            chunks: Vec::new(),
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
        chunks: context.snapshot.chunks_for_range(
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
    let plan = fragment_search_plan(&context, range);
    capacity_metrics::record_search_chunks(plan.chunks.len());
    let final_boundary = plan.final_boundary;

    process_chunks_concurrent(
        plan.chunks,
        context.intra_parallelism,
        context.generation,
        context.latest_generation,
        |chunk| {
            let (window_text, window_offset) = context
                .snapshot
                .search_text_cow(Some(chunk.window_range.clone()));
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
                        let at_final_boundary = final_boundary
                            .is_some_and(|end| chunk.core_range.end == end && global_start == end);
                        (in_core || at_final_boundary).then_some(global_start..global_end)
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
