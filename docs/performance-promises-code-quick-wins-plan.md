# Performance Promises Code Quick Wins Plan

Date: 2026-05-12

## Scope

This plan intentionally ignores existing planning documents. It is based on the current project code and the measurement interface only:

- Promise definitions and evidence mapping in `scripts/performance_review.py`.
- Measurement generators in `scripts/capacity_report.py`, `scripts/resource_profiles.py`, `scripts/measurement_catalog.py`, `src/bin/capacity_probe.rs`, and `src/bin/resource_probe.rs`.
- Promise-board rendering in `viewer/data-viewer.js`.
- Runtime/editor code paths that execute the promises.

## The Seven Promises

1. **Large Files**: Load, inspect, scroll, and edit very large text files quickly.
2. **Many Files**: Keep workspace and file workflows responsive above 10,000 files.
3. **Search**: Return first matches quickly and finish searches over huge files and many files.
4. **Many Tabs**: Open, switch, reorder, and manipulate huge tab sets quickly.
5. **Many Views**: Keep many views into the same loaded files responsive.
6. **Large Text Mutation**: Paste, cut, undo, redo, and metadata refresh should stay fast on huge buffers.
7. **Session Persistence Restore**: Persist and restore very large workspaces without startup stalls.

## Ranking Rules

The quickest wins are changes that:

- Remove repeated O(n) or O(n^2) work in hot UI/session/search paths.
- Fit the current architecture without a storage rewrite.
- Can be measured by the existing promise board with a small probe addition.
- Improve more than one promise when possible.

## Ranked Quick Wins

| Rank | Promises | Current Code Finding | Quick Win | Measurement Proof |
| --- | --- | --- | --- | --- |
| 1 | Many Tabs, Session Restore | `src/app/ui/tab_strip/entries/shared.rs:38` collects every slot, while `horizontal.rs:102`, `vertical.rs:121`, and `src/app/ui/tab_overflow.rs:326`/`:332` render or count across huge tab sets. | Virtualize tab strip and overflow rows. Render visible slots plus small overscan using fixed `TAB_BUTTON_WIDTH`/`TAB_HEIGHT`; synthesize drop targets from slot geometry. | Add `tab_strip_frame_cost` and `tab_overflow_frame_cost` probes. Map them to `many_tabs` and `session_restore` in `scripts/performance_review.py`. |
| 2 | Many Tabs, Large Text Mutation, Large Files | `src/app/ui/tab_strip/entries.rs:14` recomputes duplicate tab-name counts during rendering; `src/app/app_state/workspace/mutation.rs:84` reapplies tab ordering after edits; `src/app/app_state/settings_state/tab_order.rs:206` can flatten buffer text to compute file size. | Cache duplicate-name counts in `TabManager`; only reapply tab ordering after edits for order modes affected by edits; replace `buffer.text().len()` fallback with stored/piece-tree byte metrics. | Add `edit_finalize_cost_with_10k_tabs`; watch existing `tab_count_resource_tracking`, `paste_size_ceiling`, and `file_backed_open_first_visible_paint`. |
| 3 | Session Restore, Many Tabs, Many Files | `src/app/app_state/background_io.rs:216` applies restored tabs one at a time and calls `rebuild_buffer_tab_index`, `refresh_startup_restore_conflicts`, and `mark_search_dirty` per tab at `:236-240`. | Batch streamed restore results, update the buffer-tab index incrementally, and defer conflict/search refresh until the final restore event. | Add `startup_stream_apply_cost` with 100, 1,000, and 10,000 restored tabs; map to `session_restore`, `many_tabs`, and `many_files`. |
| 4 | Session Restore, Many Tabs | `src/app/services/session_store/mod.rs:164` persists every captured buffer snapshot, and `:180` writes each temp snapshot even when only one tab changed. | Persist the manifest every time, but write a buffer snapshot only when its temp id or document revision changed. Preserve unchanged snapshot files during cleanup. | Add `session_dirty_persist_delta_cost` for one edited tab in a 10,000-tab session; compare with existing `session_persist_cost`. |
| 5 | Search, Many Views, Many Files | `src/app/app_state/search_state/helpers.rs:177` collects targets by view and `:288` builds a snapshot before deduping duplicate views of the same buffer. | Dedupe by buffer id/file identity before building `DocumentSnapshot`; create one snapshot and one label set per searchable buffer. | Add `search_target_collection_cost` with many views over one buffer and many unique buffers; map to `search` and `many_views`. |
| 6 | Search, Large Text Mutation | `src/app/app_state/search_state/helpers.rs:17-30` extracts `matched_text` for every match immediately; `:72` clones matches/groups for partial snapshots; `worker/processing.rs:266` can publish every 64 matches. | Store match text lazily for plain searches and extract only for replace validation/regex replacement. Publish progress-only or coalesced partials after the first visible result. | Add `search_match_materialization_cost` and track heap/latency beside existing search first-response benchmarks. |
| 7 | Large Files, Session Restore | `src/app/services/file_service.rs:382` streams decoded file chunks, then `:432` appends each chunk through `document.insert_direct`. | Add a bulk document builder for decoded chunks, or a dedicated append fast path in `PieceTreeLite`, so large-file load builds the tree once instead of rebalancing per chunk. | Use existing `file_backed_open_first_visible_paint` and `file_backed_open_allocation`; add chunk count and insert count to resource rows. |
| 8 | Large Text Mutation, Large Files | `src/app/domain/buffer/piece_tree/edit.rs:305` replaces leaf spans, `:329` rebalances the node window, and `:334`/`:361` can recalculate the root. | Localize metric recalculation for append and leaf-local edits. Start with append/delete-at-end fast paths before broader tree surgery. | Watch `paste_size_ceiling`, `paste_allocation`, and add `piece_tree_append_cost`. |
| 9 | Many Views | `src/app/ui/editor_area/mod.rs:230` recurses through every pane node and `:316` clones split paths; `src/app/ui/editor_area/tile.rs:142` renders editor bodies even when a tile is too small to be useful. | Add a tiny-tile guard that paints only a lightweight tile shell/title below a minimum body size. Replace cloned split paths with reusable/mutable path state. | Add `many_view_frame_cost` covering 128, 512, and 1,000 views; map to `many_views`. |

## Promise-by-Promise Plan

### 1. Large Files

First target: file load and append-style text construction.

- Implement `TextDocument::from_decoded_chunks` or `PieceTreeLite::from_spans` for file reads in `src/app/services/file_service.rs`.
- Add an append fast path in `src/app/domain/buffer/piece_tree/edit.rs` if the full bulk builder is too large for the first pass.
- Remove `buffer.text().len()` from tab-order file-size fallback so large buffers are not flattened just to sort tabs.
- Measure with `file_backed_open_first_visible_paint`, `file_backed_open_allocation`, `file_size_ceiling`, and a new chunk/insert counter.

### 2. Many Files

First target: avoid doing per-file or per-tab cleanup repeatedly during restore and search setup.

- Batch startup restore application and defer expensive global refreshes until restore completion.
- Dedupe search targets before snapshot creation.
- Add `startup_stream_apply_cost` and `search_target_collection_cost` so the promise board can distinguish workspace-scale overhead from text-search cost.

### 3. Search

First target: first response and memory pressure.

- Dedupe targets before snapshots in `collect_search_targets_for_views`.
- Avoid eager `matched_text` extraction for result sets that do not immediately need replacement text.
- Replace cloned partial result payloads with progress-only partials after the first visible result, or coalesce by elapsed time and byte size instead of every 64 matches.
- Measure first response, completion, allocation, and match materialization separately.

### 4. Many Tabs

First target: rendering and edit-time metadata work.

- Virtualize horizontal/vertical tab strips and overflow popup rows.
- Cache duplicate-name counts on tab mutations instead of render.
- Make tab ordering mode-aware after edits: `FileName` and `FileAge` should not reorder because text changed; `FileSize` should use metrics, not full text flattening.
- Measure tab rendering directly, not just tab data-structure construction.

### 5. Many Views

First target: duplicate snapshots and tiny panes.

- Search should treat many views into the same buffer as one searchable target.
- Rendering should skip editor body work for panes too small to show useful text.
- Reduce split-path allocation while recursing pane trees.
- Measure active-frame cost for many views, not only the capacity to create split metadata.

### 6. Large Text Mutation

First target: work accidentally coupled to edits.

- Stop broad tab ordering work from running on every text mutation unless the active order mode needs it.
- Replace full-buffer file-size fallback with stored metrics.
- Add append/delete-at-end piece-tree fast paths before deeper piece-tree recalculation work.
- Move search match text materialization closer to replacement execution so large match sets do not penalize ordinary search.

### 7. Session Persistence Restore

First target: incremental persistence and startup-visible restore.

- Persist unchanged buffer snapshots by reference instead of rewriting them.
- Batch streamed restore tab application and update indexes incrementally.
- Defer conflict scanning and search invalidation to the end of restore.
- Keep `startup_visible_restore_cost` as the user-facing metric, then add lower-level restore-apply and dirty-persist delta probes to explain why it moved.

## Measurement Interface Updates

The viewer already has the right top-level shape: `viewer/data-viewer.js` renders the promise board at `renderPerformancePromiseBoard`, the selected promise detail at `renderPerformancePromiseDetail`, and evidence sections from the scenario payload. The fastest UI improvement is to feed it better scenario rows rather than redesign it.

Recommended measurement changes:

1. Add new probe rows in `src/bin/resource_probe.rs` and `src/bin/capacity_probe.rs`:
   - `tab_strip_frame_cost`
   - `tab_overflow_frame_cost`
   - `startup_stream_apply_cost`
   - `session_dirty_persist_delta_cost`
   - `search_target_collection_cost`
   - `search_match_materialization_cost`
   - `many_view_frame_cost`
   - `piece_tree_append_cost`
2. Register synthetic/profile rows in `scripts/resource_profiles.py` and capacity rows in `scripts/capacity_report.py` only where they represent true scale ceilings.
3. Add the new scenario names to the relevant `resource_scenarios`, `capacity_scenarios`, or `benchmark_keys` in `scripts/performance_review.py`.
4. In `viewer/data-viewer.js`, add a small "Delta Watch" row inside each promise detail only if the payload includes run-history values for the same scenario. The current `state.runs` and promise evidence matching functions are enough to drive this without changing the promise-board model.

## Implementation Order

### Wave 1: Cheap Decoupling and Better Proof

1. Replace `buffer.text().len()` in tab ordering with a metric-based size lookup.
2. Make post-edit tab reordering conditional on the active ordering mode.
3. Dedupe search targets before snapshot creation.
4. Add the missing measurement rows for tab frame cost, search target collection, edit finalize cost, and restore apply cost.

Expected outcome: immediate improvement in large mutation, search setup, many views, and many tabs, with clearer promise-board evidence.

### Wave 2: Visible UI Scale Wins

1. Virtualize tab strip rendering.
2. Virtualize tab overflow rows.
3. Add the tiny-tile guard for many-view layouts.
4. Cache duplicate-name counts.

Expected outcome: many-tab and many-view frame costs should move from proportional-to-total to proportional-to-visible.

### Wave 3: Session Scale Wins

1. Batch restored tab application.
2. Incrementally update `buffer_tab_index`.
3. Defer restore conflict refresh and search invalidation until restore completion.
4. Skip unchanged snapshot writes during session persistence.

Expected outcome: startup-visible restore should avoid per-tab global work, and saving a large session after one edit should be close to one-buffer cost.

### Wave 4: Text Storage Hot Paths

1. Add bulk document construction for file-backed loads.
2. Add append/delete-at-end piece-tree fast paths.
3. Localize piece-tree metric recalculation for common local edits.
4. Make search matched-text materialization lazy.

Expected outcome: large-file open, paste, append, undo/redo, and large search result sets should spend less time rebuilding or copying text.

## Guardrails

- Do not start with a full text-storage rewrite. The promise board can move meaningfully with render virtualization, deduping, batching, and metric-based shortcuts first.
- Keep each quick win behind a measurement row that appears in the promise detail page.
- For UI virtualization, preserve tab drag/reorder behavior before chasing additional rendering wins.
- For session persistence, verify stale snapshot cleanup carefully so skipped rewrites do not delete still-referenced temp files.
