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

pub(super) fn intra_buffer_parallelism() -> usize {
    thread::available_parallelism()
        .map_or(1, |p| p.get().min(8))
        .max(1)
}

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
    let plan = fragment_search_plan(&context, range);
    let final_boundary = plan.final_boundary;
    process_chunks_concurrent(
        plan.chunks,
        context.intra_parallelism,
        context.generation,
        context.latest_generation,
        context.program.result_limit(),
        |chunk| {
            let (window_text, window_offset) = context
                .snapshot
                .try_search_text(Some(chunk.window_range.clone()))
                .ok()?;
            let outcome = search::search_program_window_interruptible(
                window_text.as_ref(),
                context.program,
                chunk
                    .window_range
                    .len()
                    .saturating_sub(chunk.core_range.len()),
                || context.latest_generation.load(Ordering::Relaxed) == context.generation,
            )?;
            debug_assert!(outcome.error.is_none());
            Some(
                outcome
                    .matches
                    .into_iter()
                    .filter_map(|matched| {
                        let start = window_offset + matched.start;
                        let end = window_offset + matched.end;
                        let in_core = chunk.core_range.contains(&start);
                        let at_final_boundary = final_boundary.is_some_and(|boundary| {
                            chunk.core_range.end == boundary && start == boundary
                        });
                        (in_core || at_final_boundary).then_some(start..end)
                    })
                    .collect(),
            )
        },
    )
    .filter(|matches| context.program.accept_match_count(matches.len()))
}

fn next_chunk_batch(chunks: &mut impl Iterator<Item = DocumentChunk>) -> Vec<DocumentChunk> {
    // Planning and text loading stay bounded to the same eight windows.
    let batch: Vec<_> = chunks.take(8).collect();
    capacity_metrics::record_search_chunks(batch.len());
    batch
}

fn process_chunks_concurrent(
    mut input: impl Iterator<Item = DocumentChunk>,
    intra_parallelism: usize,
    generation: u64,
    latest_generation: &AtomicU64,
    max_matches: usize,
    process: impl Fn(&DocumentChunk) -> Option<Vec<Range<usize>>> + Sync,
) -> Option<Vec<Range<usize>>> {
    let stale = AtomicBool::new(false);
    let current = || generation_is_current(generation, latest_generation, &stale);
    if !current() {
        return None;
    }
    let chunks = next_chunk_batch(&mut input);
    let workers = intra_parallelism.min(chunks.len()).max(1);
    if workers == 1 || chunks.len() < INTRA_BUFFER_PARALLELISM_MIN_CHUNKS {
        return collect_chunk_batches(input, chunks, current, |chunks, matches| {
            append_match_batches(
                matches,
                max_matches,
                std::iter::once(process_chunk_batch(
                    chunks,
                    generation,
                    latest_generation,
                    &stale,
                    &process,
                )),
            )
        });
    }
    capacity_metrics::record_search_intra_buffer_workers(workers);
    // One cohort per target; at most eight windows are outstanding at a time.
    thread::scope(|scope| {
        let mut channels = Vec::with_capacity(workers);
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let (send, receive) = std::sync::mpsc::sync_channel::<Vec<DocumentChunk>>(0);
            let (results, completed) = std::sync::mpsc::sync_channel(1);
            let stale = &stale;
            let process = &process;
            handles.push(scope.spawn(move || {
                while let Ok(chunks) = receive.recv() {
                    let result =
                        process_chunk_batch(&chunks, generation, latest_generation, stale, process);
                    let failed = result.is_none();
                    if results.send(result).is_err() || failed {
                        break;
                    }
                }
            }));
            channels.push((send, completed));
        }
        let result = collect_chunk_batches(input, chunks, current, |chunks, matches| {
            let batches = chunks.chunks(chunks.len().div_ceil(workers));
            let used = batches.len();
            for ((send, _), batch) in channels.iter().zip(batches) {
                send.send(batch.to_vec()).ok()?;
            }
            // Receive in input order, regardless of completion order.
            append_match_batches(
                matches,
                max_matches,
                channels[..used]
                    .iter()
                    .map(|(_, completed)| completed.recv().ok().flatten()),
            )
        });
        // Release both sides before joining, including cancellation, read failure
        // and panic paths; no worker can remain blocked sending a result.
        drop(channels);
        let mut joined = true;
        for handle in handles {
            joined &= handle.join().is_ok();
        }
        result.filter(|_| joined)
    })
}

fn collect_chunk_batches(
    mut input: impl Iterator<Item = DocumentChunk>,
    mut chunks: Vec<DocumentChunk>,
    current: impl Fn() -> bool,
    mut process: impl FnMut(&[DocumentChunk], &mut Vec<Range<usize>>) -> Option<bool>,
) -> Option<Vec<Range<usize>>> {
    let mut matches = Vec::new();
    while !chunks.is_empty() {
        if !current() {
            return None;
        }
        if process(&chunks, &mut matches)? {
            return Some(matches);
        }
        chunks = next_chunk_batch(&mut input);
    }
    current().then_some(matches)
}

// true preserves one overflow sentinel so the caller can reject incomplete results.
fn append_match_batches(
    matches: &mut Vec<Range<usize>>,
    max_matches: usize,
    batches: impl IntoIterator<Item = Option<Vec<Vec<Range<usize>>>>>,
) -> Option<bool> {
    for batch in batches {
        for mut found in batch? {
            found.truncate(max_matches.saturating_add(1).saturating_sub(matches.len()));
            matches.append(&mut found);
            if matches.len() > max_matches {
                return Some(true);
            }
        }
    }
    Some(false)
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

#[cfg(test)]
mod batching_tests {
    use super::{DocumentChunk, process_chunks_concurrent, search_target_ranges};
    use crate::app::services::search::{self, SearchMode, SearchProgram};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    fn chunks(count: usize) -> impl Iterator<Item = DocumentChunk> {
        (0..count).map(|index| DocumentChunk {
            core_range: index..index + 1,
            window_range: index..index + 1,
        })
    }

    #[test]
    fn one_worker_cohort_preserves_order_across_many_bounded_batches() {
        let threads = std::sync::Mutex::new(std::collections::HashSet::new());
        let generation = AtomicU64::new(1);
        let found = process_chunks_concurrent(chunks(37), 3, 1, &generation, usize::MAX, |chunk| {
            threads.lock().unwrap().insert(thread::current().id());
            if chunk.core_range.start.is_multiple_of(8) {
                thread::sleep(std::time::Duration::from_millis(1));
            }
            Some(vec![chunk.core_range.clone()])
        })
        .unwrap();
        assert_eq!(
            found,
            (0..37).map(|index| index..index + 1).collect::<Vec<_>>()
        );
        assert_eq!(threads.into_inner().unwrap().len(), 3);
        for workers in [1, 4] {
            for count in [0usize, 1, 3, 8, 9, 37] {
                for limit in [0usize, 1, 8, usize::MAX] {
                    let found = process_chunks_concurrent(
                        chunks(count),
                        workers,
                        1,
                        &generation,
                        limit,
                        |chunk| Some(vec![chunk.core_range.clone()]),
                    )
                    .unwrap();
                    let expected = (0..count.min(limit.saturating_add(1)))
                        .map(|index| index..index + 1)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        found, expected,
                        "workers={workers}, count={count}, limit={limit}"
                    );
                }
            }
        }
    }

    #[test]
    fn failed_cancelled_or_panicked_batches_never_return_partial_completion() {
        for failed_index in [0, 11, 36] {
            let generation = AtomicU64::new(1);
            assert!(
                process_chunks_concurrent(chunks(37), 4, 1, &generation, usize::MAX, |chunk| {
                    (chunk.core_range.start != failed_index).then(|| vec![chunk.core_range.clone()])
                })
                .is_none()
            );
        }
        let generation = AtomicU64::new(1);
        assert!(
            process_chunks_concurrent(chunks(37), 4, 1, &generation, usize::MAX, |chunk| {
                if chunk.core_range.start == 11 {
                    generation.store(2, Ordering::Relaxed);
                }
                Some(vec![chunk.core_range.clone()])
            })
            .is_none()
        );
        generation.store(1, Ordering::Relaxed);
        assert!(
            process_chunks_concurrent(chunks(37), 4, 1, &generation, usize::MAX, |chunk| {
                assert_ne!(chunk.core_range.start, 11, "injected worker panic");
                Some(vec![chunk.core_range.clone()])
            })
            .is_none()
        );
    }

    #[test]
    fn bounded_planning_preserves_unicode_regex_and_nonoverlapping_plain_matches() {
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

/// A bounded prefix for display after the normal global budget is exceeded.
/// Each scan window remains bounded; overlapping chunks never duplicate hits.
pub(super) fn search_target_prefix(
    snapshot: &DocumentSnapshot,
    search_range: Option<Range<usize>>,
    program: &SearchProgram,
    generation: u64,
    latest_generation: &AtomicU64,
    limit: usize,
) -> Option<Vec<Range<usize>>> {
    let range = search_range.unwrap_or(0..snapshot.document_length().chars);
    let context = FragmentSearchContext {
        snapshot,
        program,
        generation,
        latest_generation,
        intra_parallelism: 1,
    };
    let plan = fragment_search_plan(&context, range);
    let mut matches = Vec::new();
    for chunk in plan.chunks {
        if latest_generation.load(Ordering::Relaxed) != generation {
            return None;
        }
        let (text, offset) = snapshot
            .try_search_text(Some(chunk.window_range.clone()))
            .ok()?;
        let outcome = search::search_program_interruptible(&text, program, || {
            latest_generation.load(Ordering::Relaxed) == generation
        })?;
        for range in outcome.matches {
            let range = range.start + offset..range.end + offset;
            let in_core = chunk.core_range.contains(&range.start)
                || plan
                    .final_boundary
                    .is_some_and(|end| chunk.core_range.end == end && range.start == end);
            if in_core
                && matches
                    .last()
                    .is_none_or(|previous: &Range<usize>| previous.end <= range.start)
            {
                if matches.len() == limit {
                    return Some(matches);
                }
                matches.push(range);
            }
        }
    }
    Some(matches)
}
