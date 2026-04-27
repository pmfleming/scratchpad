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

Create a local editor scroll subsystem, tentatively under `src/app/ui/scrolling/` or `src/app/domain/scrolling/`, with clear ownership boundaries:

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
  - stable anchor plus fractional row/pixel offset
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
- optional scroll-beyond-last-line policy
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

## Open Decisions

- Whether the new local scroll module should live under UI or domain.
- Whether `ScrollAnchor` should anchor to byte offset, character offset, logical line, or piece-tree anchor.
- Whether scroll position should store fractional display rows, pixels, or both.
- Whether horizontal scrolling should use columns, pixels, or a hybrid.
- Whether the first version should support animated programmatic scroll or keep all programmatic scroll immediate.
- Whether old `RenderedLayout` should be replaced entirely by a new `DisplaySnapshot` or retained as a transitional wrapper.

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
