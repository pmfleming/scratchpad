# Scrolling and Visible-Window Rebuild Plan

Date: 2026-04-27

## Goal

Remove the current editor scrolling, visible-window cursor-follow behavior, and their associated tests. Replace them with a new local implementation that uses:

- Zed-style ownership patterns for scroll state, display rows, cursor reveal, and viewport rendering.
- A locally maintained egui-derived scroll container, copied and adapted from egui rather than treated as an opaque dependency.
- A fresh test suite written against the new behavior, not the old implementation details.

This plan intentionally does not preserve current scrolling dependencies. Existing scroll and visible-window helpers, state fields, debug hooks, profiles, and tests should be treated as disposable unless they still serve a non-scroll editor responsibility after the rewrite.

## Reference Sources

- Zed editor scroll manager: https://github.com/zed-industries/zed/blob/main/crates/editor/src/scroll.rs
- Zed scroll actions: https://github.com/zed-industries/zed/blob/main/crates/editor/src/scroll/actions.rs
- Zed editor element/render pipeline: https://github.com/zed-industries/zed/blob/main/crates/editor/src/element.rs
- egui scroll area implementation: https://github.com/emilk/egui/blob/master/crates/egui/src/containers/scroll_area.rs
- egui UI scroll targets: https://github.com/emilk/egui/blob/master/crates/egui/src/ui.rs
- egui response scroll helpers: https://github.com/emilk/egui/blob/master/crates/egui/src/response.rs

## Current Local Surface To Remove

The current implementation is spread across several layers:

- `src/app/ui/editor_area/tile.rs`
  - `egui::ScrollArea::both()`
  - `show_editor_scroll_area`
  - `sync_editor_scroll_state`
  - `editor_scroll_source`
  - `resolve_editor_scroll_offset`
  - `editor_scroll_content_size`
  - pointer-wheel, pointer-drag, and selection-edge autoscroll offset resolution
  - `EditorScrollAreaDebugState`
- `src/app/ui/editor_content/mod.rs`
  - `WindowRenderMode`
  - `preferred_window_render_mode`
  - `should_prefer_visible_window`
  - `should_prefer_focused_window`
  - fallback routing between full render, focused visible window, and read-only visible window
- `src/app/ui/editor_content/native_editor/mod.rs`
  - `render_editor_visible_text_window`
  - `render_editor_focused_text_window`
  - `VisibleWindow*` structs
  - visible-window galley reuse
  - visible-window padding and layout publication
  - cursor reveal and viewport-line helper functions that only exist to keep the old visible-window path alive
  - `VisibleWindowDebugSnapshot`
- `src/app/domain/view.rs`
  - `scroll_to_cursor`
  - `CursorRevealMode`
  - current `editor_scroll_offset` API, unless replaced in-place by the new scroll manager facade
  - current cursor-reveal request/clear helpers
- `src/app/domain/buffer.rs` and `src/app/domain/buffer/state.rs`
  - `RenderedTextWindow`
  - `VisibleWindowLayoutKey`
  - visible-window cache checks
  - `visible_line_window`
  - `visible_text_window`
  - buffer-owned `editor_scroll_offset`, if it is still unused by the final model
- `src/app/ui/editor_content/gutter.rs`
  - visible-window row-offset assumptions
- `src/app/ui/editor_area/mod.rs`
  - visible-window debug/test helpers
- `src/app/ui/tile_header/split/preview.rs`
  - preview logic coupled to `RenderedTextWindow`; either replace with viewport-snapshot preview data or remove the coupling
- `src/app/ui/autoscroll.rs`
  - keep only if the new local scroll module deliberately owns edge autoscroll as a reusable primitive
- `src/app/ui/tab_drag/state/autoscroll.rs`
  - adapt if `src/app/ui/autoscroll.rs` moves or changes API
- `src/profile.rs`, `src/bin/profile_viewport_extraction.rs`, `src/bin/profile_scroll_stress.rs`, and related benchmark targets
  - remove or rewrite after the new viewport model exists

## Current Tests To Discard Or Rewrite

Delete current tests that encode old scrolling or visible-window behavior. Do not preserve them by mechanically updating assertions.

Known local unit and integration surfaces include:

- `src/app/domain/view.rs`
  - `editor_scroll_offset_is_view_owned_runtime_state`
  - cursor-reveal mode tests
- `src/app/ui/editor_content/mod.rs`
  - visible/focused window selection tests
- `src/app/ui/editor_content/native_editor/mod.rs`
  - scroll-to-cursor consumption tests
  - viewport-line range tests
  - cursor-visible range tests
  - visible-window selection/layout tests
  - page navigation scroll-offset tests
- `src/app/ui/editor_area/tile.rs`
  - scroll source, clamp, selection edge-drag, and split-view scroll offset tests
- `src/app/ui/editor_area/mod.rs`
  - visible-window click and wheel-scroll mapping tests
- `src/app/domain/buffer.rs` and `src/app/domain/buffer/state.rs`
  - visible-window layout and extraction tests
- `src/app/ui/editor_content/gutter.rs`
  - visible-window gutter-offset tests
- `src/app/ui/tile_header/split/preview.rs`
  - visible-window preview tests
- `src/app/app_state/search_state/tests.rs`
  - tests that assert `scroll_to_cursor`

Replacement tests should be created only after the new scroll/view contract is defined.

## Target Architecture

### 1. Introduce A Local Scroll Module

Create a local editor scroll subsystem under `src/app/ui/scrolling/`, with clear ownership boundaries:

- `ScrollState`
  - current offset
  - target offset for animated/programmatic scroll
  - velocity or momentum only if needed
  - scrollbar interaction state
  - sticky-to-end state
  - content size and viewport size from the last frame
- `ScrollSource`
  - scrollbar
  - wheel
  - drag
  - programmatic target
  - autoscroll
- `ScrollOutput`
  - final offset
  - viewport rect
  - content size
  - interaction state
  - whether a scroll occurred
- `ScrollTarget`
  - target rect/range
  - alignment
  - animation policy
- `ScrollController` or `ScrollManager`
  - owns input arbitration, clamping, target resolution, reveal requests, and scrollbar visibility

Copy the relevant egui mechanics locally instead of depending on `egui::ScrollArea` behavior:

- persistent state by stable id
- explicit scroll sources
- `show_viewport`-style callback
- `ScrollAreaOutput`-style output
- content-size based clamping
- scrollbar thumb sizing and drag mapping
- wheel delta consumption
- `scroll_to_rect`/target-alignment behavior
- optional `stick_to_bottom` semantics
- visible-when-needed/always/hidden scrollbar policy

Keep this module editor-specific at first. Generalize only if another UI surface needs the same behavior later.

### 2. Adopt Zed-Style Scroll Ownership

Replace ad hoc scroll fields with a Zed-inspired model:

- Each editor view owns a `ScrollManager`-like state object.
- Scroll position is represented in display coordinates, not raw logical lines.
- The top of the viewport is stable across edits through an anchor-like concept.
- The scroll manager stores visible row and column counts after layout.
- Programmatic movement does not directly mutate raw offset everywhere; it requests a scroll action through one API.
- Cursor reveal is an autoscroll request, not a boolean flag.
- Wheel scrolling, scrollbar dragging, page navigation, selection autoscroll, and cursor reveal are different scroll intents resolved by one manager.

The local project does not need to clone Zed's GPUI/entity structure, but it should copy the separation:

- document/buffer state stores text
- display/layout state maps buffer positions to display rows
- view state stores scroll and cursor state
- render code consumes a snapshot and emits scroll/layout updates
- input code emits intents

### 3. Replace Visible Windows With Viewport-First Rendering

Do not rebuild the old focused/read-only visible-window fork.

Instead, make every editor render through one viewport-first path:

- Build or reuse a display snapshot for the current view.
- Compute visible display rows from scroll position, viewport height, row height, wrap width, folds, and overscan.
- Extract only the needed text/layout rows for paint.
- Maintain total content extent separately from visible content.
- Use the same rendering path for focused and unfocused views.
- Use the same path for small and large files.
- Support wrapping by making display rows the scroll unit.
- Keep gutter, cursor, selection, search highlights, and IME rectangles derived from the same display-row snapshot.

The previous split between full render, read-only visible window, and focused visible window is the core source of complexity. The replacement should make "visible window" an internal viewport slice of the normal renderer, not a separate rendering mode.

### 4. Define New Core Types

Add explicit types before wiring UI:

- `DisplayRow`
  - visual row after wrap/fold transformations
- `DisplayPoint`
  - visual row and column used for cursor geometry and hit testing
- `ScrollAnchor`
  - piece-tree anchor plus fractional display-row offset (`f32`) within that anchor's wrapped row
  - horizontal offset stored in pixels alongside the anchor
- `ViewportMetrics`
  - viewport rect, row height, column width, visible row count, visible column count
- `ContentExtent`
  - total display rows, pixel height, max line width, horizontal max
- `ViewportSlice`
  - overscanned row range and source text spans needed for paint
- `RevealTarget`
  - cursor/selection/search target rect plus alignment policy
- `ScrollIntent`
  - wheel, scrollbar drag, page, line, reveal, center, top, bottom, selection edge, restore

Keep conversion between logical buffer offsets and display coordinates in one layer. Avoid scattering `line_count * row_height` and raw rect arithmetic through render code.

### 5. Rebuild Cursor Reveal

Replace `scroll_to_cursor: bool` and `CursorRevealMode` with queued scroll intents:

- ordinary typing and arrow movement: reveal cursor with margin
- search/jump/go-to-line: center cursor or nearest comfortable band
- mouse click: move cursor without unnecessary scroll unless the result is outside the viewport
- wheel/scrollbar/manual scroll: mark user intent and suppress automatic snap-back
- page navigation: move viewport and cursor together from visible row counts
- selection drag near viewport edge: request bounded autoscroll while extending selection

The reveal decision should use rendered cursor geometry from the current display snapshot. If the target is outside the current viewport slice, resolve it through display-map coordinates rather than relying on a previously painted galley.

### 6. Rebuild Scroll Extent

Make scroll bounds derive from `ContentExtent`, never from logical file line count alone.

The extent calculation must include:

- wrap width
- font size and line height
- gutter width
- tile width and height
- soft wraps
- long lines
- horizontal scrolling
- final line and EOF behavior
- scroll-beyond-last-line: one viewport-height of overscroll past EOF
- split panes with different widths

This directly addresses the failure mode described in `docs/scroll-bottom-investigation.md`: wrapped visual rows can exceed logical lines, so a logical-line height estimate can clamp too early.

### 7. Rebuild Dependencies After The New Model Lands

Ignore the current dependency graph while designing the new scroll/viewport path. After the new implementation is in place:

- remove obsolete imports and public exports
- remove old debug-only structs
- remove visible-window cache structs
- remove old scroll helpers
- remove stale profiles and benchmark targets
- rebuild module boundaries around the new scroll manager and viewport renderer
- update docs that still describe the old visible-window path

This cleanup should happen after behavior is working so the rewrite is not constrained by temporary compilation dependencies.

## Implementation Phases

### Phase 0: Freeze And Delete Scope

1. Create a removal checklist from the "Current Local Surface To Remove" section.
2. Mark old scroll/visible-window tests as expected deletion, not regression coverage.
3. Stop adding fixes to the existing visible-window path.
4. Keep unrelated tab-strip scrolling and settings/dialog `ScrollArea` usage out of scope unless the new local scroll module later replaces them deliberately.

### Phase 1: Localize egui ScrollArea Mechanics

1. Copy the needed egui scroll-area concepts into a local module.
2. Preserve upstream attribution and license notes in the copied module.
3. Strip general-purpose features that the editor does not need.
4. Keep the concepts that matter:
   - state load/store by id
   - content and viewport rects
   - content-size clamping
   - scrollbar visibility
   - wheel and scrollbar drag mapping
   - scroll targets and alignment
   - sticky-to-end handling if needed
5. Add low-level tests for scroll math only after the local module API settles.

### Phase 2: Introduce Zed-Style ScrollManager

1. Add per-view scroll manager state.
2. Add scroll intents and a single resolution path.
3. Store visible row/column counts from layout.
4. Replace direct offset writes with scroll manager calls.
5. Implement anchor-style top-of-viewport stability.
6. Add conversion helpers between pixel offset and display rows.

### Phase 3: Build The Display-Row Viewport Pipeline

1. Add display-row snapshot data for wrapped and unwrapped text.
2. Compute total content extent from display rows.
3. Compute overscanned visible row ranges from scroll position.
4. Extract viewport slices from `PieceTreeLite`/buffer state.
5. Render from the viewport slice for all files.
6. Paint gutter, text, selections, cursor, search highlights, and IME geometry from the same slice.
7. Remove old full-vs-visible-vs-focused render routing.

### Phase 4: Rewire Input

1. Wheel input emits scroll intent.
2. Scrollbar drag emits scroll intent.
3. Pointer drag on content emits selection intent and optional edge-autoscroll intent.
4. Arrow keys emit cursor movement plus reveal intent.
5. PageUp/PageDown use visible row count and update both scroll and cursor.
6. Search/location jumps emit centered reveal intent.
7. Mouse click updates cursor and reveals only if needed.

### Phase 5: Remove Old Code And Tests

1. Delete old visible-window render functions and structs.
2. Delete old cursor-follow flags and tests.
3. Delete old scroll-area wrapper code in `tile.rs`.
4. Delete old visible-window buffer/cache helpers.
5. Delete old visible-window debug hooks.
6. Delete stale profiles and benchmarks, or replace them with new viewport profiles.
7. Rebuild module exports and imports once the new path compiles.

### Phase 6: Fresh Test Suite

Write new tests around the replacement contract:

- scroll math clamps correctly on both axes
- scroll target alignment keeps a rect visible
- cursor reveal applies margins
- explicit jumps center the target
- wheel scrolling does not snap back to cursor
- page navigation uses visible row count
- long wrapped line produces multiple display rows
- split panes with different widths have different extents
- resizing recomputes display rows and preserves anchor
- selection edge autoscroll extends selection
- scrollbar drag maps thumb position to content offset
- search result activation reveals the match
- gutter row numbers align with display rows and logical rows
- IME cursor rect follows the painted cursor
- EOF is reachable in wrapped and unwrapped files
- one viewport-height of overscroll past EOF is available and clamps correctly

Prefer headless unit tests for pure scroll/layout math and targeted egui harness tests only for integration behavior that depends on UI frames.

### Phase 7: New Profiles And Instrumentation

Replace old profiles with:

- viewport slice extraction latency
- display-row rebuild latency after resize/wrap change
- scroll frame latency on large files
- cursor reveal latency after search jump
- split-pane resize + scroll extent recalculation
- memory use for display snapshots on large files

Add temporary diagnostics while validating:

- view id
- scroll anchor
- resolved scroll position
- viewport size
- visible row range
- overscan range
- content extent
- cursor display point
- reveal intent
- final clamped offset

Remove temporary diagnostics before merging.

## Design Rules

- One editor rendering path.
- One scroll manager per editor view.
- One source of truth for content extent.
- One conversion layer between buffer offsets and display coordinates.
- No direct `line_count * row_height` scroll extent estimates in UI code.
- No old visible-window mode switch.
- No cursor-follow boolean.
- No direct mutation of scroll offset from scattered input handlers.
- No tests that assert old helper names or old internal flags.

## Locked Decisions

These were previously open. They are now fixed for the v1 implementation; revisit only with a written reason.

- **Module location**: the new scroll module lives under `src/app/ui/scrolling/`. Scroll is a view-state concern (viewport size, scrollbar interaction, pixel offset); domain owns cursors and selections, not pixels.
- **`ScrollAnchor` representation**: piece-tree anchor + fractional display-row offset within that anchor's wrapped row. Survives edits above the viewport and resizes/wrap changes without visible jumps. Matches Zed's approach.
  - **Current substrate**: piece-tree point anchors are implemented with leaf-local anchor buckets plus direct `AnchorId` metadata. `ScrollAnchor::Logical` remains only as a bootstrapping/test fallback until the renderer can seed a piece-backed anchor.
- **Vertical scroll position**: fractional display rows stored as a single `f32`. Pixels are derived at paint time. Do not store both.
- **Horizontal scroll position**: pixels. Columns are a fiction once ligatures, tabs, mixed fonts, or non-ASCII appear.
- **Programmatic scroll animation**: immediate in v1. `ScrollIntent` reserves space for an `animated` variant but no easing machinery is built yet.
- **`RenderedLayout`**: replaced entirely by `DisplaySnapshot`. No transitional wrapper — a wrapper would preserve the coupling the rewrite is meant to remove.
- **Scroll-beyond-last-line**: enabled by default with one viewport-height of overscroll past EOF. Guarantees EOF is reachable under wrap regardless of extent rounding and matches modern editor norms. Built in from the start; not retrofitted.
- **`DisplaySnapshot` ownership**: per-view cache on `EditorViewState`, alongside the existing `ScrollManager`. Display rows are a function of wrap width, font, and viewport — three of those four inputs are view state, so the snapshot is view state. Split panes get independent snapshots automatically, which is required by the "different widths → different extents" acceptance criterion. Rejected alternatives: returning the snapshot through `EditorWidgetOutcome` fails because gutter, tile-header preview, and input/click-mapping consume the snapshot before or beside the editor body, not downstream of it; storing it on `BufferState` keyed by view re-creates the same view-state-on-buffer coupling that `BufferState::editor_scroll_offset` had (and that the 2026-04-29 cleanup removed), and forces lifecycle registration on view open/close/clone. Invalidation triggers: wrap width change, font change, viewport resize (all view-local), plus a buffer revision counter checked once per frame for edits. Duplicate snapshots when the same buffer is shown in two panes at the same wrap width are accepted as a v1 cost; a buffer-side memoization layer can be added later if it shows up in profiles.

## Progress Log

Each phase appends a status line here as it lands. Format: `- [phase] status — brief note (date)`.

<!-- progress entries below -->
- [Phase 3 — RenderedLayout deletion + cursor reveal as intent] **landed** (2026-04-29). The locked-decision big-bang: `RenderedLayout` is gone, `DisplaySnapshot` is the per-view source of truth for wrap-aware row data, and cursor reveal now flows through `ScrollIntent::Reveal` instead of the pixel-offset back-channel.
  - **`RenderedLayout` deleted entirely** from `buffer.rs`. `domain::mod` re-export removed. `EditorViewState::latest_layout`/`latest_layout_revision` deleted; replaced by `latest_display_snapshot` (already present) plus a new `latest_display_snapshot_revision: Option<u64>` for the take/restore dance during paint.
  - **Gutter migrated** to read `DisplaySnapshot` directly: `render_line_number_gutter` now takes `Option<&DisplaySnapshot>`, walks `row_count()` / `row_top(DisplayRow)` / `logical_line_for(DisplayRow)`, and emits a line number only when `logical_line` differs from the prior row (matching the previous "leading row" semantics without an explicit Option<usize> column on the snapshot).
  - **`EditorContentStyle::previous_layout` → `previous_snapshot`**, `tile.rs::take_previous_layout` / `restore_previous_layout_if_needed` → `take_previous_snapshot` / `restore_previous_snapshot_if_needed`. `WorkspaceTab::clear_transient_view_state` updated.
  - **Cursor reveal as intent producer**: `paint_cursor_effects` now pushes `ScrollIntent::Reveal { rect, align_y, align_x }` directly into `view.pending_intents`. `KeepVisible` → `ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX)` on Y, `Center` → `ScrollAlign::Center`, both with `NearestWithMargin(0.0)` on X. The cursor rect is computed in galley-local (content) coordinates so it feeds the manager's `reveal()` math directly without screen↔content translation. `requested_scroll_offset_for_cursor`, `content_viewport`, `scroll_offset_to_keep_rect_visible`, `scroll_offset_to_center_rect_vertically`, `scroll_offset_to_keep_axis_visible`, and their tests are deleted.
  - **`EditorWidgetOutcome::requested_scroll_offset` and `EditorContentOutcome::requested_scroll_offset` deleted**. The cursor-reveal back-channel is gone; only wheel and pointer drag still flow through pixel-offset overrides. `tile.rs::resolve_editor_scroll_offset` → `resolve_editor_scroll_offset_override` returning `Option<egui::Vec2>`; `set_editor_pixel_offset` is now only called when an actual override exists, so the manager's intent-resolved anchor is preserved when there's no out-of-band source.
  - **Second drain in `show_editor_scroll_area`**: after `publish_scroll_manager_metrics`, `drain_pending_scroll_intents` runs a second time to consume reveal intents emitted during paint. The first drain (top of fn) handles intents queued before this frame; the second handles paint-time emissions. Both go through the single `apply_intent` path.
  - **`update_visible_layout` deleted** along with `set_latest_layout` / `clear_latest_layout` helpers. Snapshot now written inline at the end of `render_editor_text_edit`. Read-only `render_read_only_text_edit` writes a snapshot too (no revision, since read-only paths don't track revisions).
  - **`profile.rs::run_scroll_stress_profile`** and **`benches/tab_stress.rs::scroll_layout_pass`**: dropped `RenderedLayout::from_galley(...).visual_row_count()` in favor of `galley.rows.len().max(1)`.
  - **Tests**: 236 lib tests pass, 0 ignored, 0 failed. `cargo check --all-targets` clean. Same one pre-existing `tests/file_service_tests.rs::preserves_encoding_when_round_tripping_windows_1252` encoding failure on master (unrelated).
  - **Acceptance criteria progress**: "no cursor-follow boolean" — was already done; "search jumps, arrow keys, page keys, mouse wheel, scrollbar drag, and selection edge autoscroll all pass through the new scroll manager" — cursor reveal (the typing/arrow case) now flows through `ScrollIntent::Reveal`. **Search jump** is the remaining producer still on the back-channel.
  - **Still deferred**:
    - Search-jump from `app_state/search_state` still calls `set_editor_pixel_offset` directly. Migrating it to push `ScrollIntent::Reveal { align_y: Center }` is the next obvious follow-up; same pattern as the cursor reveal change here.
    - Broader viewport-stability coverage for piece-tree-backed anchors.
    - Phase 6 fresh-test-suite checklist (wrapping, split-pane extents, scroll-beyond-EOF, IME geometry, gutter alignment under wrap) is still partial; the cursor-reveal intent path now has unit-testable surface (`ScrollAlign::resolve` + `apply_intent`) but no dedicated coverage yet.
    - Phase 7 profile/benchmark rewrite remains.
- [Phase 3 renderer migration — big-bang cut] **landed** (2026-04-29). Per the locked decision, `DisplaySnapshot` is the per-view source of truth on `EditorViewState`; the legacy `RenderedTextWindow`/`VisibleWindowLayoutKey` surface is deleted, not deprecated.
  - **Deleted from `buffer.rs`/`buffer/state.rs`**: `RenderedTextWindow`, `VisibleWindowLayoutKey`, `RenderedLayout::visible_text`, `RenderedLayout::visible_window_layout_key`, `set_visible_text`, `set_visible_text_with_cache_key`, `matches_visible_window_layout`, `visible_window_matches`, `visible_row_range`, `visible_line_range`, `offset_line_numbers`, `BufferState::visible_text_window`, `BufferState::visible_line_window`, `build_rendered_text_window`, `line_range_for_char_window`, and their tests. `domain::mod` re-exports updated.
  - **`BufferState::view_status` simplified** to take only `Option<CursorRange>`; the `visible_window` parameter and `visible_line_start`/`visible_line_end` fields on `BufferViewStatus` are gone. Status bar's "View N–M" segment (`viewport_label`) deleted along with its test — to be reintroduced later from `DisplaySnapshot` if needed.
  - **`update_visible_layout`** in `native_editor/mod.rs` now only builds `RenderedLayout` from the galley and stores it on the view; it no longer asks the buffer for a `RenderedTextWindow`. `visible_row_range_for_galley` and `VISIBLE_ROW_OVERSCAN` deleted. The `DisplaySnapshot` build (already added in the prior pass) remains the single source for wrap-aware row data on the view.
  - **Consumers migrated**:
    - `gutter.rs`: walks the full `RenderedLayout::row_count()` instead of `visible_row_range()`; the `layout_row_offset` y-shift is gone (the unified renderer paints the full galley, so offset is always 0). Old `visible_layout_y_offset` and its test deleted.
    - `tile_header/mod.rs::preview_lines_for_view`: dropped the windowed branch; always uses `build_preview_lines(&buffer.text())`.
    - `tile_header/split/preview.rs`: `build_preview_lines_for_window` and its tests deleted; `split.rs` re-export trimmed.
    - `editor_content/artifact.rs`: dropped the windowed read-only render path with top/bottom padding; renders the full transformed text. The `previous_layout` parameter is retained (still threaded through `editor_content`) but ignored.
    - `status_bar.rs`: `view_status` call updated; `viewport_label` field, function, and test removed.
  - **`profile.rs::run_viewport_extraction_profile`** rewritten to exercise piece-tree `extract_range` directly instead of the deleted `visible_line_window`/`visible_text_window` helpers. Sub-bin profile binaries unchanged (didn't reference the deleted surface).
  - **Tests**: 240 lib tests pass, 0 ignored, 0 failed. `cargo check --all-targets` clean. The one pre-existing failure (`preserves_encoding_when_round_tripping_windows_1252` in `tests/file_service_tests.rs`) is unrelated to scrolling and was failing on master prior to this change.
  - **Still deferred** (next steps after this milestone):
    - `DisplaySnapshot` is now built and stored per-view, but consumers still read primarily from `RenderedLayout` (gutter, status bar, click mapping). Migrating those readers off `RenderedLayout` entirely and onto `DisplaySnapshot` is the next incremental step; this pass deleted the *windowed* surface and unblocked that migration.
    - Search-jump and typing/arrow cursor reveal still flow through the pixel-offset back-channel (`requested_scroll_offset_for_cursor`); rewriting them as `ScrollIntent::Reveal` producers with alignment policy is the next Phase 4 item.
    - Broader viewport-stability coverage for piece-tree-backed anchors.
    - Phase 6 fresh test suite for the acceptance-criteria checklist still partial.
    - Phase 7 profiles still partial: `run_viewport_extraction_profile` no longer references deleted surface but exercises piece-tree extraction rather than the new viewport pipeline; `profile_viewport_extraction.rs`/`profile_scroll_stress.rs` binaries unchanged.
- [Phase 0] complete — inventory verified against tree; all referenced symbols exist (2026-04-27)
- [Phase 1] complete — `src/app/ui/scrolling/` module landed: `ScrollState`, `ScrollSource`, `ScrollAlign`, `ScrollTarget`, `ScrollbarPolicy`, `ScrollArea`/`ScrollAreaOutput`. Pixel-based primitive with one-viewport EOF overscroll, scrollbar drag/wheel, programmatic targets, persistent state. Unit tests for clamp math pass. Not yet wired into the editor. (2026-04-27)
- [Phase 2] complete — `ScrollManager`, `ScrollAnchor`, `ScrollIntent`, `ViewportMetrics`, `ContentExtent` landed. Single mutation entry point (`apply_intent`) for wheel/scrollbar/lines/pages/top/bottom/reveal/restore/edge-autoscroll. Vertical position stored as fractional display rows; horizontal as pixels. Display-map conversion plumbed via callback functions (`anchor_to_row`/`row_to_anchor`). `ScrollAnchor::Logical` now remains as a bootstrapping/test fallback; live editor paths can use piece-tree anchors once a display snapshot is available. Unit tests pass. (2026-04-27; anchor status updated 2026-04-29)
- [Phase 3] complete — `DisplaySnapshot`, `DisplayRow`, `DisplayPoint`, `ViewportSlice` landed in `src/app/ui/scrolling/display.rs`. Wraps an `egui::Galley` into a wrap-aware row-indexed snapshot with row tops, char ranges, logical-line map, max line width. `viewport_slice(top_row, viewport_h, overscan)` returns the row range to paint. Slice math unit-tested. Not yet wired into the editor renderer. (2026-04-27)
- [Phase 4+5b] complete — `WindowRenderMode` enum, `preferred_window_render_mode`, `should_prefer_visible_window`, `should_prefer_focused_window` and their tests removed from `editor_content/mod.rs`. `render_editor_body` now always calls `render_editor_text_edit`. (2026-04-27)
- [Phase 4+5c] complete — `tile.rs` no longer wraps `egui::ScrollArea::both()`; the local `scrolling::ScrollArea` is the editor's scroll container. Helpers added: `local_scroll_source`, `scrollbar_policy_from_egui`. Removed `editor_scroll_source` and its test. (2026-04-27)
- [Phase 4+5d] partial — `render_editor_visible_text_window` and `render_editor_focused_text_window` deleted from `native_editor/mod.rs`. ~49 supporting helpers (`visible_window_*`, `viewport_line_span`, `cursor_visible_line_range`, etc.) are now unreachable but still physically present in `native_editor/mod.rs`; deleting them is a mechanical cleanup that can be driven by chasing `cargo build`'s dead-code warnings. `RenderedTextWindow`, `VisibleWindowLayoutKey`, `visible_line_window`, `visible_text_window` in `buffer.rs` and downstream callers (`gutter.rs`, `tile_header/split/preview.rs`) also still present. Five old visible-window tests in `editor_area::mod` are `#[ignore]`'d. (2026-04-27)
- [Phase 4+5/6] **dead-code sweep + queue drainage + replacement tests** (2026-04-29). Built on the 2026-04-27 partial cut.
  - Done in this pass:
    - `pending_intents` queue now drained: `tile.rs::drain_pending_scroll_intents` runs at the start of every `show_editor_scroll_area` call and routes each queued `ScrollIntent` through `ScrollManager::apply_intent` (using `naive_anchor_to_row`/`naive_row_to_anchor`).
    - `tile.rs::publish_scroll_manager_metrics` now publishes `ViewportMetrics` and `ContentExtent` to the per-view `ScrollManager` after each frame, so future `Pages`/`Reveal` intents resolve against real geometry instead of zeros.
    - All 49+ unreachable visible-window helpers/structs deleted from `native_editor/mod.rs`: `VisibleWindowInputState`, `VisibleWindowRenderRequest`, `VisibleWindowLayoutState`, `VisibleWindowFrame`, `VisibleWindowInputOutcome` + impl, `VisibleWindowDebugSnapshot`, `visible_window_debug_id`, `store_visible_window_debug_snapshot`, `load_visible_window_debug_snapshot`. Stale empty `#[allow(...)]` attributes and section banners removed.
    - Dead helpers in sibling files removed: `apply_cursor_movement_unwrapped`/`move_vertically`/`current_line` from `cursor.rs`; `windowed_search_highlights`/`windowed_char_range`/`normalize_char_window` from `highlighting.rs`; `cursor_range_after_click`/`handle_keyboard_events_unwrapped`/`handle_mouse_interaction_window` from `interactions.rs` and `interactions/keyboard.rs`. Their tests deleted with them.
    - Unreachable test surface deleted from `native_editor/mod.rs::tests` (~10 tests referencing deleted helpers `cursor_visible_line_range`/`focused_visible_line_range`/`viewport_visible_line_range`/`visible_line_range_for_window`/`unpainted_cursor_reveal_outcome`/`cursor_reveal_visible_line_range`/`visible_window_selection`/`cursor_window_selection_mode` and the `visible_layout_for_test` helper).
    - `editor_area/mod.rs`: 5 `#[ignore]`'d visible-window tests deleted (`visible_window_release_snapshot_tracks_widget_rect_and_pointer_path`, `scrolled_visible_window_click_places_cursor_in_scrolled_document_region`, `scrolled_wide_visible_window_click_places_cursor_in_scrolled_document_region`, `focused_wheel_scroll_updates_visible_window_and_click_mapping`, `focused_arrow_down_reveals_cursor_after_wheel_scroll`) along with their dead test helpers (`active_visible_window_debug`, `visible_window_click_point`, `active_scroll_area_state`, `line_index_for_active_cursor`, `active_view_visible_lines`, `run_editor_frame_with_rect`, `click_pointer_with_rect`, `settle_frame_with_rect`, `mouse_wheel_event`, `key_event`).
    - `tile.rs`: `EditorScrollAreaDebugState`, `editor_scroll_debug_id`, `store_editor_scroll_debug_state`, `load_editor_scroll_debug_state` deleted (only consumed by the deleted tests).
    - `EditorContentStyle::is_active` deleted (no consumer remained).
    - Stale comments removed: the orphan `WindowRenderMode` test-block comment in `editor_content/mod.rs`, and the transitional "Single viewport-first render path" banner.
    - **Phase 6 starter coverage** added in `editor_area::tests`:
      - `pixel_offset_round_trips_through_scroll_manager` — pixel set/get goes through the manager and round-trips.
      - `queued_intents_drain_through_scroll_manager` — queued `Pages` + `Lines` intents apply in order against real metrics.
      - `clear_cursor_reveal_settles_without_panicking_with_scroll_manager` — end-to-end smoke test that the manager-backed render lifecycle survives.
    - Tree compiles cleanly (`cargo check --all-targets`); 237 lib tests pass, 0 ignored, 0 failed.
  - Still deferred (legitimate Phase-3-renderer-migration / Phase-7 scope):
    - `RenderedTextWindow` / `VisibleWindowLayoutKey` / `visible_line_window` / `visible_text_window` still live in `buffer.rs`/`buffer/state.rs` because the live renderer (`update_visible_layout` in `native_editor/mod.rs`) still consumes them, and so do `gutter.rs`, `tile_header/split/preview.rs`, `editor_content/artifact.rs`, and `profile.rs`. Removing them is gated on the `DisplaySnapshot`-backed renderer migration — Phase 3's types exist but they aren't yet plumbed into the live render path.
    - `view.rs::editor_pixel_offset()` remains buffer-less fallback behavior. Render paths with buffer access should use `editor_pixel_offset_resolved()` so piece-backed anchors resolve through `DisplaySnapshot`.
    - `autoscroll.rs` / `tab_drag/state/autoscroll.rs` not yet rewired through `ScrollIntent::EdgeAutoscroll`.
    - `profile.rs`, `bin/profile_viewport_extraction.rs`, `bin/profile_scroll_stress.rs` still reference the old visible-window surface; pending Phase 7 rewrite.
    - The `pending_intents` queue is now drainable end-to-end but no in-tree producer pushes through `request_intent` yet — current scroll producers still flow through the pixel-offset back-channel (`set_editor_pixel_offset` → `apply_intent(ScrollbarTo)`). Adding intent-first producers (search-jump → `Reveal`, page nav → `Pages`, etc.) is the next incremental Phase 4 step.

- [Phase 4 producers + DisplaySnapshot anchor resolution + autoscroll rewire + buffer cleanup] **closed** (2026-04-29). Built on top of the 2026-04-29 dead-code sweep.
  - Done in this pass:
    - **EdgeAutoscroll wired through `ScrollIntent`**: `ScrollManager` now stores X- and Y-axis edge-autoscroll velocities (`edge_autoscroll_x`/`edge_autoscroll_y`); `apply_intent` accepts `ScrollIntent::EdgeAutoscroll { axis, velocity }`; `tick_edge_autoscroll(dt, ...)` and `clear_edge_autoscroll()` step both axes through `apply_intent(Lines/Wheel)` so all velocity-driven scroll motion goes through the single mutation path.
    - **`tile.rs::apply_selection_edge_autoscroll_intent`**: replaces the old `requested_scroll_offset_for_selection_edge_drag`/`scroll_offset_from_selection_edge_drag` pixel back-channel. It builds a snapshot-aware closure, emits `EdgeAutoscroll{Axis::X, vx}` + `EdgeAutoscroll{Axis::Y, vy}` for the active drag, ticks with `dt = 1.0` per frame, and clears the velocity when the drag releases. Dead helper `scroll_offset_from_selection_edge_drag` and its test deleted.
    - **DisplaySnapshot-aware piece-anchor resolution**: new free function `scrolling::display_aware_anchor_to_row(snapshot, resolve_piece)` in `manager.rs` (re-exported from `scrolling::mod`). New `EditorViewState::editor_pixel_offset_resolved(&self, &BufferState)` uses it, so when the manager holds a `ScrollAnchor::Piece(id)` the buffer's `piece_tree().anchor_position(id)` is consulted to map back to a real document offset → display row. Naive identity fallback retained for `Logical` anchors. The original `editor_pixel_offset()` (returns ~0 for piece anchors) is still used by buffer-less callers but is no longer the live render-path source of truth.
    - **`drain_pending_scroll_intents` upgraded** to take `&BufferState` and build the same snapshot-aware closure for `Reveal`/`RestoreAnchor` anchor↔row conversions.
    - **Anchor lifecycle fix in `upgrade_scroll_anchor_to_piece`**: `EditorViewState` tracks `last_piece_anchor: Option<AnchorId>` and releases the previous anchor before allocating a fresh one each frame. Closing or clearing transient view state also releases view-owned anchors.
    - **Buffer-side legacy scroll offset deleted**: `EditorScrollOffset` struct, `BufferState::editor_scroll_offset` field/methods/`sanitize_scroll_axis`, and the `editor_scroll_offset_is_buffer_owned_runtime_state` test all removed from `buffer/state.rs`. Scroll position is now exclusively per-view.
    - **First in-tree intent producer wired**: `render_editor_text_edit` now calls `consumed_page_navigation_direction(ui)` for unconsumed PageUp/PageDown presses and emits `view.request_intent(ScrollIntent::Pages(direction))` so page navigation flows through the queue → `ScrollManager::apply_intent(Pages)` path. Old `requested_scroll_offset_for_page_navigation`/`page_navigation_requested_scroll_offset`/`page_navigation_scroll_delta`/`page_navigation_delta_size` helpers and the back-channel through `EditorWidgetOutcome.requested_scroll_offset` for page nav deleted. Replacement test `page_navigation_emits_pages_intent_with_signed_direction` covers both directions.
    - **Tests**: 252 lib tests pass (was 249 before this pass + 3 net additions across `manager`/`tile`/`native_editor` covering the new flows, with the page-nav test rewritten to assert the intent direction rather than the pixel offset). `cargo check --all-targets` clean.
  - Still deferred (true Phase-3-renderer / Phase-7 scope, unchanged from prior log entry):
    - `RenderedTextWindow` / `VisibleWindowLayoutKey` / `visible_line_window` / `visible_text_window` and their consumers (`update_visible_layout`, `gutter.rs`, `tile_header/split/preview.rs`, `editor_content/artifact.rs`, `profile.rs`) — gated on `DisplaySnapshot`-backed renderer migration.
    - Search-jump and cursor-reveal still flow through the pixel-offset back-channel (`requested_scroll_offset_for_cursor` → `set_editor_pixel_offset` → `apply_intent(ScrollbarTo)`). The math is equivalent and goes through the manager, but rewriting them as `ScrollIntent::Reveal` producers is a larger change with significant existing test surface; left for a focused follow-up.
    - `bin/profile_viewport_extraction.rs`, `bin/profile_scroll_stress.rs`, and `profile.rs` still reference the legacy visible-window surface; pending Phase 7 rewrite.

- [Phase 4+5] **partial — clean break landed for view state surface, full input rewire and old-code deletion deferred** (2026-04-27).
  - Done:
    - `EditorViewState` now owns a `ScrollManager` and a `pending_intents: Vec<ScrollIntent>` queue.
    - `scroll_to_cursor: bool` and inline `cursor_reveal_mode` field deleted; replaced by `pending_cursor_reveal: Option<CursorRevealMode>` resolved at render time into a `ScrollIntent::Reveal`.
    - `editor_scroll_offset()`/`set_editor_scroll_offset()` deleted; replaced by `editor_pixel_offset()`/`set_editor_pixel_offset()` which delegate to the scroll manager (intents on the X+Y scrollbar axes).
    - Three old visible-window tests in `editor_area::mod` marked `#[ignore]` with replacement scheduled for Phase 6: `scrolled_visible_window_click_places_cursor_in_scrolled_document_region`, `scrolled_wide_visible_window_click_places_cursor_in_scrolled_document_region`, `focused_wheel_scroll_updates_visible_window_and_click_mapping`.
    - Tile-level test `duplicated_views_can_track_independent_scroll_offsets` deleted (asserted old API).
    - Tree compiles; 257 tests pass, 3 ignored.
  - Deferred to next session(s):
    - `tile.rs` still wraps content in `egui::ScrollArea::both()`; replace with the local `scrolling::ScrollArea`.
    - `editor_content::mod` still routes between `WindowRenderMode::Full`/`VisibleWindow`/`Focused`; collapse to one viewport-first render path using `DisplaySnapshot`/`ViewportSlice`.
    - `native_editor::mod` still contains `render_editor_visible_text_window`, `render_editor_focused_text_window`, `VisibleWindow*` structs, `VisibleWindowDebugSnapshot`, viewport-line helpers — delete after the unified renderer lands.
    - `buffer.rs` still owns `RenderedTextWindow`, `VisibleWindowLayoutKey`, `visible_line_window`, `visible_text_window`, buffer-side `editor_scroll_offset` — delete after callers move off them.
    - `gutter.rs`, `tile_header/split/preview.rs` still depend on `RenderedTextWindow`.
    - `autoscroll.rs`, `tab_drag/state/autoscroll.rs` not yet rewired through `ScrollIntent::EdgeAutoscroll`.
    - `profile.rs`, `bin/profile_viewport_extraction.rs`, `bin/profile_scroll_stress.rs` still reference old surface.
    - `pending_intents` queue is plumbed but never drained; renderer side of Phase 4 (drain queue → `ScrollManager::apply_intent`) is not yet implemented.
    - Buffer-less `view.rs` `editor_pixel_offset()` is fallback-only; live render paths should prefer `editor_pixel_offset_resolved()`.

## Acceptance Criteria

- The old visible-window route is gone.
- The old cursor-follow route is gone.
- Associated old tests are gone.
- Editor scrolling no longer depends on `egui::ScrollArea` for core behavior.
- The local scroll implementation owns viewport size, content extent, clamping, and scroll target resolution.
- Small and large files use the same viewport-first rendering path.
- Wrapped and unwrapped files can scroll to EOF.
- Split panes keep independent scroll state and independently computed extents.
- Search jumps, arrow keys, page keys, mouse wheel, scrollbar drag, and selection edge autoscroll all pass through the new scroll manager.
- Fresh tests cover the new behavior rather than old implementation details.
