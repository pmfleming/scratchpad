# Test Coverage Plan

Date: 2026-05-11

Basis: current project code outside `docs/**`, current git working tree, and deleted test files recovered from git history. Existing docs were intentionally not read.

## Source Evidence

- Current branch has 182 `#[test]` tests under `src/**` and no top-level `tests/` directory.
- Commit `a83f0bbdb46a876fe683952345bc0fe6ae09189b` deleted 260 tests across inline module tests and top-level integration tests.
- Commit `43738d72e65e6d4514239325cb4629c3a90c49fa` deleted 4 `tests/transaction_tests.rs` tests. Those are mostly superseded by the newer embedded text history direction, but the command-level undo expectations are still useful as workflow coverage.
- The heaviest current coverage is in:
  - `src/app/domain/buffer/document/tests.rs`: text history and edit coalescing.
  - `src/app/ui/dialogs/text_history/tests.rs`: text history presentation model.
  - `src/app/ui/editor_content/native_editor/layout.rs`: display slicing, control substitution, replacement preview projection.
  - `src/app/services/search.rs` and `src/app/app_state/search_state/api_tests.rs`: search program and a smaller app search API layer.
- The biggest deleted coverage areas were:
  - `src/app/app_state/search_state/tests.rs`: 35 app-level search, replace, text history, fragmented search, and worker ordering tests.
  - `tests/piece_tree_tests.rs` plus `src/app/domain/buffer/piece_tree/tests.rs`: 32 total piece-tree public API, anchor, Unicode, fragmentation, and randomized edit tests.
  - `src/app/commands/tests.rs`: 24 app command workflow tests.
  - `src/app/ui/editor_content/native_editor/tests.rs` and `src/app/ui/editor_area/tile/tests.rs`: 40 editor input, viewport, reveal, and scroll anchoring tests.
  - `tests/file_controller_tests.rs`, `tests/session_store_tests.rs`, `tests/settings_store_tests.rs`, `tests/file_service_tests.rs`, `tests/startup_tests.rs`, `tests/tab_tests.rs`: persistence, startup, file IO, and workspace integration tests.

## Placement Rule

Use inline or sibling module tests when the behavior depends on private functions, private state, exact helper contracts, or tight local invariants.

Use a sibling `tests.rs` file when a module has more than about five tests, shared fixtures, randomized cases, or a fixture setup that would make the source file harder to scan. Keep examples like `src/app/domain/buffer/document/tests.rs`, `src/app/app_state/search_state/api_tests.rs`, and `src/app/ui/scrolling/manager/tests.rs`.

Use top-level `tests/` integration tests when the behavior crosses public module boundaries: `ScratchpadApp`, command dispatch, session restore/persist, file controller flows, settings migration, startup parsing plus startup execution, or search/replace across multiple buffers/tabs. These tests should use only public crate APIs unless a seam is intentionally exposed.

Do not move tests purely to make the directory tree symmetrical. The deciding question is what contract is being protected:

- Private helper contract: inline `mod tests`.
- Private module with many cases: sibling `tests.rs`.
- Public app workflow or disk-backed behavior: top-level `tests/`.

## Deleted Tests Review

### Revive Directly Or Nearly Directly

These deleted tests still describe behavior that exists today and should return with small API adjustments:

- Piece tree public API and invariants from `tests/piece_tree_tests.rs`: anchor movement, anchor bias, anchor release, normalized ranges, Unicode coordinates, bounded extraction, line spans, range spans, and multibyte stress.
- Piece tree internal invariants from `src/app/domain/buffer/piece_tree/tests.rs`: empty-document anchor insert, prefix metric shifts, repeated inserts/removals, pack behavior, randomized string-model comparison, and large local edit history.
- File service read/write integration from `tests/file_service_tests.rs`: UTF-8, UTF-16LE, Shift_JIS, Windows-1252, binary rejection, CR-only and CRLF handling, explicit reopen encoding, and unencodable legacy save failure.
- Settings store integration from `tests/settings_store_tests.rs`: missing settings, TOML round trip, malformed TOML, legacy defaults, legacy font migration, tab list defaults, and YAML-to-TOML migration.
- Startup parser coverage from `tests/startup_tests.rs`: positional files, clean mode, add-to-active, one-based tab index parsing, comma lists, invalid switches, help, and version.
- Tab/workspace structural coverage from `tests/tab_tests.rs` and `tests/tab_manager_tests.rs`: split/close, resize, preview placement, repair missing views, raw text mode repairs, combine/rebalance, promote file, split files to tabs, and custom order.

### Revive As Updated Integration Coverage

These tests are relevant, but the implementation has changed enough that they should be rewritten around today's app APIs:

- `src/app/app_state/search_state/tests.rs`: keep the behavior list, but split it into:
  - fast app-search API tests beside `search_state`;
  - top-level integration tests for cross-buffer replace, tab navigation, dirty labels, duplicate buffers, search result grouping, and search worker ordering.
- `src/app/commands/tests.rs`: keep command workflow assertions, especially tab/view activation focus, dirty close prompts, settings-surface behavior, settings TOML application, tab combination, and promotion flows. Rewrite as `tests/app_command_tests.rs`.
- `tests/file_controller_tests.rs`: keep disk freshness, save conflict, startup target insertion, open-here batching, settings-file focus-loss behavior, and saved startup preference behavior. Rewrite as `tests/file_controller_tests.rs` using current session/settings constructors.
- `tests/session_store_tests.rs`: keep persist/restore, encoding metadata, control character mode, split views, active view, unique restored view ids, combined workspace tabs, newer disk reload, dirty conflict, and missing disk markers. Add text history persistence assertions because session now persists embedded history metadata.
- Deleted transaction tests from `tests/transaction_tests.rs`: do not revive the old transaction log model wholesale. Instead, add command-level tests that active-buffer undo/redo preserves the history UI state and restores editor selections.

### Mostly Superseded

These deleted tests are covered by newer local tests or should stay deleted unless a regression appears:

- Many old text-history coalescing expectations are now better covered in `src/app/domain/buffer/document/tests.rs`.
- Some old editor desired-size and EOF scroll-space checks are now covered in `native_editor/layout.rs` and `tile/scroll_frame.rs`.
- Some old search service tests are now covered by `src/app/services/search.rs`, especially bounded regex, capture replacement expansion, Unicode whole-word matching, ASCII case-insensitive search, and interruptible cancellation.

## Core Editor Input Coverage Gaps

The editor is now split across `native_editor/mod.rs`, `editing.rs`, `cursor.rs`, `word_boundary.rs`, `interactions/keyboard.rs`, `interactions/mouse.rs`, `layout.rs`, `painting.rs`, `editor_area/mod.rs`, and `editor_area/tile/*`. Current tests cover a few layout, cursor, mouse, IME event classification, and copy/cut cases, but they leave the main input pipeline thin.

High-priority gaps:

- `editing.rs` has no direct tests, even though it owns insert, paste source tagging, delete, backspace, wordwise delete/backspace, outdent, cut, line-ending normalization, deleted span capture, and operation record creation.
- `word_boundary.rs` has no tests, but both cursor movement and wordwise deletion depend on it.
- `interactions/keyboard.rs` tests only IME event classification. It does not test Enter, Tab, Shift+Tab, Backspace, Delete, wordwise modifiers, select-all, copy/cut/paste, Insert shortcuts, undo/redo shortcuts, or cursor updates.
- `native_editor/mod.rs` does not currently test `request_cursor_reveal_after_input`; deleted tests around same-line edit, newline edit, changed frame, stable frame, and drag suppression should be reintroduced.
- `interactions/mouse.rs` has only low-level secondary click and pointer tracking tests. It does not cover shift-click extension, double-click word selection, triple-click row selection, click-count reset, end-of-row click normalization, or display-map-adjusted cursor ranges.
- `cursor.rs` covers document-line movement, PageDown, and End. It does not cover selection collapse, Shift movement, wordwise movement, Home/Ctrl+Home/Ctrl+End, PageUp, display map conversion, or clamping across viewport slices.
- `layout.rs` has good control-character and replacement-preview projection tests, but the changed `preview_text_slice` mapping loop should gain edge cases for replacement ranges at the document end, adjacent replacement ranges, deletion, and replacements that cross visible-slice boundaries.
- `editor_area/tile/scroll_frame.rs` covers content size and EOF tail only. Deleted piece-anchor tests for preserving viewport content after insert/delete above the viewport, wrapped rows, near EOF, split views, unresolved anchors, and pending reveal suppression should be rebuilt.
- No current test drives the full app-level path from editor-like input to `BufferState`, active selection, search refresh, text history, and cursor reveal. Add a small app-level facade test around `ScratchpadApp::insert_text_in_active_view`, cut/delete/select-all, undo/redo, and active search match reselection.

## Coverage Gap Matrix

This pass compares current test distribution against behavior-heavy code paths. It is not line coverage; it is contract coverage.

| Area | Current coverage signal | Gap | Recommended home |
| --- | --- | --- | --- |
| Editor edit mechanics | `editing.rs` has no `#[test]`; deleted native editor tests covered only some public outcomes | Insert, paste source tagging, line-ending normalization, backspace/delete, wordwise delete, selection delete, cut, outdent, operation records | Inline tests in `src/app/ui/editor_content/native_editor/editing.rs` |
| Editor word boundaries | `word_boundary.rs` has no tests | Whitespace, punctuation, underscore, Unicode letters/numbers, clamp behavior, double-click selection boundaries | Inline tests in `src/app/ui/editor_content/native_editor/word_boundary.rs` |
| Keyboard input handling | Only IME preedit/commit event classification is tested | Enter, Tab, Shift+Tab, Backspace/Delete, word modifiers, select-all, copy/cut/paste, Insert shortcuts, undo/redo shortcut dispatch | Inline tests in `src/app/ui/editor_content/native_editor/interactions/keyboard.rs`; app facade cases in `tests/editor_workflow_tests.rs` |
| Cursor movement | Four current tests cover document-line movement and End | Selection collapse, Shift extension, word movement, Ctrl+Home/End, PageUp, visible-slice clamping, display-map conversion | Inline tests in `src/app/ui/editor_content/native_editor/cursor.rs` |
| Mouse selection | Four current tests cover secondary click and pointer tracking | Shift-click selection extension, double-click word selection, triple-click row selection, click count reset, row-end normalization, display-map adjusted cursor | Inline tests or sibling tests in `src/app/ui/editor_content/native_editor/interactions/mouse.rs` |
| Editor reveal and snapshot lifecycle | A few `native_editor/mod.rs` tests cover rebuild and copy/cut behavior | Same-line vs newline reveal mode, drag reveal suppression, stable frame reveal consumption, pending cursor sync, IME focus clearing | Inline tests in `src/app/ui/editor_content/native_editor/mod.rs` and `painting.rs` |
| Editor scroll anchoring | Current `scroll_frame.rs` tests cover content size and EOF tail only; deleted tile tests had broader anchor coverage | Preserve viewport after insert/delete above viewport, wrapped rows, near EOF, split-view independent anchors, unresolved anchors, reveal intent suppression | Sibling tests near `src/app/ui/editor_area/tile/scroll_frame.rs` |
| Display mapping and replacement previews | Good control-character coverage exists; current working diff touched `preview_text_slice` cursor mapping | Adjacent replacements, replacement at EOF, deletion at EOF, replacement crossing visible-slice boundaries, active match mapping under control substitutions | Inline tests in `src/app/ui/editor_content/native_editor/layout.rs` |
| App editor facade | `app_state/workspace/editing.rs` has no direct tests | `select_all`, copy/cut/delete/insert, undo/redo, active selection clearing, search refresh and match reselection after edits | `tests/editor_workflow_tests.rs` |
| Piece tree | Current tree has no visible `piece_tree/tests.rs`; deleted suite had 32 tests | Anchor bias, release, Unicode coordinates, fragmentation, line spans, bounded extraction, randomized string model, provenance spans, add-buffer compaction | Internal sibling tests plus public `tests/piece_tree_tests.rs` |
| Search workflow | Search service and small API tests exist; deleted app-level search suite had 35 tests | Grouping by tab/file, duplicate buffers, activating matches, focus file without selecting match, selection scope validation, invalid regex blocking replace, cross-buffer replace revalidation, fragmented worker ordering | `tests/search_workflow_tests.rs` plus focused `search_state` sibling tests |
| Search runtime and worker | `runtime.rs`, `visual.rs`, `replace.rs`, `worker/processing.rs`, and `fragments.rs` have little or no direct coverage | Dirty/fresh transitions, stale result rejection, partial result ordering, highlight clearing, active match preservation, bounded regex chunk behavior | Sibling tests in `src/app/app_state/search_state/*` |
| File controller | Deleted integration tests are gone; current file controller modules have no top-level coverage | Open/open-here batching, disk freshness, conflict save, reload/reopen encoding, startup target placement, settings file focus-loss refresh | `tests/file_controller_tests.rs` |
| Session store | Only restore helper tests remain | Full persist/load round trip, split views, active view, unique view IDs, combined tabs, missing/stale/conflict disk states, text history persistence | `tests/session_store_tests.rs` |
| Settings store and settings app state | Store has one default test; mutators/tab order have some local tests | TOML round trip, malformed TOML, ignored legacy YAML, older TOML defaults, settings tab persistence, settings TOML edit application | `tests/settings_store_tests.rs` and command workflow tests |
| Startup | Parser currently has no visible tests; deleted `tests/startup_tests.rs` had 9 | CLI switch parsing, comma-delimited files, invalid combinations, help/version, clean/add-to behavior | `tests/startup_tests.rs` |
| Command/tab workflows | Deleted command, tab, and tab manager tests are gone | Promote view/tab files, combine/rebalance tabs, dirty close prompts, tab reorder, view activation focus, split repair and raw text repairs | `tests/app_command_tests.rs`, `tests/tab_tests.rs` |
| Background IO | Deleted background IO tests are gone; dispatcher/worker modules have no visible tests | Lane saturation, request return on full lane, ordered parallel loads, streaming partial ordering, cancellation/generation handling | Sibling service tests and `tests/background_io_tests.rs` where disk/threading is involved |
| UI shell changes | Current working tree touches tab buttons, tab overflow, settings widgets, and tab cells | Option propagation, close/promote response routing, settings combo row change dispatch, tab attention color and truncation behavior | Small local unit tests for pure helpers; visual/manual smoke separately |

## Coverage Milestones

1. Close the editor-under-input gap first. A failing edit, cursor, or reveal contract will make higher-level workflow tests noisy.
2. Restore piece-tree invariants before broad workflow tests, because almost every editor, search, and history behavior assumes char/byte and anchor correctness.
3. Reintroduce top-level `tests/` gradually by workflow: editor, search, file/session/settings, then tab command workflows.
4. Add one regression test beside any module touched by the current working tree when that module has a pure helper seam. Current candidates are `layout.rs`, tab button option routing, settings row structs, and tab overflow row action routing.
5. Keep stress/randomized tests deterministic and bounded in the default suite; reserve long-running stress for ignored tests or profile binaries.

## Implementation Plan

### Phase 1: Fast Core Safety Net

Add tests that run without disk IO and without full egui rendering:

- `src/app/ui/editor_content/native_editor/editing.rs`
  - Add inline tests for insert replacing selections, paste source records, newline normalization to preferred line ending, simple backspace/delete, wordwise backspace/delete, deleting selected text, cutting selected text, Shift+Tab outdent, and no-op outdent.
- `src/app/ui/editor_content/native_editor/word_boundary.rs`
  - Add inline tests for whitespace, punctuation, underscores, Unicode alphanumeric text, start/end clamps, and left/right asymmetry.
- `src/app/ui/editor_content/native_editor/cursor.rs`
  - Add inline tests for selection collapse, Shift+Arrow extension, Home/End with CRLF, Ctrl+Home/Ctrl+End, wordwise left/right, PageUp, and display-map cursor conversion.
- `src/app/ui/editor_content/native_editor/mod.rs`
  - Add inline tests for cursor reveal decisions: same-line edit requests horizontal reveal, newline edit requests keep-visible reveal, cursor movement without edits requests keep-visible reveal, unchanged cursor does not request, drag suppression clears reveal.
- `src/app/ui/editor_content/native_editor/layout.rs`
  - Add preview projection tests for adjacent replacements, replacement at EOF, deletion at EOF, and replacement outside the current viewport slice.

Acceptance: `cargo test native_editor` should exercise these without touching the filesystem.

### Phase 2: Restore Piece Tree Confidence

Recreate the deleted piece-tree coverage in the current module layout:

- Add `src/app/domain/buffer/piece_tree/tests.rs` as a sibling module if not already present, or inline submodules in `piece_tree.rs` and child modules if private access is needed.
- Move public black-box cases to `tests/piece_tree_tests.rs` once the top-level `tests/` directory returns.
- Restore randomized edit/string-model tests with deterministic seeds and bounded iteration counts so the default suite stays quick.
- Add current-history-specific cases for provenance spans, add-buffer compaction, deleted span payload fallback, and anchor survival after copy-on-write snapshots.

Acceptance: piece-tree tests should catch char/byte coordinate drift, Unicode boundary errors, and anchor instability before editor tests fail.

### Phase 3: Rebuild Editor View And Scroll Regression Tests

Use the deleted `native_editor/tests.rs` and `editor_area/tile/tests.rs` as a checklist, not as code to paste:

- Restore viewport slice and local highlight clipping tests near `layout.rs`.
- Restore pending cursor range and IME output tests near `interactions.rs`, `painting.rs`, or `mod.rs` depending on private access.
- Restore piece-anchor scroll tests near `editor_area/tile/scroll_frame.rs` or a sibling `tests.rs` once fixture helpers exceed a few functions.
- Add split-view tests ensuring the same buffer can keep independent scroll anchors, cursor anchors, and wrap-dependent snapshots across multiple `EditorViewState`s.
- Add reveal intent tests that verify search result navigation and editor typing do not fight user scroll state.

Acceptance: editor scroll/reveal behavior should be testable without opening a native window.

### Phase 4: Restore Public Workflow Tests

Create top-level integration tests for behavior that crosses app/services/modules:

- `tests/app_command_tests.rs`
  - Restore command workflow tests from deleted `src/app/commands/tests.rs`: activate tab/view focus, promote view to tab, promote tab files to tabs, dirty close prompts, settings surface persistence, tab combining and balancing.
- `tests/search_workflow_tests.rs`
  - Restore deleted app-level search tests: grouping by tab/file, activating matches, duplicate buffers, selection-scope validation, invalid regex blocking replace, stale highlight clearing, cross-buffer replace revalidation, navigation clearing unrelated tab multi-selection, worker order, fragmented search, and bounded regex chunk search.
- `tests/file_controller_tests.rs`
  - Restore open-here, startup target insertion, file freshness, save conflict, settings TOML focus-loss application, and open preference behavior.
- `tests/session_store_tests.rs`
  - Restore persist/restore coverage and add text-history persistence checks.
- `tests/settings_store_tests.rs`
  - Restore TOML/YAML migration and backward-compatible default coverage.
- `tests/startup_tests.rs`
  - Restore CLI parse coverage.
- `tests/file_service_tests.rs`
  - Restore full encoding read/write integration and legacy encoding failures.

Acceptance: these tests should use temp directories and avoid shared global state. If any test needs wall-clock waiting, isolate it and keep the default path deterministic.

### Phase 5: Coverage Hygiene

- Add a lightweight script or cargo alias for fast tiers:
  - Core: document, piece tree, native editor, search service.
  - Workflow: app command, search workflow, file/session/settings/startup.
- Add a convention that new editor behavior must include at least one local unit test and, when it crosses `ScratchpadApp`, one workflow test.
- Keep randomized tests deterministic with named seeds printed on failure.
- Keep slow or stress-style tests behind `#[ignore]` only when they exceed the normal local feedback budget.

## Priority Order

1. Editor input local tests: `editing.rs`, `word_boundary.rs`, `cursor.rs`, `native_editor/mod.rs`.
2. Piece tree invariants and randomized string-model tests.
3. Editor scroll/reveal regression tests.
4. Search workflow tests, especially cross-buffer replace and fragmented worker behavior.
5. File/session/settings/startup integration tests.
6. Command workflow tests for tab/view/settings operations.

## First Pull Request Shape

Keep the first implementation small:

- Add `word_boundary.rs` tests.
- Add `editing.rs` tests for insert, paste, delete/backspace, wordwise delete/backspace, outdent, and line-ending normalization.
- Add `native_editor/mod.rs` reveal-decision tests.
- Add `layout.rs` preview projection edge tests for the currently modified mapping code.

This gives immediate protection around the editor under input without resurrecting the full deleted suite in one pass.
