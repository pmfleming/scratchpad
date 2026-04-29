# Scrolling and Visible-Window Gap Closure Plan

Date: 2026-04-28

## Goal

Finish the scrolling and visible-window rebuild described in `docs/scrolling-visible-window-rebuild-plan.md` by closing the current hybrid-state gaps and adding recoverable error handling without introducing a separate error log.

The current code has useful scaffolding: `src/app/ui/scrolling/` exists, editor views own a `ScrollManager`, and the local scroll area has replaced the direct `egui::ScrollArea` wrapper. The remaining work is to make that new model the operational path instead of a compatibility layer around raw pixel offsets and `VisibleWindow` metadata.

## Non-Goals

- Do not add a file-backed error log or a new logging service for scroll/render errors.
- Do not preserve old visible-window behavior by mechanically updating tests.
- Do not redesign unrelated editor input, storage, search, or tab-drag systems unless they need a small API adaptation for the new scroll contract.
- Do not optimize galley construction before the viewport-first contract is correct and covered by tests.

## Current Gaps To Close

### 1. Scroll Ownership Is Still Split

`ScrollManager` exists, but the live editor path still resolves raw pixel offsets in `src/app/ui/editor_area/tile.rs` through wheel, drag, cursor reveal, and page-navigation helpers. Pending intents are applied after render, so the rendered frame may consume stale scroll state.

Close by making `ScrollManager` the only mutable scroll-position owner for editor views:

- Convert wheel input into `ScrollIntent::Wheel` before rendering the editor body.
- Convert scrollbar drags into `ScrollIntent::ScrollbarTo` inside the local scroll area output bridge.
- Convert page navigation into `ScrollIntent::Pages` rather than returning raw pixel offsets from native editor code.
- Convert cursor reveal into `ScrollIntent::Reveal` with `ScrollAlign::NearestWithMargin` or `ScrollAlign::Center`.
- Convert selection-edge autoscroll into `ScrollIntent::EdgeAutoscroll` and tick it once per frame while dragging.
- Apply queued intents before computing the frame's viewport rect and render offset.
- Keep a temporary pixel facade only for egui container interop, and mark it as compatibility-only until removed.

Acceptance criteria:

- `resolve_editor_scroll_offset`, `scroll_offset_from_wheel_delta`, page-navigation pixel helpers, and selection-edge pixel offset helpers are deleted or moved behind test-only compatibility shims.
- Cursor reveal, wheel, scrollbar drag, page up/down, top/bottom, and selection-edge autoscroll all flow through `ScrollIntent`.
- Split panes retain independent scroll state because state remains view-owned.

### 2. Rendering Is Not Yet Viewport-First

The unified editor renderer still builds and paints a full-document galley, then computes a `ViewportSlice` afterward only to publish visible metadata. That removes the old focused/read-only fork, but it does not yet deliver the plan's viewport-first rendering model.

Close in two steps:

1. Establish the viewport snapshot contract.
   - Store a `DisplaySnapshot` or equivalent display-map snapshot on `EditorViewState` or a view-local render cache.
   - Make the snapshot authoritative for display rows, row tops, row character ranges, logical-line mapping, maximum line width, and content height.
   - Reconcile the current mismatch where `DisplaySnapshot` labels every wrapped display row with a logical line while `RenderedLayout` labels only the first wrapped row.

2. Move painting to viewport slices.
   - Compute visible rows from `ScrollManager.top_display_row`, `ViewportMetrics`, wrap width, font size, and overscan.
   - Build or reuse layout only for the overscanned visible rows once the display map can provide source spans.
   - Paint text, cursor, selection, search highlights, gutter, and IME rectangles from the same `ViewportSlice`.
   - Keep a full-galley fallback only as a temporary implementation detail behind a small adapter, not as the public render path.

Acceptance criteria:

- The normal editor body no longer paints the full document when a viewport slice is available.
- Focused and unfocused editors use the same viewport renderer.
- Small and large files use the same renderer; only cache strategy differs.
- Gutter and status-bar visible range are derived from the same snapshot used for text paint.

### 3. Content Extent Still Has Logical-Line Fallbacks

`editor_scroll_content_size` still folds in `buffer.line_count * row_height`. That can mask missing content extent and can reintroduce bottom-clamping bugs for wrapped lines.

Close by deriving scroll bounds only from `ContentExtent`:

- Compute `display_rows`, `height`, and `max_line_width` from the display snapshot.
- Include wrap width, font size, line height, gutter width, horizontal scrolling, EOF behavior, and split-pane width.
- Keep scroll-beyond-last-line as a scroll-area policy: one viewport height of vertical overscroll past EOF.
- Remove logical-line height estimates from editor scroll clamping.

Acceptance criteria:

- Wrapped visual rows determine vertical scroll extent.
- Long unwrapped rows determine horizontal extent.
- Split panes with different widths can have different extents for the same buffer.
- Tests cover wrapped EOF, long-line horizontal scrolling, empty buffer, final line without newline, and split width changes.

### 4. `VisibleWindow` Compatibility Needs Removal Or Renaming

`VisibleWindow` is still defined on `RenderedLayout` and consumed by gutter, status bar, artifact rendering, and split preview. It now behaves more like viewport metadata than the old visible-window renderer, but the name and API keep old assumptions alive.

Close by replacing it with viewport-snapshot data:

- Introduce a `PublishedViewport` or `ViewportSnapshot` type with visible display-row range, visible logical-line range, content rect, row offset, and source spans when available.
- Store the latest published viewport on `EditorViewState`, not inside `RenderedLayout`.
- Adapt gutter rendering to consume display-row positions from the viewport snapshot.
- Adapt status bar visible range to consume logical-line range from the viewport snapshot.
- Adapt split preview to consume a logical-line range or preview-specific snapshot, not `VisibleWindow`.
- Keep artifact mode compatible by publishing the same viewport snapshot shape.

Acceptance criteria:

- `VisibleWindow` is deleted or reduced to a migration alias with no production callers.
- `RenderedLayout` no longer owns visible viewport state.
- Gutter, status bar, and split preview agree on visible ranges because they read one published viewport source.

### 5. Cursor Reveal Needs Full Intent Semantics

`CursorRevealMode` still queues an old-style reveal flag and render code returns requested pixel offsets. This should become a typed reveal intent resolved by `ScrollManager`.

Close by introducing explicit reveal targets:

- Replace `CursorRevealMode` with a queued `RevealRequest` or direct `ScrollIntent::Reveal` once cursor geometry is known.
- Use `ScrollAlign::NearestWithMargin(CURSOR_REVEAL_MARGIN_PX)` for ordinary typing and arrow movement.
- Use `ScrollAlign::Center` or a comfortable center band for search, go-to-line, jump, and workspace navigation.
- Suppress automatic cursor snap-back after user wheel or scrollbar movement unless the next command explicitly requests reveal.
- Resolve off-slice cursor targets through the display map, not through a previously painted galley.

Acceptance criteria:

- `CursorRevealMode` and cursor reveal pixel-offset helpers are removed.
- Search and workspace jumps request centered reveal through the same API as cursor movement.
- Mouse click moves cursor without scrolling unless the result lands outside the viewport.
- Tests cover user scroll suppression, centered search reveal, ordinary keep-visible reveal, and off-slice target reveal.

### 6. Edge Autoscroll Is Only Partially Wired

`drag_delta_to_intents` and `ScrollManager::tick_edge_autoscroll` exist, but live selection-edge autoscroll still returns raw pixel offsets.

Close by moving edge autoscroll ownership into the scroll manager path:

- During selection drag, compute edge proximity and enqueue `ScrollIntent::EdgeAutoscroll`.
- Tick edge autoscroll with frame delta before rendering the next viewport.
- Clear edge autoscroll when selection drag ends or pointer leaves the allowed cross-axis margin.
- Add horizontal support or explicitly document vertical-only behavior until horizontal selection autoscroll is implemented.

Acceptance criteria:

- Selection-edge autoscroll no longer mutates raw pixel offsets.
- Autoscroll continues while the primary drag remains active outside the editor response.
- Autoscroll stops promptly when drag ends.

### 7. Tests Still Encode Old Internals

Several tests still assert pixel helper behavior or `VisibleWindow` behavior. These should be replaced after the new contract lands.

Close by replacing tests in phases:

- `scrolling::manager` unit tests: intent ordering, clamping, reveal alignment, user-scroll suppression, EOF overscroll.
- `scrolling::display` unit tests: wrapped row mapping, logical-line mapping, row char ranges, viewport overscan.
- editor tile tests: view-owned independent scroll, scrollbar bridge, wheel intent ingestion, no raw pixel helper dependency.
- native editor tests: cursor reveal requests, page navigation intents, off-slice target resolution.
- gutter/status/split preview tests: all consume the published viewport snapshot.
- regression tests: wrapped bottom scroll, split-pane width changes, long-line horizontal scroll, empty file, final line without newline.

Acceptance criteria:

- Old visible-window and raw pixel helper tests are deleted rather than mechanically rewritten.
- The new suite verifies behavior through public scroll/render contracts.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` are clean before removing compatibility shims.

## Error Handling Without An Error Log

Scrolling and viewport rendering should be resilient, but failures should not be written to a separate error log. Errors should be represented as recoverable state, surfaced in the UI when actionable, and covered by tests.

### Error Handling Principles

- Prefer typed results at boundaries where recovery is possible.
- Prefer invariant-preserving fallbacks inside per-frame rendering where failing closed is better than crashing.
- Use `debug_assert!` for programmer invariants that tests should catch, but do not rely on panics for runtime user data.
- Surface actionable failures in the existing UI, such as a status-bar message, inline editor placeholder, or non-modal notification.
- Keep transient render degradation in memory on the view or app state; do not add a persistent error-log file.
- Deduplicate repeated frame errors so one bad layout condition does not spam the UI every frame.

### Error Types

Add small typed errors near the subsystem that owns them:

- `ScrollInvariantError`
  - invalid row height
  - non-finite scroll offset
  - negative or non-finite content extent
  - invalid viewport dimensions
- `DisplaySnapshotError`
  - row mapping mismatch
  - missing char range for requested row
  - stale snapshot revision
  - invalid wrap width
- `ViewportRenderError`
  - cursor target cannot be mapped
  - requested viewport slice cannot be extracted
  - highlight or selection range is outside document bounds after an edit

These errors should be plain Rust types with `Display` implementations. A dependency such as `thiserror` is optional and should be added only if it matches existing project style.

### Recovery Strategy

For each frame:

1. Validate metrics before applying scroll intents.
   - If row height, viewport size, or content extent is invalid, clamp to a minimal safe viewport and publish a transient warning state.

2. Validate snapshot revision before using cached display data.
   - If stale, rebuild once.
   - If rebuild fails, fall back to a minimal plain-text full render for that frame and show a non-blocking editor warning.

3. Validate viewport slices before paint.
   - If the requested row range is out of bounds, clamp to available rows.
   - If source spans are missing, paint an empty safe row range and preserve scroll state.

4. Validate reveal targets before applying reveal intents.
   - If a cursor/search target cannot be mapped, keep the current scroll position and surface a status message such as `Could not reveal target in current layout`.

5. Validate gutter/status/preview consumers.
   - If published viewport data is missing, show conservative fallback values instead of panicking.

### UI Surface

Use existing app-visible state rather than a new log:

- Add an in-memory `EditorRenderNotice` or similar field on `EditorViewState` for the most recent recoverable scroll/render issue.
- Display the notice in the status bar or as a compact inline editor placeholder when the editor cannot paint text safely.
- Clear the notice automatically after a successful render for the same view and revision.
- Coalesce repeated notices by error kind and view id.

Acceptance criteria:

- No new file-backed error log is introduced.
- Recoverable scroll/render failures do not panic in release builds.
- Users receive a short, actionable message when rendering degrades.
- Tests cover invalid metrics, stale snapshot recovery, out-of-range viewport slices, and unmappable reveal targets.

## Implementation Order

### Phase 1: Freeze The New Contract

- Document the final `ScrollManager`, `ScrollIntent`, `ContentExtent`, `ViewportMetrics`, and `ViewportSnapshot` contracts in module comments.
- Add typed error enums and in-memory render notice state.
- Reconcile `DisplaySnapshot` logical-line labeling with the gutter/status needs.
- Add unit tests for display-row mapping and error validation helpers.

Exit criteria:

- The data model is explicit enough to remove old compatibility APIs without guessing.

### Phase 2: Move Input To Intents

- Convert wheel, scrollbar, page navigation, cursor reveal, and edge autoscroll to queued intents.
- Apply intents before the frame's viewport slice is computed.
- Keep only one compatibility bridge between `ScrollManager` and local pixel scroll area state.

Exit criteria:

- Live input no longer writes raw editor scroll offsets outside the scroll manager facade.

### Phase 3: Publish Viewport Snapshot

- Add `ViewportSnapshot` or `PublishedViewport` and store it on `EditorViewState`.
- Move gutter, status bar, and split preview consumers to the new snapshot.
- Remove production dependency on `VisibleWindow`.

Exit criteria:

- `RenderedLayout` no longer owns viewport visibility metadata.

### Phase 4: Make Paint Viewport-First

- Use display-row ranges and source spans to layout and paint only overscanned visible rows.
- Keep full-galley fallback behind a narrow adapter while behavior is validated.
- Ensure cursor, selection, search highlights, IME output, and gutter all use the same viewport slice.

Exit criteria:

- Normal editor paint does not render the full document when viewport data is valid.

### Phase 5: Rebuild Extent And Bounds

- Remove logical-line height fallback from scroll clamping.
- Derive `ContentExtent` from display snapshot data.
- Validate EOF overscroll, horizontal scrolling, long lines, split panes, empty files, and final line behavior.

Exit criteria:

- Wrapped-row bottom scroll bugs cannot be reproduced by logical-line underestimation.

### Phase 6: Replace Tests And Remove Shims

- Delete old visible-window and raw pixel helper tests.
- Add replacement tests against public scroll/render contracts.
- Remove compatibility comments and unused bridge helpers.
- Re-run `cargo check`, `cargo test`, and `cargo clippy --all-targets -- -D warnings`.

Exit criteria:

- The codebase no longer needs the old scroll/visible-window vocabulary for production behavior.

## Final Done Criteria

- Every editor view owns its scroll state through `ScrollManager`.
- Every scroll-affecting input is represented as a `ScrollIntent`.
- Rendering uses one viewport-first path for focused, unfocused, small-file, and large-file editors.
- Scroll extents are derived from display rows and content width, not logical line count.
- Gutter, cursor, selection, search highlights, IME, status bar, and split preview derive from one published viewport snapshot.
- Recoverable scroll/render errors are represented as typed state and surfaced in the UI without a separate error log.
- Old visible-window and raw pixel offset tests are removed or replaced with contract tests.
- `cargo check`, `cargo test`, and `cargo clippy --all-targets -- -D warnings` pass.

## Progress Notes

- 2026-04-28: Removed the remaining source/test vocabulary for `VisibleWindow`, `RenderedTextWindow`, and old raw pixel drag-offset helpers. Selection-edge autoscroll tile coverage now asserts queued `ScrollIntent::EdgeAutoscroll` values instead of clamped pixel offsets. `cargo test --lib` and `cargo clippy --all-targets -- -D warnings` pass with `CARGO_INCREMENTAL=0`.
- 2026-04-28: Completed the remaining gap-closure work. `DisplaySnapshot` now owns wrap-aware row metadata, continuation-row labeling is aligned with gutter consumers, and gutter/status/split preview/artifact consumers read `PublishedViewport` from `EditorViewState`. The normal editor paint path builds an overscanned viewport galley from `ViewportSlice` source spans and falls back to full-galley paint only through the recoverable `ViewportRenderError` adapter. `ScrollInvariantError`, `DisplaySnapshotError`, `ViewportRenderError`, and `EditorRenderNotice` cover recoverable scroll/render degradation without a file-backed log.
- 2026-04-28: Replaced `CursorRevealMode` with `RevealRequest`; cursor movement, search activation, workspace mutation, and native rendering now resolve reveals into queued `ScrollIntent::Reveal` values. The old cursor pixel-offset helper tests were deleted and replaced with direct reveal-intent coverage for keep-visible margin and centered reveal. A source scan confirms no remaining production/test references to `VisibleWindow`, `RenderedTextWindow`, `CursorRevealMode`, `scroll_offset_from_wheel_delta`, or cursor reveal pixel-offset helpers under `src/`.
- 2026-04-28: Final scrolling validation after the closing slice: `cargo fmt`, `cargo test --lib`, and `cargo clippy --all-targets -- -D warnings` pass with `CARGO_INCREMENTAL=0` (`cargo test --lib`: `293 passed; 0 failed; 0 ignored`). Full `cargo test` reaches the integration suite and then fails in the existing non-scrolling test `tests/file_service_tests.rs::preserves_encoding_when_round_tripping_windows_1252` (`left: [99, 97, 102, 239, 191, 189, 33]`, `right: [99, 97, 102, 233, 33]`).
- 2026-04-29: Follow-up review found four additional gaps after this plan's completion note. See `docs/scrolling-visible-window-outstanding-gap-plan.md`. The fractional row, layout-aware live-intent, and horizontal edge-autoscroll fixes have landed; the remaining full-document galley dependency is now tracked as a larger bounded display-map migration.

## Completion Status

Implementation complete for this plan. Follow-up review work recorded on 2026-04-29 is also now closed for the normal editor path: display-row discovery uses the piece-tree-backed `DisplayMap`, normal paint lays out only the overscanned viewport text, and the full-galley display-snapshot adapter has been removed. `DisplayMapCache` is now part of that single normal build path, reusing exact-revision maps and unchanged per-line layouts without adding a separate long-file/long-line renderer. The standalone read-only text edit path was removed so derived artifact/control-character display no longer bypasses the normal editor renderer.
