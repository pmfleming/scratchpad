# Scrolling and Visible-Window Outstanding Gap Plan

Date: 2026-04-29

## Goal

Close the four outstanding review findings from the scrolling and visible-window rebuild without reopening the old visible-window architecture.

The previous rebuild and gap-closure plans removed the old render-mode fork and introduced the local scrolling subsystem. This follow-up plan focuses on the remaining correctness and architecture gaps found during review:

- fractional scroll row offsets are double-counted
- live editor scroll intents still use naive row mapping
- the renderer still lays out the full document before slicing
- horizontal selection-edge autoscroll is emitted but ignored

## Non-Goals

- Do not restore `WindowRenderMode`, `RenderedTextWindow`, `VisibleWindow`, or the old focused/read-only visible-window fork.
- Do not replace the local `src/app/ui/scrolling/` subsystem with `egui::ScrollArea`.
- Do not optimize large-file loading, search, or piece-tree storage except where required for viewport-first rendering.
- Do not treat passing manager-only tests as acceptance for live editor call sites.

## Phase 1: Fix Fractional Display-Row Semantics

### Problem

`ScrollManager` treats the anchor conversion callback as if it returns only the wrapped block's base row, then adds `anchor.display_row_offset`. The current callback implementations already include the fractional offset:

- `scrolling::naive_anchor_to_row(anchor)` returns `logical_line + display_row_offset`
- `domain::view::layout_anchor_to_row(anchor)` returns `display_row_for_logical_line + display_row_offset`

That means a 1.5-row anchor can be interpreted as 2.0 rows in `top_display_row`, `pixel_offset_y`, line/page movement, wheel scrolling, and reveal math.

### Work

1. Define the callback contract explicitly.
   - Preferred contract: `anchor_to_row(anchor)` returns the full fractional display row.
   - `row_to_anchor(row)` accepts the full fractional display row and stores the fraction in `ScrollAnchor::display_row_offset`.

2. Remove duplicate fraction additions in `ScrollManager`.
   - Update `top_display_row`.
   - Update `Lines`, `Pages`, and pixel scrolling calculations.
   - Audit reveal and clamp paths for the same assumption.

3. Add regression tests with non-integer rows.
   - `top_display_row` returns `1.5`, not `2.0`.
   - wheel scrolling by half a row preserves a `.5` fraction.
   - page movement from a fractional row advances by visible rows plus the original fraction.
   - reveal from a fractional offset resolves from the correct current pixel offset.

### Acceptance Criteria

- A single callback contract is documented in `manager.rs`.
- No `ScrollManager` method adds `display_row_offset` to a callback result that already includes it.
- Fractional scroll positions round-trip through `editor_pixel_offset`.

## Phase 2: Apply Live Editor Intents With Layout-Aware Mapping

### Problem

The live editor path applies pending intents with:

- `scrolling::naive_anchor_to_row`
- `scrolling::naive_row_to_anchor`

This bypasses the wrap-aware `RenderedLayout` conversion already used by `EditorViewState::editor_pixel_offset()` and `set_editor_pixel_offset()`. As a result, wheel, page, reveal, scrollbar, and edge-autoscroll intents can resolve using logical-line coordinates instead of display-row coordinates under soft wrap.

### Work

1. Add layout-aware intent application on `EditorViewState`.
   - Example API: `EditorViewState::apply_pending_scroll_intents()`.
   - Internally clone or borrow the latest layout once, then pass layout-aware closures into `ScrollManager::apply_intent`.
   - Use the same conversion logic as `set_editor_pixel_offset`.

2. Replace the tile-level naive bridge.
   - `src/app/ui/editor_area/tile.rs::apply_pending_scroll_intents` should delegate to the view method.
   - `tick_edge_autoscroll` should also use layout-aware conversion.

3. Recheck frame ordering.
   - Apply queued intents before preloading the local `ScrollState`.
   - After rendering publishes a fresh display snapshot/layout, ensure the next frame uses that layout for scroll conversion.
   - Decide whether changed frames should use the previous layout, the newly rebuilt layout, or a safe fallback.

4. Add integration-style tests using wrapped text.
   - Wheel over a wrapped first logical line advances by display rows.
   - PageDown uses visible display rows under wrap.
   - Center reveal below a wrapped block lands on the expected display row.
   - Selection edge autoscroll uses wrapped display-row coordinates.

### Acceptance Criteria

- No production editor path applies `ScrollIntent` with naive conversion when a current layout exists.
- Naive conversion remains only as a documented first-frame fallback or pure unit-test helper.
- Wrapped and unwrapped documents use the same live scroll intent path.

## Phase 3: Implement Horizontal Selection-Edge Autoscroll

### Problem

`drag_delta_to_intents` emits `ScrollIntent::EdgeAutoscroll` for both axes, but `ScrollManager` ignores `Axis::X`. This creates a misleading intent pipeline: the tile layer appears to request horizontal autoscroll, but long unwrapped lines do not move horizontally during selection drag.

### Work

1. Store horizontal edge-autoscroll velocity in `ScrollManager`.
   - Add `edge_autoscroll_x: f32`.
   - Apply it in `tick_edge_autoscroll`.
   - Clamp through the existing horizontal max-width logic.

2. Clear both axes together.
   - `clear_edge_autoscroll` should reset X and Y.
   - If one axis is active and the other is zero, preserve the active axis behavior.

3. Add tests.
   - X edge autoscroll advances `horizontal_px`.
   - X edge autoscroll clamps to `max_line_width - viewport_width`.
   - clearing autoscroll stops both axes.
   - mixed X/Y autoscroll updates both horizontal pixels and vertical anchor.

4. Validate live pointer behavior.
   - Dragging near left/right edges of an unwrapped long line should move horizontally.
   - Dragging near top/bottom should preserve existing vertical behavior.

### Acceptance Criteria

- `Axis::X` is not ignored by `ScrollIntent::EdgeAutoscroll`.
- Horizontal and vertical selection-edge autoscroll share the same manager-owned lifecycle.

## Phase 4: Convert Full-Galley Slicing Into True Viewport Layout

### Problem

The unified native editor renderer still performs full-document layout:

1. flatten text with `buffer.document().text_cow()`
2. build a full-document galley
3. derive a `DisplaySnapshot` from that full galley
4. compute a `ViewportSlice`
5. build a second galley for visible paint

This removes the old render-mode fork, but it is still not the viewport-first contract from the rebuild plan. The display map and paint path are still downstream of a full-document layout cost.

### Strategy

Treat this as a staged architecture change, not a quick patch. The first three phases above are correctness fixes. This phase replaces the remaining full-galley dependency with a bounded display-map and viewport layout pipeline.

### Work

1. Split display-map construction from paint-galley construction.
   - Introduce a display-map type that can answer row-to-source-span queries without requiring a full paint galley.
   - Remove the `DisplaySnapshot::from_galley` adapter once the display map is authoritative.

2. Add a bounded viewport source extraction API.
   - Given top display row, viewport height, wrap width, font metrics, and overscan, return source spans needed for paint.
   - Avoid whole-document `text_cow()` for normal paint.
   - Use piece-tree range extraction for the viewport text.

3. Build paint layout only for the viewport slice.
   - Rebase search highlights and selections to the viewport source span.
   - Preserve cursor and IME geometry by mapping global document positions into viewport-local layout positions.
   - Preserve gutter and status data from the same display-map snapshot.

4. Keep recovery behavior explicit.
   - If bounded mapping fails, fall back to full-galley adapter for one frame and set `EditorRenderNotice`.
   - Clear the notice after a successful bounded render.

5. Add performance and correctness tests.
   - A large document render does not call the full-document layout path when viewport data is valid.
   - Viewport paint extracts bounded text.
   - Search highlights crossing viewport boundaries are clipped correctly.
   - Cursor reveal works when the cursor starts outside the current viewport slice.
   - Split panes with different widths produce different display extents for the same buffer.

### Acceptance Criteria

- Normal editor paint no longer requires full-document `text_cow()` and full-document galley construction.
- `DisplaySnapshot` is built from the display map only; no full-galley snapshot adapter remains.
- Small and large files both use the same bounded viewport renderer.
- Existing full-galley fallback is recoverable and observable through `EditorRenderNotice`, not silent.

## Phase 5: Update Documentation And Progress Claims

### Problem

The rebuild plan currently says the phases are complete, while the implementation still has the four gaps above. That can mislead future work.

### Work

1. Add a progress note to `docs/scrolling-visible-window-rebuild-plan.md`.
   - Point to this follow-up plan.
   - Clarify that completion was provisional and these review gaps remain.

2. Add a progress note to `docs/scrolling-visible-window-gap-closure-plan.md`.
   - Point to this follow-up plan.
   - Distinguish completed gap-closure work from newly discovered follow-up gaps.

3. Keep this file as the active checklist until all four review findings are closed.

### Acceptance Criteria

- The docs do not claim unconditional completion while known P1/P2 gaps remain.
- Each finding has either a landed fix note or an explicit deferred rationale.

## Suggested Implementation Order

1. Phase 1: fractional row semantics.
2. Phase 2: layout-aware live intent application.
3. Phase 3: horizontal edge autoscroll.
4. Phase 5 documentation updates for the first three fixes.
5. Phase 4 viewport-layout migration as a larger follow-up branch.

The first three phases are bounded correctness work and should land before the larger viewport-layout migration. Phase 4 can then proceed with cleaner scroll semantics and fewer moving parts.

## Final Done Criteria

- Fractional display-row offsets are represented exactly once.
- Live editor scroll intents use layout-aware display-row conversion whenever a current layout exists.
- Horizontal selection-edge autoscroll works or is explicitly removed from emitted intents.
- Normal editor paint uses bounded viewport layout rather than full-document galley construction.
- Rebuild and gap-closure docs point to this follow-up plan until it is complete.
- `cargo fmt`, `cargo test --lib`, and relevant scroll/render integration tests pass.

## Progress Log

Each implementation slice should append a line here using this format:

`- [Phase N] status — brief note (date)`

<!-- progress entries below -->
- [Phase 1] complete — `ScrollManager` now treats `anchor_to_row` as the full fractional display-row callback and no longer double-counts `display_row_offset`; regression tests cover top-row, wheel, page, and pixel-offset fractional behavior. (2026-04-29)
- [Phase 2] complete — live editor scroll intent application now routes through `EditorViewState::apply_pending_scroll_intents()` / `tick_edge_autoscroll()`, using the latest `RenderedLayout` when available and naive conversion only as the first-frame fallback. Added wrapped-text coverage proving pending intents resolve through display rows. (2026-04-29)
- [Phase 3] complete — `ScrollManager` now stores and ticks horizontal edge-autoscroll velocity, clamps it through the horizontal extent, and clears X/Y autoscroll together. Added X-only, clamp, and clear-path tests. (2026-04-29)
- [Phase 4] complete — `DisplayMap` now builds wrap-aware display rows from piece-tree source spans without constructing a full-document paint galley. Normal editor paint derives `DisplaySnapshot` from that map, extracts only the overscanned viewport source range, and rebases search/selection/cursor positions into viewport-local coordinates. The explicit full-galley fallback renderer, `DisplaySnapshot::from_galley`, and its tests were removed. The standalone read-only text render path was also removed so control-character/artifact display no longer bypasses the normal editor renderer. (2026-04-29)
- [Phase 4] complete — `DisplayMapCache` now sits inside the same DisplayMap build path used by every normal editor render. Exact revision/geometry hits reuse the whole map, changed revisions reuse unchanged per-line layouts by content fingerprint, and changed lines rebuild through the same piece-tree/source-span code path. No separate long-file or long-line rendering path was introduced. (2026-04-29)
