# Recent Capacity Regression Review

This document summarizes the review of the latest capacity test after the recent two implementation passes focused on:

- reducing piece-tree flattening
- expanding bounded background work for file load, piece-tree construction, session restore, and metadata recalculation

The main question is why the latest capacity run appears worse rather than better.

## Executive Summary

The latest capacity artifacts suggest two different outcomes:

1. most of the recent concurrency-oriented changes were unlikely to improve this specific probe very much, because the probe still measures direct in-memory buffer construction and metadata refresh rather than the open/load/restore paths that were moved off-thread
2. one of the recent changes likely introduced a real regression risk: the new fragmented plain-text search fallback appears likely to increase CPU and allocation work on fragmented buffers

In short:

- part of the disappointment is benchmark mismatch
- part of it may be a real regression

## What the Latest Capacity Report Shows

From `target/analysis/capacity_report.json`:

- file size ceiling still reports `32.0 MB` last good and `128.0 MB` first failure
- tab count ceiling still reports `512 tabs` last good and `4096 tabs` first failure
- paste size ceiling now reports `8.0 MB` last good and `64.0 MB` first failure

That means the clearest negative movement in the latest run is the paste ceiling.

## Finding 1. The New Fragmented Plain-Text Search Fallback Is a Likely Real Regression

The most concrete regression candidate is in [src/app/app_state/search_state/worker.rs](/C:/Code/scratchpad/src/app/app_state/search_state/worker.rs).

Relevant code:

- `SEARCH_FRAGMENT_CHUNK_CHARS` at line 16
- `search_target_ranges(...)` at line 277
- `search_fragmented_plain_text(...)` at line 328

The new path avoids flattening the entire document for fragmented plain-text search, which was the intended goal. However, in the fragmented case it now:

- extracts a fresh `String` window for each chunk with `extract_range(...)`
- runs the matcher separately on each chunk
- repeats overlap work across chunk boundaries

That can reduce peak whole-buffer materialization, but it can also increase total CPU and allocation cost substantially when the buffer is already fragmented by edits or large pastes.

This is therefore a plausible real regression on large edited buffers and large paste-adjacent workflows.

## Finding 2. The Background I/O Worker Is Still a Single Serial Lane

The expanded background work in [src/app/services/background_io.rs](/C:/Code/scratchpad/src/app/services/background_io.rs) is still executed through one FIFO worker thread.

Relevant code:

- `spawn_background_io_worker()` at line 75
- single `while let Ok(request) = request_rx.recv()` loop at line 80
- `LoadPaths` handled at line 82
- `PersistSession` handled at line 96
- `RefreshEncodingCompliance` handled at line 106

This means:

- path loads
- session persistence
- encoding-compliance refreshes

all compete for the same worker lane.

That is useful for moving work off the foreground thread, but it does not create real parallel throughput. A large session persist or whole-buffer encoding-compliance scan can still head-of-line block file load or restore work behind it.

So the code now does more work in the background, but not necessarily more work concurrently.

## Finding 3. Session Persistence Is Only Partially Off the UI Path

In [src/app/services/session_manager.rs](/C:/Code/scratchpad/src/app/services/session_manager.rs), session persistence is now queued in the background, but the expensive capture step still happens synchronously before enqueueing.

Relevant code:

- `maybe_persist_session(...)` at line 9
- `SessionPersistRequest::capture(...)` call at line 27
- `clear_session_dirty()` at line 33
- `queue_background_session_persist(...)` at line 34

The capture itself walks the current tabs and snapshots every buffer in [src/app/services/session_store/mod.rs](/C:/Code/scratchpad/src/app/services/session_store/mod.rs):

- `SessionPersistRequest::capture(...)` at line 377
- `CapturedSessionTab::capture(...)` at line 393
- `buffer.document_snapshot()` capture at line 412

So:

- the disk write moved off-thread
- but “prepare the entire session snapshot” is still foreground work

That weakens the intended benefit of the background session-save change.

## Why the Capacity Probe Likely Did Not Show Much Benefit

The current capacity probe in [src/bin/capacity_probe.rs](/C:/Code/scratchpad/src/bin/capacity_probe.rs) still measures direct in-memory editor-core work rather than the file-open/session-restore paths that were recently parallelized.

Relevant code:

- file-size sweep constructs buffers directly with `BufferState::new(...)` at line 62
- paste-size sweep uses `run_paste_capacity_cycle(...)` at line 188
- that path does:
  - `BufferState::new(...)` at line 189
  - `document_mut().insert_direct(...)` at line 196
  - `buffer.refresh_text_metadata()` at line 197
- tab-count sweep also builds tabs directly via `BufferState::new(...)` at lines 204, 230, and 236

That means the probe still times:

- direct buffer construction
- direct paste mutation
- direct full metadata refresh
- direct tab object growth

It does not primarily time:

- background file decode
- background piece-tree construction for open/reopen/reload flows
- background session restore
- background path loading

So even if the recent background-load changes were good architectural moves, this particular probe was not well positioned to show their benefit.

## Interpretation

The latest result should not be read as “all recent changes were bad.”

The stronger interpretation is:

- the changes did not target the hottest paths in the current capacity probe closely enough
- the probe still centers on synchronous metadata refresh and direct mutation costs
- the new fragmented-search fallback may have introduced additional CPU/allocation cost on edited buffers

That combination can easily produce a “no gain” or “slightly worse” result even if some open/load/restore paths improved in real app usage.

## Most Likely Next Targets

If the goal is to improve the measured capacity ceilings from the current probe, the next work should focus more directly on the paths this probe still exercises:

1. reduce post-paste metadata refresh cost
   The paste probe still does `insert_direct(...)` followed by `refresh_text_metadata()`, so this remains a direct latency cost.

2. reduce whole-document artifact and format rescans
   Full metadata recomputation after edits still appears to be a major synchronous cost center.

3. revisit fragmented plain-text search
   The current chunked fallback likely trades lower peak flattening for higher repeated extraction and scan cost.

4. separate background work into more than one bounded lane where appropriate
   File loads, session persistence, and encoding-compliance refreshes should not all queue behind one another on the same worker.

5. move session snapshot capture itself off the foreground path where safe
   Background persistence only helps fully once the expensive capture/preparation work is also revision-safe and deferred.

## Conclusion

The latest capacity regression review points to two main conclusions:

- the recent concurrency work mostly missed the exact hot paths measured by the current capacity probe
- the new fragmented plain-text search fallback is a plausible real regression and should be treated as the highest-confidence suspect from this pass

The immediate next step should be to target the probe’s actual synchronous costs more directly:

- paste-time metadata refresh
- whole-document metadata rescans
- fragmented-search overhead

Those are more likely to move the measured capacity ceilings than additional background-load work alone.
