# Anchor Follow-Up Execution Plan

Date: 2026-04-29

## Purpose

Finish the anchor work by validating performance, closing lifecycle coverage gaps, and making a clear decision about global search-result anchoring. The core anchor substrate is already implemented; this plan is about confidence and boundaries.

## Current Baseline

- Point anchors are stored on piece-tree leaves and tracked by `AnchorId` metadata.
- View scroll, cursor, selection endpoint, and view-local search highlight endpoints use owner-aware anchors.
- `DocumentSnapshot` strips live anchors.
- Live render paths use piece-backed scroll anchors once a buffer and display snapshot are available.
- Focused anchor, search, native editor, and tile suites pass.

## Decision

Keep global worker-owned search matches snapshot/range based for now. View-local search highlights are already anchored, which protects the rendered UI without turning every search result into a long-lived live anchor. Revisit this only if product behavior requires search results to survive arbitrary edits without worker refresh.

## Phase 1: Benchmark Baseline

Status: completed for the first anchor-storage baseline. Results are recorded in `docs/anchor-storage-baseline-2026-04-29.md`, with raw output under `target/analysis/anchor_storage_baseline_2026-04-29.txt`.

Goal: prove the leaf-backed anchor store does not regress into full-anchor scan behavior.

1. Run `cargo bench --bench anchor_storage`.
2. Capture insert/remove timings for 1, 10, 100, 1,000, and 10,000 live anchors.
3. Add a short results note to `docs/measurement-tools.md` or a dedicated benchmark report.
4. If timings scale unexpectedly with total anchor count for edits outside affected leaves, inspect `redistribute_anchors_into_leaves` and leaf lookup paths before changing higher-level editor code.

Exit criteria: benchmark results are recorded and no obvious accidental global-scan pattern appears.

## Phase 2: Combined Lifecycle Leak Test

Goal: prove every runtime owner releases cleanly through the same lifecycle boundaries.

Add a focused test that creates all current runtime anchor owner types on one view:

- view scroll anchor
- cursor endpoint anchor
- selection endpoint anchor
- pending cursor endpoint anchor
- search highlight start/end anchors

Then assert `live_anchor_count() == 0` after each relevant lifecycle path:

1. `clear_transient_view_state`
2. `clear_view_state_for_buffer_replacement`
3. `close_view` in a split tab

Exit criteria: each path releases every owner type without leaving anchor buckets in the piece tree.

## Phase 3: Raw Range Assignment Cleanup

Goal: reduce future drift between raw public range mirrors and anchor-backed state.

1. Find direct assignments to `cursor_range`, `pending_cursor_range`, and `search_highlights`.
2. Convert call sites that have mutable buffer access to:
   - `set_cursor_range_anchored`
   - `set_pending_cursor_range_anchored`
   - `set_search_highlights_anchored`
3. Leave direct assignments only in tests, read-only views, or call sites without buffer access.
4. Add comments or helper methods for intentional raw mirror writes if needed.

Exit criteria: live mutation paths prefer anchored setters whenever they can access the owning buffer.

## Phase 4: Full Verification Pass

Goal: validate the anchor substrate against the broader editor surface.

Run:

```powershell
cargo test anchor --lib
cargo test piece_anchor --lib
cargo test app::domain::view::tests --lib
cargo test app::app_state::search_state::tests --lib
cargo test app::ui::editor_content::native_editor::tests --lib
cargo test app::ui::editor_area::tile::tests --lib
cargo test snapshot_does_not_affect_live_anchor_after_undo_redo --lib
cargo test replacement_with_undo_and_redo_tracks_live_anchor --lib
cargo test unicode_replacement_tracks_anchor_by_char_offset --lib
```

If this remains stable, run broader CI or `cargo test --lib` before considering the anchor work complete.

## Deferred Work

- Do not migrate the global `SearchMatch` list to live anchors unless refresh-based matching proves insufficient.
- Do not add range/interval anchor infrastructure for diagnostics or huge search result sets until a concrete workload needs it.
- Do not make anchors persistent across session restore; runtime anchors should continue to be rebuilt from view state and display snapshots.