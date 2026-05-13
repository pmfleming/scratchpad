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
