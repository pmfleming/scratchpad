# Performance Improvement Plan

Date: 2026-05-10

This is a doc-only plan for improving Scratchpad runtime performance. It has been rewritten after review against the project code and the `scripts/open-overview.ps1` refresh path. It intentionally does not depend on the other planning docs.

The framing is now narrower:

- Use the open-overview pipeline as the measurement surface.
- Treat existing implementation as the starting point, not as missing infrastructure.
- Fix shared multipliers before tuning throughput.
- Reconcile probe measurements with the real in-app paths before committing to UI latency targets.

## Measurement Surface

`scripts/open-overview.ps1` is the reference workflow for regenerating the dashboard data. Its full refresh path runs the visibility-mode generators for slowspots, search speed, capacity, resource profiles, locality, static leverage, hotspots, escape hatches, clone alerts, test catalog, measurement catalog, project code metrics, architecture map, flamegraphs, speed-efficiency, and performance review.

For this plan, the relevant generated artifacts are:

- `target/analysis/performance_review.json`
- `target/analysis/speed_efficiency_report.json`
- `target/analysis/capacity_report.json`
- `target/analysis/resource_profiles.json`
- Flamegraph rows produced through `scripts/generate_flamegraphs.py`

Phase 0 must produce a clean open-overview baseline before later phases are judged.

## Executive Summary

The largest misses are still search completion, search dispatch, snapshot creation, viewport extraction, paste allocation, and layout/scroll behavior. The phase order stays shared-multiplier-first, but two important pieces of framing changed after checking the code:

1. The runtime editor painting path already consults `LayoutCache`. Phase 5 is "right-size and reshape the existing cache, then reconcile probe measurements with the cache-consulting runtime path."
2. Search deduplication currently happens after `SearchTargetSnapshot` construction. That means `buffer.document_snapshot()` can run once per view before duplicates are discarded. Phase 2 must dedup by file identity before snapshot construction, not merely make the one retained target cheap.

The highest-leverage changes are:

1. Thread the already-maintained `BufferState::line_count` into snapshot creation so `DocumentSnapshot::from_shared` stops recomputing display line count with a full piece-tree scan.
2. Move search target dedup ahead of snapshot construction, then keep target payloads cheap and lazy.
3. Measure and reduce `SearchResultAccumulator::partial_snapshot` clone cost; worker queue coalescing already exists.
4. Treat `flatten_text`, `extract_text`, `search_text`, `search_text_cow`, and `record_full_text_flatten` as the whole-document allocation tripwire set.
5. Add concrete paste-shape meters so Phase 4 knows whether paste arrives as one edit, many edits, an incremental metadata miss, or a forced full scan.
6. Right-size the existing layout cache, decide per-view versus per-buffer lifetime, and align profiling probes with the runtime layout path.

## Current Metric Picture

### Coverage

`performance_review.json` currently reports:

| Metric | Value |
| --- | ---: |
| Covered scenarios | 7 / 7 |
| Latency rows | 200 |
| Capacity rows | 9 |
| Resource rows | 9 |
| Flamegraph rows | 12 |
| Budget misses | 82 |
| Ceilings reached | 7 |
| Failed source artifacts | 0 |

`speed_efficiency_report.json` reports:

| Metric | Value |
| --- | ---: |
| Search scenarios | 51 |
| Search dispatch scenarios | 12 |
| Editor scenarios | 22 |
| Tabs and splits scenarios | 10 |
| Over-budget latency rows | 22 |
| Critical triage rows | 29 |
| Watch triage rows | 26 |
| OK triage rows | 49 |

Artifact caveat: earlier artifact generation showed disk-space warnings during flamegraph generation and one profile row captured compile errors from a dirty tree. Phase 0 must regenerate a clean baseline from a compiling tree before optimization results are trusted.

### Largest Budget Misses

| Scenario | Scale | Budget | Mean | Ratio | Primary profile |
| --- | ---: | ---: | ---: | ---: | --- |
| `search_current_app_state_completion_aggregate_size` | 256 files | 85 ms | 1828.762 ms | 21.51x | `search_current_app_state_profile` |
| `search_current_completion_aggregate_size` | 256 files | 55 ms | 552.867 ms | 10.05x | `search_current_app_state_profile` |
| `document_snapshot_creation_latency` | 4 MB | 5 ms | 34.770 ms | 6.95x | `document_snapshot_profile` |
| `search_current_app_state_completion_aggregate_size` | 128 files | 85 ms | 467.768 ms | 5.50x | `search_current_app_state_profile` |
| `search_current_dispatch_aggregate_size` | 128 files | 12 ms | 48.506 ms | 4.04x | `search_dispatch_profile` |
| `search_all_completion_aggregate_size` | 256 files | 85 ms | 329.750 ms | 3.88x | `search_all_tabs_profile` |
| `search_active_completion_file_size` | 1 MB | 25 ms | 95.347 ms | 3.81x | `search_current_app_state_profile` |
| `search_all_dispatch_aggregate_size` | 128 files | 20 ms | 48.977 ms | 2.45x | `search_dispatch_profile` |
| `viewport_extraction_latency` | 4 MB | 16 ms | 34.125 ms | 2.13x | `viewport_extraction_profile` |

Search first response is already healthy. The current aggregate-size maximum is 0.336 ms. Preserve that as a guardrail: no regression past 1 ms at the current 256-target ceiling.

### Capacity Ceilings

`capacity_report.json` shows 7 of 9 ceilings reached, with 7 memory-bound scenarios and 2 CPU-bound scenarios.

| Scenario | Last OK | First failure | Suspected bound |
| --- | ---: | ---: | --- |
| File size | 32 MB | 128 MB | Memory |
| Layout bytes | 8 MB | 32 MB | Memory |
| Many file count | 1,000 files | 10,000 files | Memory |
| Paste size | 8 MB | 64 MB | Memory |
| Search file size | 64 MB | 256 MB | Memory |
| Split count | 128 splits | 512 splits | Memory |
| Tab count | 32 tabs | 512 tabs | Memory |
| Search target count | 10,000 files | Not reached | CPU |
| View count | 1,000 views | Not reached | CPU |

The memory-bound ceilings line up with whole-document allocation, cloned result state, eager match strings, layout cache churn, and inactive tab/view state.

### Resource Profiles

| Scenario | Max elapsed | Max allocated | Max peak live |
| --- | ---: | ---: | ---: |
| Many file tracking | 2278.711 ms | 18.249 GB | 128.811 MB |
| Tab count tracking | 2516.981 ms | 4.155 GB | 531.172 MB |
| Session restore | 1448.872 ms | 3.727 GB | 70.899 MB |
| Search file size tracking | 103.015 ms | 536.871 MB | 536.871 MB |
| Paste allocation | 345.301 ms | 270.691 MB | 269.879 MB |
| File-backed open allocation | 485.732 ms | 277.387 MB | 135.208 MB |
| Session persist | 10504.315 ms | 46.140 MB | 24.797 MB |

## What Already Exists

### Search target deduplication

Search target deduplication exists, but it is applied too late for the expensive path. `collect_search_targets_for_views` builds a `SearchTargetSnapshot` for each view, and `build_search_target_from_view` eagerly calls `buffer.document_snapshot()`. Only after that does the `HashMap` keep one target per `SearchFileIdentity`.

`AllOpenTabs` also tracks `seen_files`, but that cross-tab dedup happens after each tab has collected targets. Duplicate targets can therefore pay snapshot construction cost before being discarded.

The ordered view path also chains `ordered_view_ids_in_layout_order()` with `tab.views.iter()`. That can feed duplicate views to collection and rely on downstream dedup to clean them up.

Remaining gap:

- Dedup by file identity before constructing `SearchTargetSnapshot`.
- Use ordered view traversal once, not ordered traversal plus a full fallback iteration unless a missing-view fallback is actually needed.
- Keep the current first-response behavior as a guardrail while reducing dispatch cost.

### Piece-tree and buffer line metadata

The piece tree already maintains newline counts through metrics, per-piece counts, per-leaf prefix sums, and per-node prefix sums. The plan should not ask for generic incremental newline metrics as if they are missing.

`BufferState::line_count` is already maintained from `BufferTextMetadata`. `DocumentSnapshot::from_shared` currently calls `display_line_count_from_piece_tree` because display line count must account for standalone `\r` line breaks, not only `\n`.

Remaining gap:

- Thread the maintained line count into snapshot construction with revision safety.
- Keep the piece-tree scan as validation, repair, or test-only fallback.
- Add narrow standalone-CR validation if the existing line count cannot safely cover display-line semantics.

### Metadata fast paths

`BufferState` already has two important metadata fast paths:

- `can_skip_metadata_rescan` bypasses metadata work for ASCII-only inserts on ASCII-only buffers without existing control characters.
- `incremental_text_metadata_after_operation` handles single-edit operations and adjusts line-ending counts incrementally.

Remaining gaps:

- Multi-edit operations fall back to a full rescan.
- Non-ASCII or control-character state can force otherwise cheap edits down the full-scan path.
- Paste appears to hit `TextInspection` heavily, but the current artifact set does not prove whether paste arrives as one edit or many edits.
- `recheck_encoding_compliance` is another full-buffer scan gated by `encoding_compliance_stale`; once snapshot scanning is removed, it may become the next visible full-buffer cost.

### Layout cache

`LayoutCache` already exists in `src/app/domain/view/layout_cache.rs`, with keys for buffer revision, visible range, wrap width, font/style, highlight revision, replacement preview signature, control-character visibility, and dark mode. The cap is currently 8 entries and 4 MB.

The runtime editor layout path already uses it. `editor_galley` builds a cache key, retains the current revision, calls `view.layout_cache.get(&cache_key)`, records cache hits/misses, inserts on misses, and warms nearby slices when enabled.

The remaining `build_layouter` callers are the profiling and probe paths: `src/profile.rs`, `src/bin/resource_probe.rs`, and `src/bin/capacity_probe.rs`. That means the 45.821 ms large-document scroll baseline may be measuring a probe path that bypasses runtime cache behavior. Phase 5 must reconcile the probes with the in-app path before treating that row as a real-user scroll target.

Remaining gaps:

- Right-size the 8-entry / 4 MB budget using split-count, tab-count, and revision-churn evidence.
- Decide whether cache lifetime should remain per view or move toward per-buffer sharing for the same buffer shown in multiple splits.
- Reduce cache-key, galley, highlight-boundary, and nearby-warmup churn.
- Slice existing cache hit/miss metrics by split count, tab count, and revision churn instead of adding only another raw hit-rate counter.

### Highlight mapping

`CharByteMap` already has an ASCII fast path that allocates nothing. The residual cost is not "avoid allocating a map for ASCII." The residual cost is that `layout_job_with_highlights` builds the map before it knows whether there are any highlight ranges. On plain text with no search/selection highlights, the path should be able to append one plain segment without scanning non-ASCII text into a map.

### Worker coalescing

Search worker queue coalescing already exists, including coalesced queue depth reporting and worker active duration reporting.

Remaining gap: `SearchResultAccumulator::partial_snapshot` clones `matches` and `result_groups` on every emit. That clone path needs direct timing and byte/count metrics; generic queue-depth metrics are not the missing evidence.

### Whole-document flattening

`DocumentSnapshot::flatten_text` is the canonical whole-buffer flattening surface, but it is not the only name that can reach it:

- `flatten_text` calls `piece_tree.extract_text()`.
- `extract_text` is an unguarded alias for `flatten_text`.
- `search_text` calls `search_text_cow` and then `into_owned()`.
- `search_text_cow` may return an owned flattened string.
- `record_full_text_flatten` records flattening, but today it is a passive meter.

Remaining gap: Phase 1 and Phase 3 need guardrails that fail loudly when supposedly cheap snapshot or span-search paths reintroduce full-document flattening through any of these surfaces.

## Prioritized Plan

### Phase 0: Clean baseline and answer shaping questions

Goal: produce a trustworthy baseline and collect the missing evidence that changes implementation shape.

Actions:

- Reconcile or isolate the dirty working tree before measuring. The plan file is doc-only, but runtime performance evidence must come from a compiling tree with known changes.
- Regenerate the open-overview artifact set from the same commit.
- Preserve the generated files as the pre-plan baseline.
- Add a concrete paste-shape counter:
  - `paste_operation_count`
  - `paste_edit_count`
  - `paste_total_inserted_bytes`
  - metadata path result: skipped, incremental, full rescan
  - full-rescan reason
- Add or expose measurements for `partial_snapshot` clone count, clone duration, cloned matches, cloned groups, and emitted snapshot bytes.
- Add a guardrail around `record_full_text_flatten` for paths that later phases promise will be snapshot-cheap or span-based.
- Capture baseline layout cache hit/miss, eviction count, and bytes by split count, tab count, and revision churn.
- Reconcile large-document scroll probes with the runtime path that already consults `LayoutCache`.

Success criteria:

- Producer scripts complete without compile errors or disk-space flamegraph warnings.
- The baseline commit is visible in generated artifacts or dashboard metadata.
- Paste shape is visible as counters, not a free-form investigation note.
- Partial-snapshot clone cost, full-flatten events, and layout-cache behavior have enough evidence to guide the next phases.
- The large-document scroll row is labeled as either runtime-cache-representative or probe-only.

### Phase 1: Thread maintained line count into snapshots

Goal: make ordinary `DocumentSnapshot::from_shared` close to O(1) by using line-count data already maintained by `BufferState`.

Actions:

- Treat `BufferState::line_count` as the primary source for snapshot display line count where the caller has a `BufferState`.
- Thread line count, and any needed revision guard, through the snapshot construction path instead of calling `display_line_count_from_piece_tree`.
- Keep `display_line_count_from_piece_tree` as a validation, repair, or test-only slow path.
- Add correctness checks for mixed line endings, especially standalone `\r`.
- If the maintained line count cannot safely cover standalone-CR display semantics, add the narrowest standalone-CR display-break metric needed.
- Enforce the full-flatten guardrail for snapshot-cheap paths, covering `flatten_text`, `extract_text`, `search_text`, and `search_text_cow`.

Primary code areas:

- `src/app/domain/buffer/snapshot.rs`
- `src/app/domain/buffer/state.rs`
- `src/app/domain/buffer/analysis.rs`
- `src/app/domain/buffer/piece_tree.rs`

Success criteria:

- `document_snapshot_creation_latency/4194304` drops from 34.770 ms to under 5 ms.
- Snapshot creation no longer appears as a dominant frame in `search_dispatch_profile`.
- Search dispatch improves before worker logic changes.
- Mixed CRLF/LF/CR files keep correct display line counts.

### Phase 2: Dedup before snapshot construction and cheapen result emission

Goal: remove dispatch-time full-document work and bound result-publication allocation.

Actions:

- Move file-identity dedup before `SearchTargetSnapshot` construction.
- For view-scoped collection, collect ordered candidate identities first, choose the winning view/buffer for each identity, then build one target per file.
- For all-tabs collection, avoid building duplicate targets for files already seen in earlier tabs.
- Fix the ordered-view double iteration so duplicate views are not fed into target construction as a normal path.
- Build search targets around cheap snapshot identity, buffer identity, and revision.
- Defer `matched_text` extraction until replace execution or until the UI needs a bounded visible preview.
- Store match ranges and revision guards as the primary result payload.
- Replace full `partial_snapshot` cloning with batched deltas, compact shared snapshots, or a thresholded emit strategy.
- Measure `partial_snapshot` clone time and clone volume directly.

Primary code areas:

- `src/app/app_state/search_state/runtime.rs`
- `src/app/app_state/search_state/helpers.rs`
- `src/app/app_state/search_state/api.rs`
- `src/app/app_state/search_state/replace.rs`
- `src/app/services/search/*`

Success criteria:

- `search_current_dispatch_aggregate_size/128` drops from 48.506 ms to under 12 ms.
- `search_all_dispatch_aggregate_size/128` drops from 48.977 ms to under 20 ms.
- Dispatch snapshot construction count scales with deduplicated files, not views.
- Search result memory grows with retained range metadata and visible previews, not with eager copied match text.
- Replace remains revision-safe and revalidates text at replacement time.
- First-response aggregate latency stays under 1 ms.

### Phase 3: Remove whole-buffer flattening from search completion

Goal: reduce total completion latency for large searches while preserving the already-fast first response.

Actions:

- Target `flatten_text`, `extract_text`, `search_text`, and `search_text_cow` directly; do not add a parallel search API that leaves old full-buffer flattening on the hot path.
- Search over piece spans or bounded text windows where possible.
- Keep `search_text_cow` for genuinely borrowable contiguous text, but treat full-buffer owned flattening as a budget breach for aggregate search.
- Use `record_full_text_flatten` as a regression tripwire for search scenarios.
- Keep the current hard worker cap of 4 until Phase 2 reduces dispatch and allocation pressure.
- Then replace the fixed cap with a request-aware function:
  - Use 1 worker for small requests below 8 MB total target text or fewer than 4 targets.
  - Use up to `min(available_parallelism, 8, target_count)` for larger requests.
  - Cap back down when the request is memory-bound or flattening is observed.
  - Allow a benchmark/runtime override only for profiling and capacity experiments.
- Preserve first response as a protected metric: no regression past 1 ms at the current measured ceiling.

Primary code areas:

- `src/app/domain/buffer/snapshot.rs`
- `src/app/app_state/search_state/worker/processing.rs`
- `src/app/app_state/search_state/worker/fragments.rs`
- `src/app/services/search/*`

Success criteria:

- `search_current_app_state_completion_aggregate_size/256` moves from 1828.762 ms toward the 85 ms budget.
- `search_current_completion_aggregate_size/256` moves from 552.867 ms toward the 55 ms budget.
- `search_all_completion_aggregate_size/256` moves from 329.750 ms toward the 85 ms budget.
- Full-flatten events disappear from aggregate search completion except in explicitly allowed fallback cases.
- First-response aggregate latency stays under 1 ms.

### Phase 4: Narrow metadata and compliance full scans

Goal: stop paste, open, and edit workflows from falling into full `TextInspection` unless the operation truly requires it.

Actions:

- Start from the existing `can_skip_metadata_rescan` and `incremental_text_metadata_after_operation` fast paths.
- Use Phase 0 paste counters to pick the fix:
  - If paste arrives as one edit but misses the incremental path, fix the predicate or stale-state condition.
  - If paste arrives as many edits, add a batched incremental metadata path across multiple edits.
  - If paste is forced to a full scan for compliance or artifact summary reasons, split cheap line-ending/count maintenance from expensive inspection.
- Keep stale flags for expensive fields and refresh them asynchronously or on demand where correctness allows.
- Add `recheck_encoding_compliance` to the scan budget: keep its stale gating, but measure bytes scanned and latency.

Primary code areas:

- `src/app/domain/buffer/state.rs`
- `src/app/domain/buffer/document.rs`
- `src/app/domain/buffer/analysis.rs`
- `src/app/services/file_controller/*`

Success criteria:

- Paste ceiling improves from last OK 8 MB / first failure 64 MB to at least 64 MB OK.
- Large paste elapsed time drops below 150 ms for the current measured profile.
- `paste_stress_profile` is no longer dominated by full `TextInspection`.
- Full metadata scan bytes and encoding-compliance scan bytes are visible as separate metrics.

### Phase 5: Right-size and reshape the existing layout cache

Goal: make editor layout and scroll work scale with visible text and real invalidations, with measurements that represent the in-app path.

Actions:

- Keep the runtime fact explicit: the editor painting/layout path already consults `LayoutCache`.
- Reconcile `src/profile.rs`, `src/bin/resource_probe.rs`, and `src/bin/capacity_probe.rs` with the runtime cache-consulting path before using probe latencies as user-facing frame targets.
- Revisit `MAX_ENTRIES = 8` and the 4 MB cap using split-count, tab-count, and revision-churn measurements.
- Decide whether layout cache lifetime should remain per view or become shared per buffer. Prefer sharing only if invalidation and memory bounds stay simple.
- Slice existing hit/miss metrics by split count, tab count, and revision churn.
- Add or expose layout cache eviction count, warmup count, warmup usefulness, and layout bytes per frame.
- Skip `CharByteMap::build` entirely when there are no search/selection highlights and the path can append one plain segment.
- Reuse highlight boundaries when text, query/highlight state, wrap width, and style have not changed.
- Reduce nearby-slice warmup churn when cache capacity is too small to retain the warmed entries.

Primary code areas:

- `src/app/domain/view/layout_cache.rs`
- `src/app/ui/editor_content/native_editor/layout.rs`
- `src/app/ui/editor_content/native_editor/highlighting.rs`
- `src/app/ui/editor_content/native_editor/painting.rs`
- `src/profile.rs`
- `src/bin/resource_probe.rs`
- `src/bin/capacity_probe.rs`

Success criteria:

- The large-document scroll measurement is confirmed to use the same cache behavior as runtime painting, or it is split into probe-only and runtime-cache rows.
- Runtime large-document scroll drops under one 60 Hz frame, about 16.7 ms, after measurement reconciliation.
- Layout allocation scales with visible lines and invalidation count.
- Cache hit rate remains healthy as split count rises.
- The layout cache does not churn immediately under tab-count and split-count capacity scenarios.

### Phase 6: Improve piece-tree line lookup and viewport extraction

Goal: make viewport extraction consistently logarithmic or viewport-bound.

Actions:

- Use existing piece-tree newline prefix sums more directly in `line_lookup` and `line_index_at_offset`.
- Reduce or eliminate hot-path `scan_piece_for_line_lookup` work for large buffers.
- Add standalone-CR display-break support only where required for display-line correctness.
- Keep the viewport extraction path separate from full snapshot or full metadata repair work.

Primary code areas:

- `src/app/domain/buffer/piece_tree.rs`
- `src/app/domain/buffer/piece_tree/support.rs`
- `src/app/domain/buffer/piece_tree/slice.rs`
- `src/app/ui/editor_content/native_editor/*`

Success criteria:

- `viewport_extraction_latency/4194304` drops from 34.125 ms to under 16 ms.
- `viewport_extraction_profile` is no longer dominated by leaf or piece scanning.
- Large file capacity improves without requiring full-buffer flattening.

### Phase 7: Reduce inactive tab, view, and session scale costs

Goal: avoid retaining or reconstructing heavy editor state for inactive surfaces.

Actions:

- Virtualize inactive tab and split-view editor state.
- Store lightweight descriptors for inactive tabs and hydrate full editor state on demand.
- Deduplicate shared buffer snapshots across views after Phase 1 makes snapshots cheap.
- Align this work with Phase 5's layout-cache lifetime decision: switching tabs or splits should not destroy useful cache entries unnecessarily.
- Move session persist work off the UI path and coalesce saves.
- Persist compact session descriptors instead of reconstructing heavy view state.

Primary code areas:

- `src/app/domain/tab_manager.rs`
- `src/app/domain/tab.rs`
- `src/app/app_state/workspace/*`
- `src/app/services/session_manager.rs`
- `src/app/services/session_store/*`
- `src/app/ui/editor_area/*`
- `src/app/domain/view/layout_cache.rs`

Success criteria:

- Tab count ceiling improves beyond the current last OK 32 tabs / first failure 512 tabs.
- Split count ceiling improves beyond the current last OK 128 splits / first failure 512 splits.
- Session persist no longer blocks for the current 10.504 s worst profile.
- Session restore allocation falls substantially below the current 3.727 GB allocation profile.
- Layout-cache hit rate does not collapse when switching among tabs and splits.

## Recommended Order

1. Clean baseline and add the missing paste, clone, flatten, and layout-cache slices.
2. Thread `BufferState::line_count` into `DocumentSnapshot`.
3. Dedup search targets before snapshot construction and fix ordered-view double iteration.
4. Make result emission cheap and lazy-load `matched_text`.
5. Remove whole-buffer flattening from search completion.
6. Narrow metadata and encoding-compliance full scans.
7. Reconcile layout probes with runtime caching, then right-size and reshape the existing layout cache.
8. Improve piece-tree line lookup and viewport extraction.
9. Virtualize inactive tab/view/session state.

This order keeps the plan focused on shared multipliers first. Snapshot cost appears directly in the snapshot benchmark and indirectly in search dispatch, many-view behavior, and tab/view construction. Layout-cache and virtualization work come later because their correct shape depends on whether cached state is per view, per buffer, or shared across hydrated tab state.

## Success Targets

| Area | Current | Target |
| --- | ---: | ---: |
| 4 MB document snapshot | 34.770 ms | < 5 ms |
| Current search dispatch, 128 aggregate targets | 48.506 ms | < 12 ms |
| All-tabs search dispatch, 128 aggregate targets | 48.977 ms | < 20 ms |
| Current app-state search completion, 256 aggregate targets | 1828.762 ms | < 85 ms |
| Current search completion, 256 aggregate targets | 552.867 ms | < 55 ms |
| All-tabs search completion, 256 aggregate targets | 329.750 ms | < 85 ms |
| Search first response at current 256-target ceiling | 0.336 ms max | < 1 ms |
| 4 MB viewport extraction | 34.125 ms | < 16 ms |
| Large-document scroll | 45.821 ms | Reconcile probe/runtime first, then < 16.7 ms |
| Paste allocation scenario | 345.301 ms | < 150 ms |
| Session persist profile | 10504.315 ms | Nonblocking; foreground capture < 500 ms |

## Guardrails

- Lazy `matched_text` must preserve replace correctness. Use buffer revision guards and revalidate ranges at replacement time.
- Search target dedup must preserve active-buffer and active-tab ordering, including prioritized active view behavior.
- Mixed line endings must remain correct, especially standalone `\r` display breaks.
- Span-based search must preserve regex, whole-word, case-sensitivity, Unicode, and cross-piece match behavior.
- `record_full_text_flatten` should become a regression guard for snapshot-cheap and search-completion paths, covering all aliases that can flatten.
- Metadata deferral must not present stale safety-critical file state as authoritative.
- `recheck_encoding_compliance` must remain correct for non-UTF-8 buffers while becoming visible in scan metrics.
- Layout-cache changes need bounded memory, precise invalidation, and a clear lifetime model across splits and tabs.
- Probe changes must keep benchmark reproducibility while making clear whether they represent runtime cache behavior.
- Parallelism should scale only after allocation-heavy paths are reduced; otherwise it can amplify memory pressure.

## Measurement Additions

Add or expose these measurements as phases begin:

- Snapshot slow-path count and snapshot line-count source.
- Full-text flatten count by call site, including `flatten_text`, `extract_text`, `search_text`, and `search_text_cow`.
- Search candidate view count, deduplicated file count, snapshot construction count, and snapshot construction time per retained target.
- Ordered-view duplicate count before target construction.
- `partial_snapshot` clone duration, cloned match count, cloned group count, and emitted bytes.
- Eager versus lazy `matched_text` bytes.
- Paste operation count, edit count, inserted bytes, metadata path result, and full-rescan reason.
- Metadata full-scan bytes, incremental metadata bytes, and skipped-rescan count.
- Encoding-compliance scan bytes and latency.
- Layout cache hit/miss rate sliced by split count, tab count, and revision churn.
- Layout cache eviction count, warmup count, warmup hit usefulness, and layout bytes per frame.
- Probe/runtime layout-path label for scroll and layout latency rows.
- Session foreground capture time versus background serialization time.

## Final Recommendation

Start with the narrowest code delta: thread the existing `BufferState::line_count` into snapshots, then dedup search targets before snapshot construction. At the same time, make full-document flattening and paste shape visible enough that later phases cannot drift into guesswork.

After that, reduce result-emission allocation and search completion flattening before increasing worker parallelism. Treat Phase 5 as cache shaping and measurement reconciliation, because the runtime editor already uses `LayoutCache`; the unknown is whether its current capacity, lifetime, and probe coverage match real user scroll behavior.
