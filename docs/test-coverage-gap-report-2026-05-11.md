# Test Coverage Gap Report — 2026-05-11

Survey of test coverage across `src/` and `tests/`. Source code only; no design docs were consulted.

## Summary

| Metric | Count |
| --- | --- |
| Source files (excl. `bin/`, `scripts/`) | 208 |
| Source files containing `#[test]` | 40 (~19%) |
| Inline `#[test]` functions | 225 |
| Integration tests in `tests/*.rs` | 31 |
| Integration test files | 6 |
| Total tests | ~256 |

Inline tests are concentrated in: `domain/buffer/*` (piece tree, document, history), `ui/editor_content/native_editor/*` (cursor, layout, editing, word_boundary), `ui/dialogs/text_history`, `ui/scrolling`, `services/search`, `services/file_service`, and `app_state/search_state`. Integration tests target file I/O, settings persistence, session restore, search workflow, startup arg parsing, and tab/pane operations.

## Coverage by Top-Level Subsystem

| Subsystem | Files | Files w/ tests | Notes |
| --- | --- | --- | --- |
| `domain/buffer` | 16 | 6 | Strong coverage on the piece tree and document layers; history layer is large and untested. |
| `domain/{panes, tab, view}` | 13 | 0 | No tests in source; only `tests/tab_tests.rs` exercises split/combine/rebalance at the workspace level. |
| `services/file_controller` | 7 | 0 | Save/Open/Rename/Open-Here orchestration is unverified. |
| `services/file_service` | 2 | 1 | Read/write covered by `services/file_service.rs` + `tests/file_service_tests.rs`. |
| `services/background_io` | 4 | 0 | Worker, dispatcher, and types untested. |
| `services/session_store` | 4 | 1 | Restore path has 3 inline tests + `tests/session_store_tests.rs`. Ops/model/dispatcher untested. |
| `services/settings_store` | 1 | 1 | One inline test + `tests/settings_store_tests.rs`. |
| `services/search` | 4 | 1 | `services/search.rs` has 8 tests; ASCII and word-boundary matchers have no direct unit tests. |
| `app_state/search_state` | 9 | 3 | API surface tested via `api_tests.rs`; `replace.rs`, `runtime.rs`, `worker.rs`, `fragments.rs`, `visual.rs` untested. |
| `app_state/settings_state` | 7 | 4 | Mutators/window/tab_order/display_tabs covered; `toml_refresh`, `history_budget`, top-level untested. |
| `app_state/workspace` | 6 | 2 | `mutation.rs` + `restore_conflict.rs` partial; `lifecycle.rs`, `accessors.rs`, `editing.rs` untested. |
| `commands` | 2 | 0 | Dispatch and tab-transfer have no tests. |
| `chrome` | 4 | 0 | Window chrome / tabs (~427 LOC in `tabs.rs`) untested. |
| `ui/editor_content/native_editor` | 11 | 8 | Strong coverage. `highlighting.rs` (333 LOC) and `interactions/mod` are untested. |
| `ui/editor_area` | 8 | 4 | `tile.rs`, `tile/chrome`, `tile/context_menu` untested. |
| `ui/scrolling` | 12 | 3 | Manager, acceleration covered; area/scrollbar/state/intent/source/target/anchor/metrics/display untested. |
| `ui/search_replace` | 6 | 1 | Only `state.rs` has tests; controls and results rendering untested. |
| `ui/dialogs` | 8 | 2 | Text-history covered; `pending`, `restore_conflict`, `encoding`, `common`, `status_history` (3 tests) thin or absent. |
| `ui/settings` | 7 | 2 | `settings.rs` + `widgets.rs` covered (`widgets.rs` test count is 0 by `#[test]` but the file is 1014 LOC; verify). Appearance, opening, sections, style, text_formatting untested. |
| `ui/tab_strip` + `tab_drag` | ~16 | 1 | Only `entries/shared.rs`. Rendering, drag state, autoscroll, drop-target untested. |
| `ui/tile_header` | 6 | 0 | Split drag/preview/geometry untested. |
| `startup` | 2 | 0 inline | Behavior covered indirectly by `tests/startup_tests.rs` (8 tests); `parser.rs` (287 LOC) has no inline unit tests. |
| Top-level (`text_history`, `memory_budget`, `capacity_metrics`, `color_contrast`, `diagnostics`, `shortcuts`, `fonts`, `theme`, `utils`) | 9 | 4 | Diagnostics (10 tests), color_contrast (2), shortcuts (2), `app/mod.rs` `paths_match` not directly tested. `text_history`, `memory_budget`, `capacity_metrics`, `fonts`, `theme`, `utils` untested. |

## High-Value Gaps (Critical, Untested)

Ranked by combined risk × size. Each is a place where regressions would corrupt user data, lose work, or silently degrade behavior.

### 1. `services/file_controller/save.rs` (705 LOC) — **Tier 1**
Orchestrates save, save-as, save with encoding fallback, and the pending-reload pipeline. Touches disk, the snapshot pipeline, and conflict resolution. Zero tests. A bug here loses user data.
Suggested: round-trip save → reload, save with encoding-failure path, save-while-disk-changed conflict, save-as new path.

### 2. `services/file_controller/open.rs` (310 LOC) and `open_here.rs` (378 LOC) — **Tier 1**
Untested. Open flows decide whether to reuse an existing tab, replace the active tab, or open in a split. Wrong decisions move tabs unexpectedly.

### 3. `domain/buffer/history.rs` (632 LOC) and `history/coalescing.rs` (200 LOC), `history/budget.rs` (52 LOC) — **Tier 1**
`PieceProvenanceStore`, undo/redo entry coalescing, budget enforcement. The companion `coalescing.rs` exposes `entry_sealed_by_divider`, `coalesced_local_edit_record`, etc. — pure-function coalescing is high-leverage to unit-test. Feedback memory notes coalescing breaks on `,;:.` + newline, exactly the kind of invariant a regression test should pin down.

### 4. `domain/buffer/document/history_ops.rs` (496 LOC) — **Tier 1**
Undo/redo application against the document. Companion `document/tests.rs` has 28 tests but check whether `history_ops` paths are exercised — name suggests document.rs proper, not the history-ops file specifically.

### 5. `services/background_io/worker.rs` (305 LOC) and `dispatcher.rs` (155 LOC) — **Tier 2**
Threaded I/O. Untested. Failure modes (worker panic, queue overflow, cancelled requests) are exactly what unit tests catch.

### 6. `services/session_store/ops.rs` and `model.rs` — **Tier 2**
Only the `restore.rs` slice has inline tests (3). Persistence of split layouts and dirty-buffer round-trips are exercised by `tests/session_store_tests.rs` (4 tests), but ops-level invariants (write atomicity, schema migration) are not.

### 7. `app_state/background_io.rs` (604 LOC) — **Tier 2**
Largest untested file in `app_state`. Coordinates background I/O completion → state mutation. No tests.

### 8. `domain/view.rs` (581 LOC) and `domain/view/{anchors, layout_cache}.rs` — **Tier 2**
View geometry, anchor stability across edits, layout caching. No tests; bugs surface as scroll jumps and cursor drift.

### 9. `domain/tab/{layout, promotion, repair}.rs` (~644 LOC combined) and `domain/panes/split.rs` (134 LOC) — **Tier 2**
Pane tree manipulation. `tests/tab_tests.rs` covers some happy paths; rebalancing edge cases (degenerate splits, repair after corrupted persisted state) are not covered.

### 10. `app_state/search_state/{replace, runtime, worker, worker/processing, fragments, visual}.rs` — **Tier 2**
Replace pipeline and search worker. `tests/search_workflow_tests.rs` covers one happy path for each of count/replace; the rich state machine (cancellation, scope changes mid-search, replace-all undo grouping) is unverified.

### 11. `commands/dispatch.rs` and `commands/tab_transfer.rs` (307 LOC) — **Tier 2**
Command dispatch and the tab-transfer command (move tab into split, transfer between workspaces). Untested.

### 12. `services/search/matchers/{ascii, word_boundary}.rs` — **Tier 2**
Pure functions on bytes. ASCII matcher (119 LOC) and word boundary detection (70 LOC). Easy to unit-test with table-driven cases; currently zero direct tests.

### 13. `startup/parser.rs` (287 LOC) — **Tier 3**
Argument parsing logic. `tests/startup_tests.rs` covers the parser end-to-end well (8 tests). Inline unit tests would be nice but the integration tests probably suffice.

### 14. `text_history.rs` (112 LOC) — **Tier 3**
Builds the `TextHistoryEntryView` list shown in the dialog. Pure transformation; trivial to test. Per memory note, the history UI has specific Now-line and highlighting conventions worth pinning.

### 15. `memory_budget.rs` (150 LOC), `capacity_metrics.rs` (275 LOC) — **Tier 3**
Atomic counters and budget enforcement. Pure logic, untested. Wrong arithmetic here silently misreports capacity.

### 16. UI rendering files (`ui/dialogs/encoding.rs`, `ui/search_replace/results.rs`, `ui/scrolling/area.rs`, `ui/tab_strip/context_menu.rs`, `ui/editor_area/tile.rs`, `chrome/tabs.rs`) — **Tier 3 (lower priority)**
Several 400–540 LOC rendering files have no tests. Egui rendering is hard to unit-test, but pure helpers extracted from these (geometry, hit-testing, label formatting) are good candidates.

## Notable Coverage Strengths

- **Piece tree / document / editing**: ~75 inline tests across editor_content + buffer modules; the editing engine is the best-tested area.
- **Settings & session persistence**: integration tests cover round-trip, malformed input, ignored legacy YAML, and dirty-buffer conflicts.
- **Startup arg parsing**: 8 integration tests cover the user-facing CLI surface.
- **Search dialog state**: `app_state/search_state/api_tests.rs` (7) plus `services/search.rs` (8) plus the workflow integration tests give end-to-end confidence on the count path.

## Recommendations (in priority order)

1. **Unit-test `domain/buffer/history/coalescing.rs`** — pure functions, high-leverage, anchored by an existing invariant (`,;:.` + newline divider rule).
2. **Add a `file_controller_tests.rs` integration suite** covering save, save-as, save-with-encoding-fallback, and save-during-disk-conflict against a temp dir.
3. **Add inline tests for `services/background_io/worker.rs`** — request lifecycle, cancellation, panic recovery.
4. **Add unit tests for `services/search/matchers/{ascii,word_boundary}.rs`** — pure byte-level table-driven tests; the cheapest high-confidence win in the codebase.
5. **Cover `app_state/search_state/replace.rs`** — replace-all crossing buffer boundaries, undo grouping, cancellation.
6. **Add tests for `domain/view/anchors.rs`** — anchor stability under inserts/deletes around the anchor (most likely place for cursor-drift regressions).
7. **Backfill `commands/tab_transfer.rs`** — moving tabs between workspaces and into splits has rich edge cases (drop on self, drop on empty pane).
8. **Pin invariants in `domain/panes/split.rs`** — `MIN_SPLIT_RATIO`/`MAX_SPLIT_RATIO` clamping, no degenerate trees after repeated splits/closes.
9. Extract pure helpers from the larger `ui/` rendering files (geometry, hit-testing) and unit-test those, leaving egui paint code uncovered.

## Methodology

- Enumerated `*.rs` under `src/` (excluding `bin/`, `scripts/`).
- Counted `#[test]` occurrences per file.
- Grouped files by subsystem and cross-referenced inline tests against companion `tests.rs` modules and the 6 files in `tests/`.
- Ranked gaps by file size × user-facing risk (data loss > silent corruption > UX regression > cosmetic).
- Reviewed the contents of selected high-risk files to confirm they contain executable logic (not just type definitions or trivial accessors).

No external coverage tool (`cargo tarpaulin`, `cargo llvm-cov`) was run; counts are by `#[test]` attribute presence, not branch/line coverage. Running an actual coverage tool would refine tier rankings — particularly to confirm that integration tests reach the `file_controller` and `background_io` layers, which appear to have only indirect exposure.
