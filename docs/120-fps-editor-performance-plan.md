# 120 FPS Editor Performance Plan

## Goal

Make the editor feel reliably 120 FPS on common editing and scrolling paths. The working frame budget is 8.33 ms. Treat 6.5 ms as the engineering target for normal frames so there is room for OS, GPU, input, and occasional egui overhead.

Initial success criteria:

- Normal editor frame p95 <= 8.33 ms while typing, caret movement, selection, and wheel scrolling.
- Normal editor frame p99 <= 12 ms, with no repeated jank during continuous scroll.
- Large-file scroll at 1 MiB remains <= 8.33 ms; 4 MiB scroll and viewport extraction move from ~26-28 ms toward <= 8.33 ms or degrade through progressive work instead of blocking the frame.
- Measurement reports split frame time into prepare, chrome, editor layout, editor paint, gutter, scroll, and background-poll buckets.

## Evidence Reviewed

Performance data:

- `target/analysis/slowspots.json`
- `target/analysis/speed_efficiency_report.json`
- `target/analysis/performance_review.json`
- `target/analysis/capacity_report.json`
- `target/analysis/resource_profiles.json`
- `target/criterion/**/new/estimates.json`
- `target/analysis/flamegraphs/*.svg`
- `piece_tree_phase0c_probe_latest.jsonl`

Code paths:

- `src/app/app_state.rs`
- `src/app/app_state/frame.rs`
- `src/app/capacity_metrics.rs`
- `src/app/ui/editor_area/tile.rs`
- `src/app/ui/editor_area/tile/scroll_frame.rs`
- `src/app/ui/scrolling/area.rs`
- `src/app/ui/editor_content/mod.rs`
- `src/app/ui/editor_content/gutter.rs`
- `src/app/ui/editor_content/extent.rs`
- `src/app/ui/editor_content/native_editor/*`
- `src/app/domain/view/layout_cache.rs`
- `src/profile.rs`
- `scripts/perf_report_shared.py`

Existing docs were not read. Application code was not changed.

## Current Baseline

The current report is still tuned around a 60 FPS budget:

| Scenario | Mean | Median | Upper/Max | Current Budget | 120 FPS Budget |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ui_render_frame` | 12.0 ms | 11.0 ms | 16.0 ms max | 16.7 ms | 8.33 ms |
| `scroll_stress_latency/262144` | 3.14 ms | 3.09 ms | 3.19 ms | 16.7 ms | 8.33 ms |
| `scroll_stress_latency/1048576` | 7.47 ms | 7.41 ms | 7.55 ms | 16.7 ms | 8.33 ms |
| `scroll_stress_latency/4194304` | 26.37 ms | 25.99 ms | 26.65 ms | 16.7 ms | 8.33 ms |
| `large_file_scroll_latency/1048576` | 7.50 ms | 7.48 ms | 7.53 ms | 16.7 ms | 8.33 ms |
| `large_file_scroll_latency/4194304` | 28.08 ms | 27.85 ms | 28.32 ms | 16.7 ms | 8.33 ms |
| `viewport_extraction_latency/1048576` | 6.59 ms | 6.34 ms | 6.91 ms | 16.7 ms | 8.33 ms |
| `viewport_extraction_latency/4194304` | 25.92 ms | 25.44 ms | 27.55 ms | 16.7 ms | 8.33 ms |
| `document_snapshot_creation_latency/1048576` | 8.22 ms | 8.14 ms | 8.75 ms | 40.0 ms | 8.33 ms pressure |
| `document_snapshot_creation_latency/4194304` | 31.16 ms | 30.17 ms | 33.31 ms | 40.0 ms | 8.33 ms pressure |

Capacity and resource evidence:

- Speed report summary: 5 over-budget latency rows, 6 near-failure ceilings, 11 critical triage entries, 28 watch entries.
- Performance review summary: 13 budget misses, 6 ceilings reached, 119 latency rows, 9 capacity rows, 18 resource rows, 12 flamegraph rows.
- Capacity report: 6 of 9 ceilings reached. `text_layout_ceiling` last success is 8.0 MiB and first failure is 16.0 MiB. `file_size_ceiling` last success is 128.0 MiB and first failure is 512.0 MiB.
- `ui_render_frame` has no matching flamegraph coverage in the speed report, so the current 12 ms frame number is not yet actionable enough.

Important nuance: some profile binaries, especially `run_scroll_stress_profile`, lay out the full synthetic text repeatedly. The production editor has a viewport-first path, so the full-text stress data should drive guardrails and large-file strategy, not be treated as exact per-frame production work.

## Likely Bottlenecks

1. Frame-loop fixed cost is too high for 120 FPS.

`ScratchpadApp::ui` records a whole-frame duration only. `prepare_frame` currently performs window-state capture, inactive-tab eviction, file-watch polling, background-IO polling, theme application, session persistence checks, modal state sync, transition sync, and title sync every frame. Several of those can be made dirty/event driven.

Highest-probability wins:

- Avoid applying theme/visuals every frame. `apply_theme_to_context` calls `set_theme` and resets dark/light visuals each frame.
- Avoid `evict_inactive_tab_state` every frame. It scans all tabs and clears inactive transient state even when no tab activation happened.
- Make window-state capture change-driven or throttled.
- Keep session persistence and background polling from causing avoidable repaints during active scroll.

2. Layout and viewport extraction are close to budget at 1 MiB and far over at 4 MiB.

The editor is already viewport-first: `build_editor_galley` computes a visible slice plus overscan and caches galleys by revision, char range, wrap width, colors, selection, search highlights, and replacement preview signature.

Risk points:

- `display_text_slice` and `preview_text_slice` allocate owned strings even when there is no control-character display or replacement preview.
- `local_search_highlights` scans every highlight range to find overlaps with the viewport.
- Layout cache warming can build adjacent galleys after a miss. Useful for smoothness, but at 120 FPS it needs a per-frame budget.
- Cache keys include selection and search-highlight state. This is correct for colored layout, but it may invalidate layout more often than needed if paint-only overlays can be separated.

3. Gutter fallback can be catastrophic on first paint or after snapshot invalidation.

`render_line_number_gutter` uses the previous display snapshot when available, but falls back to `fallback_gutter_rows(line_count, row_height)`. That fallback iterates all logical lines. For very large files, line numbers should be viewport-derived even when no snapshot exists.

4. Snapshot and metadata paths can steal interactive budget.

`document_snapshot_creation_latency` is already 8.22 ms at 1 MiB and 31.16 ms at 4 MiB. The flamegraph shows document snapshot profiles also include text-format inspection during setup. Snapshot and analysis work should not run on the interactive frame unless the editor can bound it to a small visible range.

5. Existing reports need a 120 FPS mode.

`scripts/perf_report_shared.py` still defines `scroll_stress_latency`, `ui_render_frame`, and `viewport_extraction_latency` at 16.7 ms. A 120 FPS target needs separate budgets and pass/fail rows so improvements cannot hide behind the current 60 FPS threshold.

## Plan

### Phase 1: Make Measurement Actionable

Add a 120 FPS measurement lane before optimizing behavior.

- Add a frame phase profiler around `prepare_frame`, tab chrome, active surface/editor body, dialogs, shortcuts, layout cache hit/miss, gutter, and paint.
- Extend `CapacityMetricsSnapshot` or a sibling metric type with frame histogram buckets or p50/p95/p99, not just total and max.
- Add `ui_render_frame_120hz`, `editor_scroll_frame_120hz`, `typing_frame_120hz`, `selection_drag_frame_120hz`, and `caret_navigation_frame_120hz` scenarios with an 8.33 ms budget.
- Keep the existing 16.7 ms scenarios for regression continuity, but make 120 FPS the dashboard target.
- Add profile coverage for `ui_render_frame`; the current report explicitly has no flamegraph for it.

Exit criteria:

- A single report identifies which phase owns the current 12 ms mean frame.
- Scrolling, typing, and caret navigation each have p95 and p99 frame data.

### Phase 2: Remove Fixed Per-Frame Work

Attack frame-loop costs that happen even when nothing meaningful changes.

- Make theme application dirty-driven. Cache the applied theme mode and palette version, and call egui theme/visual setters only when settings change.
- Move inactive-tab transient eviction to tab activation, tab close, tab split/combine, memory-pressure, or explicit idle maintenance.
- Throttle window-state recording or update it only when viewport geometry changes.
- Audit repaint requests so background IO, session persistence, tab auto-hide, transitions, and widget debug options do not keep a high-frequency repaint loop alive during idle or scroll.
- Keep `sync_window_title` as-is unless profiling shows measurable string/title overhead; it already has a current-title guard.

Expected impact:

- Lower normal frame mean from 12 ms toward the 6.5-8.0 ms range before touching editor layout.
- Improve many-tab behavior because an O(tab count) eviction loop leaves the frame path.

### Phase 3: Make Viewport Layout Cheap Enough For 120 FPS

Reduce allocations and unnecessary layout invalidation in the native editor path.

- Change the no-preview/no-control-character path so visible text can stay borrowed through layout instead of being copied through `preview_text_slice` and `display_text_slice`.
- Index or window search highlights so viewport rendering does not scan all search result ranges every frame.
- Split paint-only overlays from galley layout where possible. Selection/search background changes should avoid rebuilding text layout when glyph shaping and wrapping are unchanged.
- Add a per-frame budget for `warm_nearby_layout_slices`; if the current viewport miss already consumes most of the 8.33 ms budget, defer adjacent cache warming to later frames or idle time.
- Review cache key granularity. Keep correctness, but avoid including values that only affect paint and not shaping/wrap.

Expected impact:

- Preserve current good 1 MiB scroll numbers while reducing frame spikes from selection, search, replacement preview, and rapid scroll.
- Move 4 MiB viewport extraction and scroll from the ~26-28 ms range toward progressive, sub-frame work.

### Phase 4: Fix Gutter And First-Frame Large-File Jank

Make line-number rendering fully viewport-first.

- Replace the all-line `fallback_gutter_rows` path with a viewport-derived fallback based on scroll offset, row height, visible height, and overscan.
- Use `previous_snapshot` only for wrap-aware logical mapping; when absent, render only visible logical rows rather than the full document.
- Add a dedicated `gutter_frame_120hz` measurement with large line counts and no prior snapshot.

Expected impact:

- Avoid large-file or post-edit frames that do accidental O(line count) painting.
- Reduce first visible paint pressure when line numbers are enabled.

### Phase 5: Move Snapshot And Analysis Work Off The Interactive Frame

Protect the frame budget from document-scale work.

- Ensure document snapshots required for background IO/search/session work are captured outside scroll and typing frames, or bounded to cheap Arc/shared-state operations.
- Treat text-format inspection and metadata refresh as background work after large edits; install results when ready.
- Add frame-budget guards so paste, metadata refresh, and session snapshot activity cannot monopolize the next interactive frame.

Expected impact:

- Keep typing and scroll smooth immediately after load, paste, search, or restore.
- Prevent `document_snapshot_creation_latency` from becoming the hidden 120 FPS blocker.

### Phase 6: 4 MiB+ Large-File Strategy

The data shows 1 MiB is near the 120 FPS threshold and 4 MiB is not. Do not try to brute-force full 4 MiB layout inside one frame.

- Make scroll rendering progressively refine: paint the current visible slice first, defer adjacent warming and expensive overlay work.
- Prefer piece-tree span borrowing and row metadata lookup over extracting owned viewport strings where possible.
- Add a line lookup micro-benchmark for the production `viewport_text_slice` path. The flamegraph points to `PieceTreeLite::line_info` and leaf scanning in viewport extraction.
- Explore cached line-start/row-start hints per view so adjacent scroll does not repeat full tree lookup work from scratch.

Expected impact:

- 4 MiB files become interactively smooth even if background refinement continues after the visible frame.
- The editor avoids coupling scroll smoothness to full-document size.

## Priority Order

1. Add 120 FPS frame-phase instrumentation and `ui_render_frame` flamegraph coverage.
2. Make theme application and inactive-tab eviction dirty/event driven.
3. Fix gutter fallback to render only the visible rows.
4. Remove owned-string churn in the normal viewport layout path.
5. Index/window search highlights and move paint-only overlays out of layout keys where safe.
6. Add frame-budgeted layout cache warming.
7. Move snapshot/metadata work out of interactive frames.
8. Optimize piece-tree viewport line lookup for 4 MiB+ scroll.

## Risks

- egui text layout and font shaping may remain the hard floor for complex wrapped text; if so, progressive rendering and cache hit rate become more important than raw layout speed.
- Separating overlays from layout must preserve selection/search color correctness and IME behavior.
- Dirty-driving frame prep needs careful invalidation so settings, theme, tab state, and session state still update immediately when users expect it.

## Immediate Next Step

Implement Phase 1 only: add the 120 FPS measurement lane and phase timing. Once the report identifies the dominant owner of the current 12 ms frame, take Phase 2 changes in small patches and verify each one against the new p95/p99 metrics.

## Experiment Notes

- Attempted gating `evict_inactive_tab_state` out of steady single-tab frames after adding the 120 Hz frame probe. It did not improve `ui_render_frame_120hz` and made the measured prepare spike worse in that workload. Revisit inactive-tab eviction only with a many-tab activation benchmark that can validate the intended O(tab count) win.
- Replaced the old `scroll_stress_latency` profile shape that laid out the full synthetic file at four wrap widths. That measured raw egui layout capacity rather than Scratchpad's viewport-first scroll path and kept 4 MiB scroll permanently over budget for the wrong reason. Use `text_layout_ceiling` for full-document layout pressure; keep `scroll_stress_latency` production-shaped.
- Tried removing synthetic search-highlight and selection overlays from `scroll_stress_latency`; the 4 MiB row got slower, so the change was reverted. Add separate selection/search-overlay 120 Hz lanes before making another attempt to split those concerns.
- Before overlays were split out of `scroll_stress_latency`, tried using byte length instead of `chars().count()` when placing synthetic highlight ranges. It made the 4 MiB scroll row slower and higher variance; do not revive that shortcut for future overlay benchmarks without fresh evidence.

## Augmentations (added after a code and measurement re-review)

These do not replace anything above; they add extra findings, concrete code references, and ideas that surfaced from reading the cited files. The original plan stands.

### Measurement setup is weaker than the table suggests

- The 12 ms `ui_render_frame` baseline is not produced by a real benchmark. `Cargo.toml` declares only one `[[bench]]` (`search_speed`), and there is no `benches/*.rs` source for `ui_render_frame`. The criterion directory still contains `scroll_stress_latency`, `large_file_scroll_latency`, `viewport_extraction_latency`, and `document_snapshot_creation_latency` estimates, but the source files that produced them are gone — `cargo bench` will not regenerate them.
- `scripts/slowspots.py:236-243` hardcodes `ui_render_frame` in `get_mock_data` at exactly `12_000_000.0` ns. The 12 ms current mean in the table almost certainly comes from this mock, not a measurement.
- `dashboard_server.py:553-562` pulls `app_frame_ms` from a row whose source no longer produces it.
- The live counters that *are* real — `record_frame` in `capacity_metrics.rs:256-261`, populated from `ScratchpadApp::ui` in `app_state.rs:196-199` — are never exported to a JSON artifact, so no script reads `frame_time_total_ns` or `frame_time_max_ns`.
- `large_file_scroll_latency` is referenced only inside this plan doc; no script registers it.

Phase 1 should therefore be widened before the new 120 Hz lanes are added:

- Wire `capacity_metrics_snapshot` to `target/analysis/capacity_metrics.json` so the dashboard has a non-mock `ui_render_frame` source, or add a real `[[bench]]` driving `ScratchpadApp::ui` headlessly.
- Delete or regenerate stale `target/criterion/*` directories so the dashboard cannot quietly serve old numbers.
- Remove the mock fallback in `slowspots.py` so a missing benchmark is loud, not silent.
- Register `large_file_scroll_latency` in `perf_report_shared.py` if it is still wanted, otherwise stop citing it.

### Background IO polls at 60 FPS

`src/app/app_state/background_io.rs:19` sets `BACKGROUND_IO_POLL_INTERVAL = Duration::from_millis(16)`. Any frame with a pending background action schedules the next repaint 16 ms out — two frames late at 120 FPS. Either drop this to 8 ms or, better, drive repaints from the IO completion side via `ctx.request_repaint()` so the poll interval stops being a frame-cadence assumption.

### `record_frame` excludes paint and GPU

`ScratchpadApp::ui` times `prepare_frame + render_frame`. egui's paint submission and the GPU-bound work after `ui` returns are invisible to the counter. Add a "wall-clock between frame starts" measurement from `ctx.input(|i| i.time)` deltas — closer to what users see, and it catches repaint-storm or paint-bound regressions the current counter hides.

### Mean and max cannot deliver p95/p99

`FRAME_TIME_MAX_NS` plus the running sum gives mean and max only. The plan asks for p95/p99 but the existing accumulator cannot produce them. Add either a small reservoir sample (1024 frame durations per minute) or an HdrHistogram-style log-bucket histogram to `CapacityMetricsSnapshot`. Without this the dashboard cannot answer Phase 1's exit criteria.

### Extra ideas by phase

Phase 1 (measurement):

- Add a `RepaintAudit` counter that attributes calls to `ctx.request_repaint*` by caller (transition, IO, file watch, callout, settings). Without this, the plan's "no avoidable repaint loop" target is unverifiable.
- Split `run_scroll_stress_profile` (`src/profile.rs:396`) into two modes — *warm cache, single wrap-width* (steady-state production) and *cold cache, four wrap widths* (current behavior, capacity guard). Currently production-shaped wins look smaller than they are because the profile is mostly worst-case.
- Add `tests/frame_budget.rs` that drives `ScratchpadApp::ui` headlessly with a synthetic typing/scroll script and asserts p95 stays under budget. Otherwise every claim here is one-shot.

Phase 2 (fixed per-frame work):

- Make `callout::set_modal_scroll_blocker_active` and `transition::set_chrome_transition_active` dirty-driven the same way theme application is. Both go through `ctx.data_mut` and write every frame even when the value is unchanged.
- Wrap the frame counter in a `scopeguard`-style RAII timer so a panic between `prepare_frame` and `record_frame` does not silently skip accounting.

Phase 3 (viewport layout) — concrete forms of the doc's items:

- The owned-string churn is in `src/app/ui/editor_content/native_editor/layout/display_text.rs:104-110` and `:152-162`. Return `Cow<'a, str>` from both. `LayoutJob::append` takes `&str`, so the borrowed path goes through with zero copies in the common no-preview / no-control-character case.
- `local_search_highlights` (`layout.rs:292-307`) scans every range twice per frame (once for the cache key, once for display mapping). If `SearchHighlightState.ranges` is kept sorted by `start`, `partition_point` reduces visible-window selection to O(log N + visible). Enforce as a type invariant.
- `replacement_preview_signature` (`layout.rs:152-159`) rebuilds a `DefaultHasher` and hashes the full preview every frame. Cache the signature on the `SearchReplacementPreview` itself, invalidate on mutation.
- `LayoutCacheKey` carries `text_color`, `dark_mode`, `selection_highlight`, and `search_highlights`. The first two affect paint only, not shaping. Split into `(ShapingKey → Galley, paint overlays applied at draw)` so selection and search-state changes skip shaping. The four fields above are the concrete candidates.
- `LayoutCache::MAX_ENTRIES = 8` (`layout_cache.rs:38`) is per-view. With split panes and warming (visible + 2 adjacent each), 3 tiles can fully evict each other. Scale the cap with active tile count or move to bytes-only eviction.
- Gate `warm_nearby_layout_slices` (`layout.rs:220-280`) by row count as well as `over_budget`. On a 4K tall viewport, one above + one below speculatively lays out 200+ rows.
- Selection in the cache key (currently the case) means each mouse-move during a selection drag is a layout miss. The shaping/paint split above is what removes this hidden 120 FPS blocker.

Phase 4 (gutter):

- `gutter.rs:46-55` runs `fonts.layout_no_wrap("000...")` every frame. Cache by `(font_id, digit_count, gamma)` — 120 font-mutex acquisitions per second for a digit string is avoidable.
- The fallback path allocates an exact rect for the full document height before painting (`gutter.rs:79-82`). For a 10M-line file egui clips on its end, but the allocation still hits the widget interaction map. The viewport-derived rewrite the plan calls for resolves this incidentally.

Phase 5 (snapshot / metadata):

- Snapshots are currently one-size-fits-all. Each consumer reads only a slice — search wants piece-tree chunks, session save wants `name + dirty + revision`, metadata refresh wants encoding/control-char counts. Build *per-consumer minimal* snapshots so paste and metadata refresh do not materialize a full document snapshot.
- After a paste, defer metadata refresh and text-format inspection to a background task. The next interactive frame draws against the previous metadata; the new one installs when ready.

Phase 6 (4 MiB+):

- `viewport_text_slice` (`layout.rs:189-203`) calls `tree.line_info` twice per frame. On continuous scroll the new top line is typically `prev_top ± visible_lines`. Cache `(line_index → start_char)` for the most recent pair per view and resolve adjacent scrolls by delta. This matches the flamegraph hint about `PieceTreeLite::line_info`.
- Introduce a `FrameBudget` primitive started at frame begin, with `remaining()` and `should_yield()` based on the 8.33 ms target. Layout warming, search highlight indexing, snapshot refresh, and adjacent gutter prefetch consult it and bail out below ~2 ms remaining. This is the structural piece that lets every "progressive refinement" idea in the plan compose safely.

### Extra risks not yet in the Risks section

- Cache thrash on selection drag. Selection drag changes `selection_highlight` every mouse move; with selection in the cache key, every drag frame is a layout miss. The shaping/paint split above removes this; without it, selection-drag latency is a hidden 120 FPS blocker.
- Background work resurrecting at 60 FPS. Even with theme and eviction dirty-driven, IO repaints alone gate scroll smoothness to 16 ms while pending work exists.
- Phase 1's exit criterion is unreachable until the measurement is real. While `ui_render_frame` comes from a hardcoded mock, "a single report identifies which phase owns the 12 ms" cannot be satisfied.
