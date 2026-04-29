# Stable Piece-Tree Anchors Plan

Date: 2026-04-29

## Purpose

Make piece-tree anchors a production-grade substrate for editor viewport stability. The editor should preserve the visible top-of-viewport content across edits above the viewport, split-pane redraws, wrap changes, undo/redo, and document snapshots without relying on logical-line fallback behavior in live render paths.

## Current State

- `PieceTreeLite` already exposes `create_anchor`, `release_anchor`, `anchor_position`, and `anchor_bias`.
- `AnchorRegistry` now stores direct `AnchorId` metadata while point anchors live on piece-tree leaves. Insert/remove operations redistribute anchors only for rebuilt leaves; anchors in untouched leaves move through piece-tree prefix metrics.
- `ScrollAnchor::Piece { anchor, display_row_offset }` already exists and can resolve through `display_aware_anchor_to_row` using `DisplaySnapshot` plus `PieceTreeLite::anchor_position`.
- `EditorViewState::upgrade_scroll_anchor_to_piece` can convert a logical scroll anchor into a piece-tree anchor at the current top display row and releases its previous anchor to avoid unbounded registry growth.
- Editor-level viewport stability tests now cover insert/delete above the viewport, wrapped rows, split views with different wrap widths, near-EOF deletes, and the current top-of-viewport `AnchorBias::Left` insertion behavior.
- Document-path anchor tests now cover replacement, undo/redo replay, and multi-byte Unicode char-offset behavior.
- Snapshot lifecycle tests now prove snapshots and snapshot clones strip live anchors while live anchors continue resolving correctly after undo/redo.
- Cursor, selection, and view-local search highlight endpoints now use owner-aware piece-tree anchors while retaining raw range mirrors for existing UI call sites.
- The remaining risk is now around benchmark measurement and deciding whether global search-result records should become long-lived live anchors, not the old linear-registry storage path.

## Goals

- One stable anchor per live editor view's vertical scroll position, owned by the view and released when replaced or discarded.
- No live render path should depend on logical-line anchors once a buffer and display snapshot exist.
- Anchor updates must be correct for inserts, deletes, replacements, undo/redo, cloned snapshots, and edits at the anchor boundary with explicit bias semantics.
- Anchor operations must remain cheap enough for large files and repeated edits.
- The anchor API boundary should accommodate multiple owner types, not only viewport scroll state.
- Test coverage should prove stability from both the piece-tree layer and the editor viewport layer.

## Non-Goals

- Do not introduce a second text storage structure.
- Do not make anchors persistent across process restarts; they are runtime view state.
- Do not solve animated scrolling or scroll easing here.
- Do not require exact pixel identity across font or wrap-width changes; preserve the anchored content and fractional display-row offset as closely as the display snapshot permits.

## Locked Decisions

- **Viewport anchor bias**: use `AnchorBias::Left` for top-of-viewport anchors. Current code semantics are that an equal-position insert does not shift a left-biased anchor, so text inserted exactly at the viewport's top-left coordinate becomes the new top row. If the intended product behavior changes to keep the preexisting row topmost for equal-position inserts, the viewport anchor bias or equal-position handling must change with an explicit regression test update.
- **Snapshots strip live-view anchors**: `DocumentSnapshot`, undo history, and background snapshots must not carry transient UI anchors. Live-view anchors are runtime UI state; copying them into snapshots risks memory bloat and stale anchor retention.
- **API scope**: design the anchor API around multiple owner classes. One scroll anchor per view solves the immediate viewport problem, but cursors, active selections, multi-selections, and search ranges should be able to reuse the same infrastructure without a later rewrite.
- **Scalable anchor storage**: point anchors are backed by piece-tree leaf buckets plus direct `AnchorId` metadata. Do not reintroduce global offset scans as the production path.
- **Point anchors vs ranges**: viewport anchors, cursor positions, and selection endpoints are point anchors and should live in the piece-tree anchor substrate. Large transient range collections such as search results may still use a separate interval/range index if needed.

## Phase 1: Anchor Semantics Audit

Status: mostly implemented. Raw piece-tree tests cover point-anchor bias and movement semantics; document-path tests cover replacement, undo/redo, and Unicode char-offset behavior.

1. Document the expected semantics for `AnchorBias::Left` and `AnchorBias::Right` in code comments and tests:
   - insert before anchor shifts anchor forward
   - insert at left-biased anchor does not shift
   - insert at right-biased anchor shifts forward
   - deletion before anchor shifts anchor backward
   - deletion containing anchor collapses to deletion start
   - deletion beginning exactly at anchor keeps anchor at start
2. Add replacement-style coverage using the actual editor edit path, not only raw `insert`/`remove_char_range`.
3. Add undo/redo coverage to prove anchor positions track operations replayed through `TextDocument` and `BufferState`.
4. Add multi-byte Unicode cases so char-offset anchoring does not accidentally regress to byte-offset behavior.

Exit criteria: complete for point anchors through raw piece-tree edits and document replacement/undo/redo paths. Continue expanding if cursor/selection/search owners introduce new semantics.

## Phase 2: Lifecycle Ownership

Status: implemented for view-owned scroll anchors, cursor endpoints, selection endpoints, and view-local search highlight endpoints. `AnchorOwner`/`AnchorOwnerKind` provide the owner metadata boundary, and runtime anchors are tagged with their owning view id.

1. Make view-owned anchor lifecycle explicit in `EditorViewState`:
   - store the current scroll anchor handle only through `ScrollAnchor::Piece`
   - store any auxiliary ownership state needed to release it safely
   - release the old handle before replacing it with a new handle
2. Introduce an owner-aware anchor boundary so scroll, cursor, selection, and search owners can eventually share the same substrate without changing `AnchorId` semantics.
3. Add a dedicated method for releasing a view's piece anchor when transient view state is cleared, a tab closes, a buffer is detached, or a view is duplicated into a different buffer context.
4. Audit all places that clear or replace view state, especially split/duplicate/close paths, to call that release path while the owning buffer is still available.
5. Add debug/test-only anchor count access at the piece-tree boundary so lifecycle tests can assert no leaked anchors after repeated scroll/render cycles.

Exit criteria: complete for scroll, cursor, selection, and view-local search highlight anchors across replacement, transient clear, and close paths. Continue expanding lifecycle tests if global search-result records become live anchors.

## Phase 3: Live Scroll Path Integration

Status: implemented for piece-backed viewport recovery, search activation, resolved scroll-offset bridge updates, and owner-aware cursor/search endpoint mirrors. Search match activation now queues a vertical `ScrollIntent::Reveal` from the latest `DisplaySnapshot` when geometry is available, with cursor reveal retained for horizontal correction and first-frame fallback. Wheel, drag, and scrollbar bridge offsets now seed a view-owned piece anchor directly when a buffer and display snapshot are available.

1. Ensure `upgrade_scroll_anchor_to_piece` runs after the first usable `DisplaySnapshot` and before any edit can rely on viewport stability.
2. Replace remaining live uses of buffer-less `editor_pixel_offset()` with buffer-aware resolution or make buffer-less calls explicitly fallback-only.
3. Verify every `ScrollIntent` drain path uses `display_aware_anchor_to_row` whenever a buffer and snapshot are available.
4. Ensure top-of-viewport anchors are created with `AnchorBias::Left` and add a regression test for insertion exactly at the top-left viewport coordinate.
5. Add a guard that detects an unresolved `ScrollAnchor::Piece` in the live render path and gracefully re-seeds from the current viewport instead of snapping to row zero.
6. Keep `ScrollAnchor::Logical` for tests and first-frame bootstrapping only; document that boundary in `scrolling/anchor.rs`.

Exit criteria: search reveal, cursor reveal, page navigation, selection edge autoscroll, and scroll-container bridge offsets now preserve piece-backed vertical anchors once a display snapshot is available. Explicit pixel offsets remain at the egui scroll-container boundary, but they no longer have to fall back to buffer-less logical vertical anchoring.

## Phase 4: Edit Stability Regression Tests

Status: implemented for the current viewport-anchor path in `src/app/ui/editor_area/tile.rs` tests.

Add editor-level tests that assert viewport stability, not just anchor offsets:

1. Scroll to a middle line, insert many lines above the viewport, and assert the same text remains at the top of the viewport.
2. Scroll to a middle line, delete lines above the viewport, and assert the same content remains visible.
3. Edit inside the anchored line and assert bias semantics are intentional.
4. Repeat the tests with word wrap enabled and a narrow viewport.
5. Repeat with two split views over the same buffer at different widths, proving each view owns an independent anchor and display-row mapping.
6. Repeat near EOF to ensure clamping and scroll-beyond-last-line behavior still work.

Exit criteria: complete for the currently wired viewport-anchor behavior. Failures in anchor update, display-row resolution, or view-local ownership show up as deterministic tests.

## Phase 5: Piece-Tree Anchor Storage

Status: implemented for point anchors. The live backing store is now integrated with the piece tree instead of a global linear offset registry.

1. Maintain the anchor store around stable `AnchorId` lookup plus piece-tree-local placement:
   - direct map from `AnchorId` to owner metadata, bias, and current node/local offset
   - anchor buckets on piece-tree leaves or nearby side tables keyed by node identity
   - aggregate anchor counts on internal nodes for diagnostics and future bulk operations
2. Keep insert/delete/split/merge/rebalance paths moving anchor buckets with affected text pieces instead of requiring a global scan.
3. Resolve anchor positions through existing piece-tree prefix metrics, combining leaf position, local offset, and bias semantics.
4. Keep the current public API shape (`create_anchor`, `release_anchor`, `anchor_position`, `anchor_bias`) stable.
5. Maintain capacity tests and benchmarks with 1, 10, 100, 1,000, and 10,000 live point anchors to catch accidental full-anchor scans.
6. Reserve interval/range indexes for non-point workloads such as large search highlight sets or diagnostics, not for viewport/cursor/selection endpoint anchors.

Exit criteria: complete for the point-anchor backing store. Continue using the benchmark to guard against regressions.

## Phase 6: Snapshot and Clone Boundaries

Status: implemented for `DocumentSnapshot` anchor stripping and live-document undo/redo isolation. General `PieceTreeLite::clone` still intentionally preserves anchors for isolated piece-tree behavior.

1. Strip live-view anchors from `DocumentSnapshot` clones by default.
2. Confirm `Arc::make_mut` behavior on `piece_tree_mut()` is acceptable when creating/releasing anchors on a shared tree.
3. Separate general `PieceTreeLite::clone` semantics from snapshot cloning if tests still need clone-preserved anchors for isolated piece-tree behavior.
4. Add tests proving snapshots, undo history, and background copies do not retain transient view anchors and do not affect live view anchor resolution after undo/redo.

Exit criteria: complete for `DocumentSnapshot` boundaries. Revisit if future background workers clone `TextDocument` directly instead of using `DocumentSnapshot`.

## Phase 7: Documentation Cleanup

1. Update `docs/scrolling-visible-window-rebuild-plan.md` to replace old “substrate gap” notes with current status.
2. Update `src/app/ui/scrolling/anchor.rs` comments so `Logical` is described as a bootstrapping/test fallback, not the v1 live model.
3. Add a short note to `docs/measurement-tools.md` if a new anchor benchmark/profile command is added.

Exit criteria: docs accurately describe the implemented anchor substrate and the remaining fallback boundaries.

## Suggested Implementation Order

1. Use the anchor storage benchmark to verify the augmented store and catch accidental full-anchor scans.
2. Decide whether global search-result records should become long-lived live anchors, or whether view-local anchored highlights plus worker refreshes are the right boundary.
3. Expand lifecycle/ownership tests if global search-result anchors are introduced.

## Remaining Questions

- Should global search-result records become live anchors, or should only view-local search highlight endpoints use anchors while the worker-owned match list remains snapshot/range based?

## Verification Checklist

- `cargo test anchor`
- `cargo test scroll_anchor`
- `cargo test piece_anchor --lib`
- `cargo test app::ui::editor_area::tile::tests --lib`
- `cargo test replacement_with_undo_and_redo_tracks_live_anchor --lib`
- `cargo test unicode_replacement_tracks_anchor_by_char_offset --lib`
- `cargo test snapshot_does_not_affect_live_anchor_after_undo_redo --lib`
- large-file anchor update benchmark or profile run
