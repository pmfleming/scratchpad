# Progress Review: `peformance-experiment` branch

Date: 2026-05-11

Commit reviewed: `a6176a39` "Save current workspace changes" (single commit on `origin/peformance-experiment`, branched from `e7ae51eb`).

## Verdict

Solid Phase 0 and Phase 1 work. Phase 2 is partially done — dedup is correctly moved before snapshot construction, but `matched_text` extraction is still eager. A few smaller wins from Phase 4 and Phase 5 are in. Two correctness concerns need attention before this lands: a standalone-CR scan that miscounts CRLF, and an absent validation test for the new line-count path. Phase 2's `MIN_CHARS` parallelism widening is ahead of where the plan ordered it.

## Phase-by-phase

### Phase 0 — clean baseline & measurements ✓ (almost complete)

`src/app/capacity_metrics.rs` (+194 lines) adds exactly the meters the review asked for:

- `snapshot_slow_line_count_count` / `snapshot_metadata_line_count_count` — slow-path vs fast-path snapshot construction.
- `search_candidate_view_count`, `search_ordered_view_duplicate_count`, `search_deduplicated_file_count`, `search_target_snapshot_count/time_ns` — dispatch-shape visibility.
- `search_eager_matched_text_bytes`, `search_partial_snapshot_*` (count/time/match/group/bytes) — clone-cost visibility.
- `paste_operation_count`, `paste_edit_count`, `paste_inserted_bytes` — paste shape (concrete counter, as the review recommended).
- `metadata_skip_count`, `metadata_incremental_count`, `metadata_full_scan_count/bytes`, `encoding_compliance_scan_*` — metadata path mix.
- `layout_cache_eviction_count`, `layout_cache_evicted_bytes`, `layout_cache_warmup_count`, `layout_plain_fast_path_count` — layout-cache and fast-path visibility.

Recording sites are wired in correctly (e.g. paste shape recorded at [state.rs around line 338](src/app/domain/buffer/state.rs:338), encoding scan timed at line 432, partial-snapshot bytes computed before the clone at [helpers.rs:80](src/app/app_state/search_state/helpers.rs:80)).

Gap: no published baseline snapshot of these meters yet. The plan's Phase 0 success criterion was "the baseline commit is visible in generated artifacts." That hasn't happened — the `target/analysis/*.json` artifacts still reflect master.

### Phase 1 — thread maintained line count into snapshots ✓ (with a correctness gap)

[snapshot.rs:24-44](src/app/domain/buffer/snapshot.rs:24): `from_shared` becomes the slow path (records `record_snapshot_slow_line_count`); new `from_shared_with_line_count` is the fast path. [state.rs:244](src/app/domain/buffer/state.rs:244) routes `document_snapshot()` through the fast path with `self.line_count`. This is the right shape.

**Correctness concern — equivalence not validated.** `BufferState::line_count` is populated from `TextInspection::inspect(text).line_count`, while the old path used `display_line_count_from_piece_tree` which calls `TextInspection::inspect_spans(...)`. These should agree because both go through `TextInspection`, but the plan asked for "a targeted correctness check for mixed line endings, especially standalone `\r`." No test or debug assertion was added. At minimum, a debug-build `debug_assert_eq!` cross-check, or a unit test exercising standalone-CR / CRLF / LF / mixed buffers and comparing the two paths, should land with this change.

### Phase 2 — search dispatch and result snapshots (partial)

**Done well.** [helpers.rs:203-260](src/app/app_state/search_state/helpers.rs:203) restructures `collect_search_targets_for_views` so dedup happens **before** `build_search_target_from_view` is called. The expensive `buffer.document_snapshot()` now runs once per file rather than once per view. Tracking is done via `view_by_file` / `ordered_files` / `seen_view_ids`. This directly addresses the framing problem the review flagged. The `file_identity` field is removed from `SearchTargetSnapshot` ([worker.rs:30](src/app/app_state/search_state/worker.rs:30)) since dedup no longer needs it downstream — clean follow-through. The double-iteration smell at [runtime.rs:248-251](src/app/app_state/search_state/runtime.rs:248) is preserved but is now harmless because the inner `seen_view_ids` check at helpers.rs:217 catches the duplicate view IDs (and records `record_search_ordered_view_duplicate` so you can see how often it fires).

`should_publish_partial`'s `PARTIAL_MATCH_DELTA` bumped from 64 to 256 ([processing.rs:281](src/app/app_state/search_state/worker/processing.rs:281)) — a defensible thresholded-emit mitigation matched with metrics, not a full restructure. Reasonable as a Phase 0/2 split.

**Still pending.** [helpers.rs around line 25](src/app/app_state/search_state/helpers.rs:25): `push_matches` still calls `target.document_snapshot.piece_tree().extract_range(range)` eagerly for every match. The plan's action "Defer `matched_text` extraction until replace execution or until the UI needs a bounded visible preview" is not implemented; the only change is adding `record_search_eager_matched_text(matched_text.len())` to measure it. That's appropriate for now (instrument before you cut), but it should be called out as deliberately deferred.

**Ahead of order.** Parallelism was widened in this same commit ([processing.rs:12-15](src/app/app_state/search_state/worker/processing.rs:12)): `SEARCH_TARGET_PARALLELISM_CAP` 4 → 8, added `SEARCH_TARGET_PARALLELISM_MIN_CHARS = 8 MB`. The plan reserves this for **after** Phase 3 reduces flattening, because parallelism multiplies allocation pressure. The MIN_CHARS gate is exactly the request-aware function the plan asked for, so the gating is right; but the cap bump should be reverted (or held behind a feature flag) until Phase 3's flatten-removal lands, otherwise it can regress memory-bound capacity scenarios.

### Phase 3 — remove whole-buffer flattening

Not started. Correct — recommended order had Phase 2 first.

### Phase 4 — narrow metadata scans (partial)

Two pieces in:

- `incremental_text_metadata_after_operation` ([state.rs:608-643](src/app/domain/buffer/state.rs:608)) now loops over all edits instead of bailing when `operation.edits.len() != 1`. Each edit is fed through `buffer_text_metadata_from_edit` with the running `line_count` and `artifact_summary`. If any single edit fails the incremental predicate, the function returns `None` (because `?` on `buffer_text_metadata_from_edit` exits early) and the caller falls through to a full rescan. That short-circuit behavior is reasonable.
- `encoding_compliance_scan` now measured ([state.rs:432-441](src/app/domain/buffer/state.rs:432)) with bytes and elapsed.
- `record_metadata_full_scan(piece_tree().len_bytes())` ([state.rs:399](src/app/domain/buffer/state.rs:399)) records the full-rescan byte cost.

**Concern — partial state on early bail.** Inside the loop at [state.rs:612-635](src/app/domain/buffer/state.rs:612), each iteration calls `buffer_text_metadata_from_edit(line_count, &artifact_summary, &mut self.format, ...)`. That helper mutates `self.format.line_ending_counts` and `is_ascii_subset` when it succeeds. If edit 3 of 5 returns `None`, the function returns `None` — but `self.format` has already been mutated by edits 1 and 2. The caller falls through to `refresh_text_metadata()` which recomputes everything via `buffer_text_metadata_from_piece_tree`, so the final state ends up correct. But this only works because the fallback is a full recompute that overwrites format. If anyone ever shortens that fallback, the partial mutation becomes a latent bug. Worth a comment at the call site or — cleaner — accumulating proposed format mutations into a local and only committing on full success.

### Phase 5 — layout cache (partial; mostly correctness scoping)

- LayoutCache capacity bumped: `MAX_ENTRIES` 8 → 32, `MAX_BYTES` 4 MB → 16 MB ([layout_cache.rs:37-38](src/app/domain/view/layout_cache.rs:37)). Plausible but un-evidenced — Phase 5 should validate that 32/16 MB is right for the tab/split capacity scenarios before locking it in.
- Plain-text fast path added ([highlighting.rs:106-110](src/app/ui/editor_content/native_editor/highlighting.rs:106)): when there's no selection and no search highlights, skip `CharByteMap::build` and `highlight_boundaries` entirely. Sensible — pairs with the existing ASCII branch of `CharByteMap`.
- Eviction and warmup meters wired ([layout_cache.rs:83](src/app/domain/view/layout_cache.rs:83), [layout.rs:467](src/app/ui/editor_content/native_editor/layout.rs:467)).

**Probe reconciliation done.** This was the headline correctness gap from the prior review. [profile.rs:414-425](src/profile.rs:414), [capacity_probe.rs:296-310](src/bin/capacity_probe.rs:296), [resource_probe.rs:390-402](src/bin/resource_probe.rs:390) now wrap their `build_layouter` callers in a small per-wrap-width `HashMap` and record cache hit/miss against the shared counters. This means the scroll/layout numbers the probes produce will start to reflect cache reuse and so are comparable to in-app behavior. Good fix.

The probe cache is per-call-site and per-wrap-width only — it doesn't share keys with the real `LayoutCache`, and a fresh `HashMap` is constructed inside each rendering scope (e.g. inside the closure at [resource_probe.rs:389](src/bin/resource_probe.rs:389)). For `render_first_visible_text_paint` that closure runs once per call so the cache is always cold and the "hit" branch will never fire — only `record_layout_cache_miss` will ever record. That looks like an oversight: the HashMap should hoist outside the closure if you want hit measurement.

### Phase 6 — piece-tree line lookup (correctness bug)

[support.rs:288-300](src/app/domain/buffer/piece_tree/support.rs:288): `scan_piece_for_line_lookup` now treats both `\n` and `\r` as line breaks via `matches!(ch, '\n' | '\r')`. **This miscounts CRLF.** For text `foo\r\nbar` scanning for line 1:

1. `\r` hits the second branch → `current_line` becomes 1, `line_start = 4`, `current_len = 0`.
2. `\n` hits the first branch (now `current_line == safe_line`) → returns `Some(line_info())` immediately with `current_len = 0`.

Result: line 1 reported as empty starting at offset 4, when it should be "bar" starting at offset 5. The scan also disagrees with `piece.newline_count` (used at [support.rs:260, 274](src/app/domain/buffer/piece_tree/support.rs:260)), which still counts only `\n`. Cross-piece traversal and within-piece traversal now use different definitions of "line break."

Fix shape: collapse CR+LF, and only count standalone CR. Either pre-scan the piece to detect CRLF pairs, or use a peekable iterator that skips `\n` when the previous char was `\r`. Whatever you do, `piece.newline_count` needs to grow a sibling `standalone_cr_count` (or a unified `display_break_count`) so the cross-piece arithmetic matches.

This bug is masked today because line lookup is only called from snapshot/viewport paths, and most test fixtures are LF-only. A targeted test with `\r\n`, `\n`, and standalone `\r` content would catch it.

### Phase 7 — tab/view/session virtualization

Not started. Correct order.

## Summary

| Phase | State | Notes |
| --- | --- | --- |
| 0 | Done in code | No baseline artifact captured yet |
| 1 | Done | Add CRLF/CR/LF equivalence test for `line_count` paths |
| 2 | Partial | Dedup-before-snapshot done; `matched_text` deferral not done; parallelism cap bump is ahead of order |
| 3 | Not started | Order is correct |
| 4 | Partial | Multi-edit incremental path works; tighten format-mutation atomicity |
| 5 | Partial | Capacity bumped, fast path added, probes reconciled — verify probe HashMap scope |
| 6 | Buggy fix | CRLF double-count regression in `scan_piece_for_line_lookup` |
| 7 | Not started | Order is correct |

## Recommended next moves, in order

1. **Revert or fix the `\r` handling in `scan_piece_for_line_lookup`.** Currently a CRLF regression; ship a test fixture covering standalone CR before re-attempting.
2. **Add the Phase 1 cross-check test.** Either `debug_assert_eq!(self.line_count, display_line_count_from_piece_tree(...))` in `document_snapshot()` for debug builds, or a unit test that exercises mixed line endings against both paths.
3. **Hoist the probe `HashMap` outside the rendering closure in `resource_probe.rs`** so cache hits are actually measurable.
4. **Decide on parallelism cap timing.** Either revert `SEARCH_TARGET_PARALLELISM_CAP` to 4 until Phase 3 lands, or gate the bump behind a flag.
5. **Capture a baseline metrics snapshot** (run `open-overview -FullUpdate` from the branch tip) and commit the resulting `target/analysis/*.json` summary as a baseline so subsequent phases have a real before/after.
6. **Tighten Phase 4 format mutations** — accumulate proposed `format` changes locally and only commit on full incremental success.
7. **Then** start Phase 3 (flatten removal) and the `matched_text` deferral as the next coherent slice.
