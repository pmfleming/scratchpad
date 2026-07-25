# Scratchpad performance-lens co-development baseline

Date: 2026-07-25

## Run

The local `scratchpad-performance-lens` checkout was run against this repository with:

```console
cargo run --bin splens -- measure all --config examples/scratchpad.toml
```

The initial full run took 11m 8s and reported two passing promises, two at risk, and three failing. That result exposed measurement defects as well as application limits: synthetic fixture construction was included in several capacity timers, one sample decided each ceiling, and a passing search-target row could mask a failing many-file row.

After co-developing both repositories, the current review reports:

| Promise | Status | Key evidence |
| --- | --- | --- |
| Large Files | fail | 1 GiB first-visible prefix 0.48 ms and staged paint 3.9 ms, but full ingest remains 1184 ms against 180 ms |
| Many Files | pass | 10,000-file first-visible latency 64.6 ms; cold-shell background completion 697 ms; prepared full hydration 554 ms |
| Search | pass | 1 GiB prepared search median about 160 ms; 10,000 targets about 2.4 ms |
| Many Tabs | pass | 10,000-tab reorder Criterion mean about 0.18 ms; prepared capacity cycle about 14 ms |
| Many Views | pass | 1,000-view prepared operation median below 0.1 ms |
| Large Text Mutation | pass | 128 MiB paste Criterion mean about 136 ms and tail estimate below 150 ms |
| Session Persistence Restore | pass | 10,000-tab startup-visible restore 14.9 ms; steady-state persist 66.8 ms; full restore completion 148 ms |

The generated source of truth remains `target/analysis/performance_review.json`.

## Scratchpad changes

- Tab reorder updates existing buffer/path indexes in place instead of clearing, reallocating, cloning path keys, and rebuilding both indexes.
- Piece construction now uses up to the machine's available 16 workers for large inputs.
- Parallel text inspection now also scales to 16 workers.
- Mixed UTF-8 piece metrics use the optimized `chars().count()` path plus SIMD-backed `memchr` newline counting instead of one scalar branch-heavy byte loop.
- Frame histogram buckets now have 0.5 ms resolution and exact bucket-boundary handling instead of 1 ms resolution with boundary inflation.
- Large files expose a bounded UTF-8-safe first-visible window while complete ingestion runs on the existing background path; preview editing and saving are disabled until hydration replaces it.
- Large file batches hydrate the selected file first and construct inactive metadata-only shells on the path lane before one bulk installation.
- Session persistence tracks snapshot revisions and skips rewriting unchanged private snapshots.
- Added `promise_latency` Criterion coverage for 10,000-tab reorder, 128 MiB paste, 10,000-tab startup restore, and steady-state session persistence.
- Added an event-to-tessellation frame probe covering input construction, app update, layout, paint generation, and egui tessellation.

On this 16-thread Ryzen 7 PRO 8840HS, the 128 MiB paste insert diagnostic moved from about 102 ms to 84 ms, metadata refresh from about 57 ms to 50 ms, and the end-to-end benchmark from an initial 159 ms mean to about 137 ms.

## Measurement changes

- Capacity fixtures are prepared outside the operation timer where appropriate; setup cost remains visible as `setup_elapsed_ms`.
- Capacity decisions use the median of three repetitions and retain min/max values.
- Search and split capacity no longer fail because text/tile fixture construction was timed as search/rebalance work.
- Promise scale failures stay correlated to the capacity scenario that failed; unrelated passing rows cannot mask them.
- `failed_capacity_scenarios` explains which evidence caused a promise failure.
- `measure all` no longer runs `search_speed` twice.
- Resource CLI output distinguishes peak live heap from cumulative allocated bytes.
- Session restore now requires authoritative speed evidence rather than passing solely because a 10,000-tab resource sample exists.
- Many-file evidence separates first-visible latency from background shell completion and prepared full hydration.
- Large-file evidence separately reports bounded first-visible decode/paint and complete piece-tree ingestion.
- Frame evidence distinguishes event-to-tessellation render preparation from GPU submission, compositor, present, and vsync work.

## Next priorities

1. **Large Files:** first-visible latency is now bounded, but complete 1 GiB hydration is still about 6.6x over budget and peaks near 4 GiB heap. Replace the eventual eager `Arc<str>` ingestion with chunked/file-backed storage.
2. **Many Files:** cold-shell first-visible behavior passes at 10,000 files; next reduce the roughly 697 ms background shell completion and validate activation latency for randomly selected cold files.
3. **Session restore:** add a cold/first-persist Criterion diagnostic alongside the passing steady-state persistence contract; the resource probe still measures about 4.9 s for the initial 10,000-tab snapshot set.
4. **Frames:** the event-to-tessellation probe passes, but GPU upload/submission, compositor timing, present callbacks, and vsync pacing remain unmeasured.
5. Run capacity and promise benchmarks on Windows as well as Linux before treating the current thresholds as cross-platform guarantees.
