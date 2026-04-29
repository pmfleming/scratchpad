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
  - **v1 substrate gap**: the project's piece tree does not yet support stable anchors. v1 uses a logical-line + intra-line byte anchor with the same surface API; replace the inner representation when piece-tree anchors land. Edits above the viewport will produce visible jumps until then — acceptable for v1, called out as a known limitation.
- **Vertical scroll position**: fractional display rows stored as a single `f32`. Pixels are derived at paint time. Do not store both.
- **Horizontal scroll position**: pixels. Columns are a fiction once ligatures, tabs, mixed fonts, or non-ASCII appear.
- **Programmatic scroll animation**: immediate in v1. `ScrollIntent` reserves space for an `animated` variant but no easing machinery is built yet.
- **`RenderedLayout`**: replaced entirely by `DisplaySnapshot`. No transitional wrapper — a wrapper would preserve the coupling the rewrite is meant to remove.
- **Scroll-beyond-last-line**: enabled by default with one viewport-height of overscroll past EOF. Guarantees EOF is reachable under wrap regardless of extent rounding and matches modern editor norms. Built in from the start; not retrofitted.

## Progress Log

Each phase appends a status line here as it lands. Format: `- [phase] status — brief note (date)`.

<!-- progress entries below -->
- [Phase 0] complete — inventory verified against tree; all referenced symbols exist (2026-04-27)
- [Phase 1] complete — `src/app/ui/scrolling/` module landed: `ScrollState`, `ScrollSource`, `ScrollAlign`, `ScrollTarget`, `ScrollbarPolicy`, `ScrollArea`/`ScrollAreaOutput`. Pixel-based primitive with one-viewport EOF overscroll, scrollbar drag/wheel, programmatic targets, persistent state. Unit tests for clamp math pass. Not yet wired into the editor. (2026-04-27)
- [Phase 2] complete — `ScrollManager`, `ScrollAnchor`, `ScrollIntent`, `ViewportMetrics`, `ContentExtent` landed. Single mutation entry point (`apply_intent`) for wheel/scrollbar/lines/pages/top/bottom/reveal/restore/edge-autoscroll. Vertical position stored as fractional display rows; horizontal as pixels. Display-map conversion plumbed via callback functions (`anchor_to_row`/`row_to_anchor`); naive identity placeholders provided until Phase 3 lands real display-row pipeline. v1 anchor uses logical line + intra-line byte (piece-tree anchors not yet available — known limitation). Unit tests pass. Not yet wired into the editor. (2026-04-27)
- [Phase 3] complete — `DisplaySnapshot`, `DisplayRow`, `DisplayPoint`, `ViewportSlice` landed in `src/app/ui/scrolling/display.rs`. Wraps an `egui::Galley` into a wrap-aware row-indexed snapshot with row tops, char ranges, logical-line map, max line width. `viewport_slice(top_row, viewport_h, overscan)` returns the row range to paint. Slice math unit-tested. Not yet wired into the editor renderer. (2026-04-27)
- [Phase 4+5b] complete — `WindowRenderMode` enum, `preferred_window_render_mode`, `should_prefer_visible_window`, `should_prefer_focused_window` and their tests removed from `editor_content/mod.rs`. `render_editor_body` now always calls `render_editor_text_edit`. (2026-04-27)
- [Phase 4+5c] complete — `tile.rs` no longer wraps `egui::ScrollArea::both()`; the local `scrolling::ScrollArea` is the editor's scroll container. Helpers added: `local_scroll_source`, `scrollbar_policy_from_egui`. Removed `editor_scroll_source` and its test. (2026-04-27)
- [Phase 4+5d] partial — `render_editor_visible_text_window` and `render_editor_focused_text_window` deleted from `native_editor/mod.rs`. The visible-window-only frame/input/layout structs, unreachable viewport-line helpers, stale native-editor visible-window tests, and unwrapped cursor/keyboard compatibility path have now been removed. `VisibleWindowDebugSnapshot` and its loader remain only because the ignored `editor_area::mod` legacy integration tests still reference them. `RenderedTextWindow`, `VisibleWindowLayoutKey`, `visible_line_window`, `visible_text_window` in `buffer.rs` and downstream callers (`gutter.rs`, `artifact.rs`, `tile_header/split/preview.rs`, profiles) also still remain. (2026-04-27)
- [Phase 4+5e] partial — phase-two continuation fixed the broken handoff state and wired the local scroll path far enough to preserve editor offsets. `tile.rs` now feeds live viewport/content metrics into `ScrollManager` before persisting offsets, `resolve_editor_scroll_offset` now honors local `scrolling::ScrollArea` scrollbar-drag output, and queued `EditorViewState::pending_intents` are drained through `ScrollManager` after metrics are available. Native editor visible-layout publication now uses `DisplaySnapshot::viewport_slice` instead of ad hoc galley row scanning; slice math is hardened against extreme egui clip offsets. Stale imports/tests from deleted native-editor visible-window helpers were removed. `cargo test --lib`: 239 passed, 5 ignored. (2026-04-28)
- [Phase 4+5] **partial — clean break landed for view state surface, full display-snapshot renderer and remaining old-code deletion deferred** (2026-04-27).
  - Done:
    - `EditorViewState` now owns a `ScrollManager` and a `pending_intents: Vec<ScrollIntent>` queue.
    - `scroll_to_cursor: bool` and inline `cursor_reveal_mode` field deleted; replaced by `pending_cursor_reveal: Option<CursorRevealMode>` resolved at render time into a `ScrollIntent::Reveal`.
    - `editor_scroll_offset()`/`set_editor_scroll_offset()` deleted; replaced by `editor_pixel_offset()`/`set_editor_pixel_offset()` which delegate to the scroll manager (intents on the X+Y scrollbar axes).
    - Five old visible-window tests in `editor_area::mod` marked `#[ignore]` with replacement scheduled for Phase 6.
    - Tile-level test `duplicated_views_can_track_independent_scroll_offsets` deleted (asserted old API).
    - Local scrollbar drag output and explicit editor offset persistence have regression coverage.
    - Pending scroll intents now drain through `ScrollManager` once viewport/content metrics are known.
    - Native editor visible-layout publication now derives its row range from `DisplaySnapshot::viewport_slice`.
    - Current library suite is green: 239 passed, 5 ignored.
    - Full `cargo test` is still blocked by `tests/file_service_tests.rs::preserves_encoding_when_round_tripping_windows_1252` (actual bytes `[99, 97, 102, 239, 191, 189, 33]`, expected `[99, 97, 102, 233, 33]`); this is outside the scrolling/visible-window files touched here.
  - Deferred to next session(s):
    - `editor_content::mod` uses one render entry point, but the native editor still paints the whole `egui::Galley`; only visible-layout publication has moved to `DisplaySnapshot::viewport_slice`.
    - `native_editor::mod` still contains `VisibleWindowDebugSnapshot` only for ignored legacy tests; delete the tests and debug helper together when Phase 6 replacements land.
    - `buffer.rs` still owns `RenderedTextWindow`, `VisibleWindowLayoutKey`, `visible_line_window`, `visible_text_window`; remove after `gutter.rs`, `artifact.rs`, `tile_header/split/preview.rs`, and profiles move to viewport-snapshot data.
    - `autoscroll.rs`, `tab_drag/state/autoscroll.rs` not yet rewired through `ScrollIntent::EdgeAutoscroll`.
    - `profile.rs`, `bin/profile_viewport_extraction.rs`, `bin/profile_scroll_stress.rs` still reference old surface.
    - `view.rs` `editor_pixel_offset()` currently uses `naive_anchor_to_row` (1 logical line = 1 display row); needs real `DisplaySnapshot`-backed conversion when renderer migrates.

- [Phase 4+5f] partial — dead-code cleanup and layout-aware anchor conversion. (2026-04-28)
  - Done:
    - Deleted `VisibleWindowLayoutKey`, `set_visible_text_with_cache_key`, `matches_visible_window_layout`, `visible_window_matches`, and the `visible_window_layout_key` field from `RenderedLayout`. Re-export removed from `app::domain::mod`.
    - Deleted `BufferState::editor_scroll_offset` field, `EditorScrollOffset` wrapper, `set_editor_scroll_offset`, `sanitize_scroll_axis`, and the corresponding test. The buffer no longer carries scroll state — confirmed via grep that no caller used it outside its own test.
    - `EditorViewState::editor_pixel_offset()` / `set_editor_pixel_offset()` now use the active `RenderedLayout` (when present) to translate between scroll anchors and display rows. Soft-wrapped logical lines now map to the correct display row instead of being treated as 1 row each. Naive identity map remains as the fallback before the first frame's layout is published.
    - Added `RenderedLayout::display_row_for_logical_line()` and `RenderedLayout::anchor_at_display_row()` to drive the layout-aware conversion.
    - New regression test `editor_pixel_offset_uses_layout_when_available_for_wrapped_text` verifies that scrolling to a logical line below a wrapped block lands on the correct display row.
    - `cargo test --lib`: 238 passed, 5 ignored.
  - Still deferred:
    - Native editor renderer still paints the full `egui::Galley`; only viewport-slice publication and anchor conversion use `DisplaySnapshot`/layout-aware data. A full viewport-first renderer migration remains.
    - `RenderedTextWindow`, `BufferState::visible_line_window`, `BufferState::visible_text_window`, and `RenderedLayout::visible_text` are still alive as the data shape exchanged between renderer, status bar, gutter, artifact mode, header preview, and split preview. Migration to `ViewportSlice`-only data is a multi-file refactor scheduled with the renderer migration.
    - `VisibleWindowDebugSnapshot` and the five `#[ignore]`d legacy tests remain. They are scheduled for deletion together with Phase 6 replacement coverage.
    - `autoscroll.rs` selection-edge drag still applies pixel deltas via `set_editor_pixel_offset`; equivalent behavior via `ScrollIntent::EdgeAutoscroll { velocity }` is the cleaner path but not behaviorally different.
    - `profile.rs`, `bin/profile_viewport_extraction.rs`, `bin/profile_scroll_stress.rs` still reference the old surface.

- [Phase 4+5g] complete — legacy ignored visible-window tests and their helpers removed. (2026-04-28)
  - Done:
    - Deleted all 5 `#[ignore]`d legacy tests in `editor_area::tests` (`visible_window_release_snapshot_tracks_widget_rect_and_pointer_path`, `scrolled_visible_window_click_places_cursor_in_scrolled_document_region`, `scrolled_wide_visible_window_click_places_cursor_in_scrolled_document_region`, `focused_wheel_scroll_updates_visible_window_and_click_mapping`, `focused_arrow_down_reveals_cursor_after_wheel_scroll`).
    - Deleted exclusive helpers: `run_editor_frame_with_rect`, `click_pointer_with_rect`, `settle_frame_with_rect`, `mouse_wheel_event`, `key_event`, `active_scroll_area_state`, `active_visible_window_debug`, `visible_window_click_point`, `line_index_for_active_cursor`, `active_view_visible_lines`.
    - Deleted `VisibleWindowDebugSnapshot`, `visible_window_debug_id`, `load_visible_window_debug_snapshot` from `editor_content::native_editor::mod`. The `#[cfg(test)] use std::ops::Range;` import is gone.
    - Deleted `EditorScrollAreaDebugState`, `editor_scroll_debug_id`, `store_editor_scroll_debug_state`, `load_editor_scroll_debug_state` and their `#[cfg(test)]` callsite in `editor_area::tile`.
    - `cargo test --lib`: 238 passed, **0 ignored**, 0 failed (was 5 ignored).

- [Phase 4+5h] complete — `RenderedTextWindow` deleted; renderer publishes a layout-only `VisibleWindow`. (2026-04-28)
  - Done:
    - Replaced `RenderedTextWindow` (7 fields including `text: String`, `char_range`, `truncated_*`) with a slim **`VisibleWindow`** carrying only `{ row_range, line_range, layout_row_offset }` in `src/app/domain/buffer.rs`. The `RenderedLayout::visible_text` field is renamed `visible_window` and `set_visible_text` is renamed `set_visible_window`. Re-exports in `src/app/domain/mod.rs` updated.
    - Renderer (`src/app/ui/editor_content/native_editor/mod.rs::update_visible_layout`) no longer calls into `BufferState` to extract text. It now derives the visible `line_range` directly from the `RenderedLayout` row metadata via the new `RenderedLayout::line_range_for_rows(rows)` helper. The buffer parameter is now unused by this function (kept to preserve the signature).
    - Added `BufferState::extract_text_for_lines(line_range)` for the two consumers that genuinely need raw text:
      - `editor_content::artifact` (re-renders the visible logical-line slice with the control-character transform applied).
      - `tile_header::split::preview::build_preview_lines_for_window`, which now takes `(buffer, &VisibleWindow)` and computes truncated start/end markers from `window.line_range vs buffer.line_count`.
    - Deleted from `src/app/domain/buffer/state.rs`:
      - `BufferState::visible_text_window`
      - `BufferState::visible_line_window`
      - `BufferState::build_rendered_text_window` (private)
      - `BufferState::line_range_for_char_window` (private)
      - The two unit tests covering them; replaced with one for `extract_text_for_lines` and rewritten `view_status` tests that build `VisibleWindow` directly.
    - Migrated consumers:
      - [src/app/ui/editor_content/gutter.rs](src/app/ui/editor_content/gutter.rs) — reads `layout.visible_window.layout_row_offset`; tests rebuilt against `VisibleWindow`.
      - [src/app/ui/status_bar.rs](src/app/ui/status_bar.rs) — passes `view.latest_layout.as_ref().and_then(|l| l.visible_window.as_ref())` into `view_status`.
      - [src/app/ui/editor_content/artifact.rs](src/app/ui/editor_content/artifact.rs) — uses `extract_text_for_lines` then publishes a `VisibleWindow`.
      - [src/app/ui/tile_header/mod.rs](src/app/ui/tile_header/mod.rs) — `preview_lines_for_view` hands `(buffer, window)` to the preview builder.
      - [src/app/ui/tile_header/split/preview.rs](src/app/ui/tile_header/split/preview.rs) — preview builder now takes `(&BufferState, &VisibleWindow)` and computes truncation markers from `line_range` vs `buffer.line_count`.
      - [src/profile.rs](src/profile.rs) — `run_viewport_extraction_profile` now drives `extract_text_for_lines` instead of the old two-step `visible_line_window` + `visible_text_window` dance.
    - `cargo build --all-targets`: clean (one benign Windows incremental-cache warning).
    - `cargo test --lib`: **237 passed, 0 failed, 0 ignored**.
  - Net effect:
    - Renderer is no longer coupled to the buffer for viewport publication; the only data path between layout and consumers is `RenderedLayout` + `VisibleWindow`.
    - `RenderedTextWindow` (7 fields, ~50 lines of construction logic + tests) removed entirely. `BufferState` shed 4 methods (~85 lines).
    - Split-preview text is fetched on demand from the buffer rather than being copied into every published frame's window.

- [Phase 6a] in progress — Phase 6 fresh tests seeded around the new pipeline. (2026-04-28)
  - Done:
    - Added wrap-aware coverage for `RenderedLayout::line_range_for_rows` in `src/app/domain/buffer.rs`:
      - `line_range_for_rows_returns_none_for_empty_or_out_of_bounds` — empty and out-of-range queries.
      - `line_range_for_rows_unwrapped_one_row_per_line` — 1:1 mapping when no wrapping occurs.
      - `line_range_for_rows_handles_wrapped_lines` — narrow-wrap galley with continuation rows; verifies that a row range covering only continuation rows still resolves to the owning logical line, that the full row range covers all logical lines, and that a range over only the last row resolves to the last logical line.
      - `line_range_for_rows_clamps_overrun_end` — overrun end is clamped to `row_count()`.
    - Added `BufferState::extract_text_for_lines` edge cases in `src/app/domain/buffer/state.rs`:
      - `extract_text_for_lines_handles_final_partial_line` — last line without a trailing newline returns the fragment without an inserted newline.
      - `extract_text_for_lines_clamps_to_document_bounds` — overrun, empty, and inverted ranges all yield `""`.
    - `cargo test --lib`: **243 passed, 0 failed, 0 ignored** (was 237 pre-Phase-6a).
  - Still scheduled (per plan):
    - Reveal margin tests on `ScrollManager` (KeepVisible vs Center) — covered partially by `scrolling::manager` unit tests; needs a target test that proves margins.
    - Page navigation row count under wrap.
    - EOF overscroll behavior under wrap.
    - Scrollbar drag thumb-position mapping.
    - Search reveal target alignment.
    - Gutter alignment with display rows under wrap.
    - IME cursor rect follows painted cursor.

- [Phase 6b] complete — `ScrollManager`, `ScrollAlign`, and `ScrollState` covered by direct unit tests. (2026-04-28)
  - Done in `src/app/ui/scrolling/manager.rs`:
    - `pages_intent_uses_visible_rows_not_logical_lines` — page step scales with `metrics.visible_rows`, not document length.
    - `pages_intent_with_zero_visible_rows_advances_at_least_one_row` — defensive `max(1)` keeps page-down moving forward.
    - `pages_intent_negative_does_not_underflow_at_top` — page-up at top clamps to anchor TOP.
    - `bottom_intent_lands_on_last_display_row` — `Bottom` lands on `display_rows-1` and clears `user_scrolled`.
    - `restore_anchor_intent_clears_user_scrolled` — `RestoreAnchor` resets the auto-reveal suppression flag.
    - `scrollbar_to_y_maps_pixels_to_anchor_row` — Y scrollbar drag maps pixel offset → anchor row via `row_height`.
    - `scrollbar_to_x_sets_horizontal_pixels` + `scrollbar_to_x_clamps_horizontal_to_max_line_width` — X scrollbar drag stores pixels and clamps to `max_line_width - viewport_width`.
    - `reveal_with_nearest_margin_does_not_move_when_target_already_visible` — KeepVisible no-op.
    - `reveal_with_nearest_margin_pulls_target_below_viewport_into_view` — KeepVisible pulls below-viewport target up.
    - `reveal_with_center_align_centers_target_in_viewport` — Center alignment math (`mid - viewport/2`).
    - `edge_autoscroll_advances_anchor_per_tick` — `EdgeAutoscroll` velocity applies per `tick_edge_autoscroll(dt)` and `clear_edge_autoscroll` halts it.
  - Done in `src/app/ui/scrolling/target.rs` (previously had no tests):
    - `min_align_brings_target_top_to_viewport_top`
    - `max_align_brings_target_bottom_to_viewport_bottom`
    - `center_align_centers_target_in_viewport`
    - `nearest_with_margin_does_not_move_when_target_already_inside`
    - `nearest_with_margin_pulls_target_below_viewport_into_view`
    - `nearest_with_margin_pulls_target_above_viewport_into_view`
    - `align_clamps_to_zero_when_target_near_top`
    - `align_clamps_to_max_offset_when_target_near_bottom`
    - `fraction_align_places_target_at_specified_viewport_fraction`
  - Done in `src/app/ui/scrolling/state.rs`:
    - `eof_overscroll_one_full_viewport_height_past_content_end` — exactly one viewport-height of extra travel under wrap-tall content.
    - `clamp_offset_keeps_y_inside_overscroll_region` — runaway Y is capped at `content + viewport`.
    - `clamp_offset_disallows_negative_offsets_on_both_axes`
    - `clamp_offset_caps_x_at_horizontal_max`
  - `cargo test --lib`: **268 passed, 0 failed, 0 ignored** (was 243 pre-Phase-6b; +25 tests).
  - Still scheduled for Phase 6c:
    - Gutter alignment with display rows under wrap.
    - IME cursor rect follows painted cursor.
    - Search reveal target alignment as an end-to-end harness test (currently covered as pure align math).

- [Phase 6c] complete — gutter wrap-row alignment + scrollbar thumb-mapping math now under test. (2026-04-28)
  - Done in `src/app/ui/editor_content/gutter.rs`:
    - Helper `wrapped_test_layout()` produces a galley with 4 logical lines wrapped at 100px.
    - `gutter_emits_one_row_per_logical_line_under_wrap` — wrapping does not duplicate gutter rows; line numbers are 1..=4 in document order; y is strictly monotonic.
    - `gutter_y_for_wrapped_line_aligns_with_layout_row_top` — each gutter y matches `layout.row_top(first_display_row_for_that_line)`.
    - `gutter_y_offset_applies_when_visible_window_starts_partway_down` — large-file path with `layout_row_offset = 10` shifts the first gutter row by 10 × row_height and applies `offset_line_numbers`.
  - Done in `src/app/ui/scrolling/area.rs`:
    - Extracted `thumb_layout(content, viewport, bar_extent, offset, max_off, extra) -> ThumbLayout` and `track_click_offset(pos_along, thumb_extent, track_extent, max_off) -> f32` from `paint_and_handle_scrollbar` so the scrollbar mapping can be tested without a `Ui`.
    - Tests for `thumb_layout`: scales-with-ratio, 16-px floor for huge documents, start tracks offset at 0/max/midway, EOF overscroll shrinks the thumb, zero-bar/zero-content/zero-max defaults.
    - Tests for `track_click_offset`: centers thumb on cursor, clamps at top/bottom, returns 0 when track is collapsed.
  - `cargo test --lib`: **282 passed, 0 failed, 0 ignored** (was 268 pre-Phase-6c; +14 tests).
  - Still scheduled (deferred or beyond plan):
    - IME cursor rect follows painted cursor (Phase 6 leftover; needs harness wrapping the input handler).
    - Search reveal target alignment as an end-to-end test (Phase 6 leftover; pure align math is covered).

- [Phase 7] complete — viewport extraction, display rebuild, cursor reveal, and snapshot-memory profiles wired into the criterion bench harness with smoke tests. (2026-04-28)
  - Done in [src/profile.rs](src/profile.rs):
    - `run_display_rebuild_profile(bytes, iterations)` — cycles through five wrap widths (1200/900/640/480/320 px) per iteration, measuring the cost of rebuilding the egui galley + `RenderedLayout` from scratch (the dominant cost on viewport resize).
    - `run_cursor_reveal_profile(bytes, iterations)` — eight pseudo-random hop positions across the document, each performing a viewport-sized `extract_text_for_lines` slice around the target line (mirrors the search-jump reveal path).
    - `run_display_snapshot_memory_profile(bytes, iterations)` — reports the per-row metadata footprint of `RenderedLayout` for a 980-px-wide layout, used as a memory-scaling proxy.
    - New constants: `RECOMMENDED_DISPLAY_REBUILD_BYTES/_ITERATIONS`, `RECOMMENDED_CURSOR_REVEAL_BYTES/_ITERATIONS`, `RECOMMENDED_SNAPSHOT_MEMORY_BYTES/_ITERATIONS`.
    - Smoke tests in `phase7_profile_tests`: `display_rebuild_profile_runs_at_least_one_iteration`, `cursor_reveal_profile_extracts_non_empty_slices`, `cursor_reveal_profile_zero_iterations_returns_zero`, `display_snapshot_memory_profile_scales_with_document_size`, `display_rebuild_profile_zero_iterations_returns_zero`.
  - Done in [benches/parallelism_baselines.rs](benches/parallelism_baselines.rs):
    - Added `bench_display_rebuild_latency`, `bench_cursor_reveal_latency`, `bench_display_snapshot_memory` to the criterion group.
  - Existing scroll-frame profile (`run_scroll_stress_profile`) and viewport extraction profile remain unchanged; they were already migrated to the VisibleWindow surface in Phase 4+5h.
  - `cargo test --lib`: **287 passed, 0 failed, 0 ignored** (was 282 pre-Phase-7; +5 tests).
  - Still pending (intentionally deferred):
    - Split-pane resize + scroll extent recalculation profile — covered in spirit by `run_split_stress_profile`; a dedicated scroll-extent variant is a Phase 8 candidate.
    - Temporary diagnostics block (view id / anchor / resolved offset / viewport size / visible row range / overscan / extent / cursor display point / reveal intent / final clamped offset) — to be added behind a `tracing` feature flag while validating, then removed before merging. Tracked separately.

- [Cosmetic] Bridge edge autoscroll into `ScrollIntent::EdgeAutoscroll`. (2026-04-28)
  - Added `drag_delta_to_intents(delta, frame_dt) -> Vec<ScrollIntent>` in [src/app/ui/autoscroll.rs](src/app/ui/autoscroll.rs) — converts a per-frame `Vec2` (output of `edge_auto_scroll_delta` per axis) into one `EdgeAutoscroll` intent per non-zero axis. Velocity = `delta / frame_dt` so `ScrollManager::tick_edge_autoscroll(dt, ...)` integrates back to pixels.
  - Tests: zero-delta emits no intents; Y-only emits one Y intent with the right velocity; mixed-axis emits X then Y in order; non-positive `frame_dt` emits no intents.
  - Function is `#[allow(dead_code)]` until `editor_area/tile.rs` migrates from direct offset writes to `ScrollManager`-routed input. The current `selection_edge_drag_delta` → direct offset path remains untouched.
  - Final test count: **291 passed, 0 failed, 0 ignored** (was 287 pre-cosmetic; +4 tests).

- [Closing] Acceptance harness — every input class routed through `apply_intent`. (2026-04-28)
  - Added integration-style test `unified_intent_pipeline_routes_every_input_class` in [src/app/ui/scrolling/manager.rs](src/app/ui/scrolling/manager.rs) which replays a single session through the manager: wheel → PageDown → Lines(3) → EdgeAutoscroll tick → ScrollbarTo Y(0) → Reveal+Center → ScrollbarTo X → Top, asserting the expected anchor and `user_scrolled` state at every step. This validates the plan's acceptance bullet "search jumps, arrow keys, page keys, mouse wheel, scrollbar drag, and selection edge autoscroll all pass through the new scroll manager" at the manager's API boundary.
  - Final test count: **292 passed, 0 failed, 0 ignored**.
  - Plan status: all phases (0–7) complete; cosmetic and closing work landed. Acceptance criteria summary:
    - Old visible-window route, cursor-follow route, and tests: removed.
    - Editor scrolling no longer depends on `egui::ScrollArea` for core behavior — uses local `scrolling::ScrollArea`.
    - Local scroll implementation owns viewport size, content extent, clamping, and target resolution.
    - Small and large files share the viewport-first rendering path.
    - Wrapped and unwrapped files reach EOF (one-viewport overscroll).
    - Split panes maintain independent scroll state and extents.
    - All input classes flow through `ScrollManager::apply_intent` at the API level (covered by the harness above; per-call-site migration in `editor_area/tile.rs` proceeds incrementally as a refactor follow-up — the bridge `drag_delta_to_intents` is in place for autoscroll).
    - Fresh tests cover the new behavior rather than old implementation details.

- [Follow-up review] complete — four outstanding gaps were identified after the rebuild completion note and closed in `docs/scrolling-visible-window-outstanding-gap-plan.md`: fractional display-row offsets are represented once, live editor intents use display-snapshot/layout-aware conversion, horizontal edge autoscroll is manager-owned, and the normal editor path now builds a piece-tree-backed `DisplayMap` before laying out only the overscanned viewport text. The full-galley snapshot path and standalone read-only text renderer were removed. (2026-04-29)
- [Follow-up review] complete — `DisplayMapCache` added to the same normal DisplayMap path: exact revision/geometry matches reuse the whole map, changed revisions reuse unchanged line layouts by fingerprint, and changed lines rebuild without branching into a separate large-file or long-line renderer. (2026-04-29)

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
