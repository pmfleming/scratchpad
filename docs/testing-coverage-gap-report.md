# Testing Coverage Gap Report

Review date: 2026-05-10

## Scope

This review checks whether Scratchpad's current operation is covered by tests,
using the README product promises, Cargo test inventory, CI configuration, and
the project's own measurement tooling as evidence. It does not propose or add
implementation code.

## Evidence Checked

- `README.md` describes the operational surface: file open/save, multi-pane
  tabs, search/replace, encodings, artifact display, undo/text history, session
  restore, settings, and measurement tooling.
- `.github/workflows/ci.yml` runs `scripts/ci.ps1`, which gates format, clippy,
  `cargo test`, and selected measurement scripts.
- `cargo test -- --list` found 169 library tests, 3 tests in `scripts/count_lines.rs`,
  and 3 tests in `scripts/leverage_ast.rs`; all other binary targets listed 0
  tests. Total listed Cargo tests: 175.
- `Test-Path tests` returned `False`; there is no repository-level `tests/`
  integration-test directory in this checkout.
- `scripts/project_code_metrics.py --mode cli` reported 40,797 application code
  lines and 2,712 test code lines.
- `cargo test --lib` currently fails: 168 tests pass, 1 fails.

## Current Suite Health

The suite is currently red. The failing test is:

`app::ui::widget_ids::tests::app_code_uses_widget_id_wrappers_for_raw_egui_ids`

It reports raw egui ID escape hatches in `src/app/ui/callout.rs`:

- `ui.allocate_exact_size(` at line 112
- `ui.scope_builder(` at lines 125 and 132
- `egui::UiBuilder::new(` at lines 125 and 132

Until this is resolved, CI cannot cleanly answer whether newer behavior is
covered or regressed. This is a test-health issue separate from broader
coverage gaps.

## Existing Strong Spots

The project has useful unit coverage in several high-value areas:

- Text document undo/redo coalescing, history compaction, redo boundaries, and
  targeted text-history replay.
- Search matching, regex replacement expansion, cancellation, Unicode word
  boundary behavior, and search-state preview cache behavior.
- Visible control-character display, copy/cut fidelity for substituted control
  glyphs, cursor movement, selection publishing, scrolling math, and replacement
  preview geometry.
- Settings tab ordering, some settings layout checks, diagnostics logging,
  widget-ID policy checks, and a small set of restore-conflict/session-restore
  cases.

Those tests are valuable, but they are mostly inline unit tests. They do not
yet cover enough of the app's whole operation.

## Glaring Gaps

### 1. Missing `tests/` Integration Suite

The most obvious gap is structural: there is no `tests/` directory even though
the project has many cross-module workflows. Current coverage is concentrated
inside individual source modules. That makes it harder to prove that user-level
operations work across app state, domain objects, services, and UI command
dispatch.

High-value integration coverage belongs around workflows such as:

- launch with startup options, restore session, then open incoming files;
- open file, edit, save, reload from disk, and preserve metadata;
- split a tab, edit one view, search/replace, then persist and restore layout;
- close dirty/saved tabs through command and context-menu paths;
- recover from background I/O send failure or partial results.

### 2. Startup and CLI Switches Are Untested

`src/app/startup.rs` exposes `parse_startup_action`, and
`src/app/startup/parser.rs` implements `/clean`, `/here`, `/addto`,
`/addto:index:N`, `/files:`, `/help`, and `/version`. The README documents
these switches as runtime behavior, but there are no parser tests.

This leaves easy-to-break cases uncovered:

- quoted and comma-delimited `/files:` payloads;
- empty entries and unbalanced quotes;
- `/addto` versus `/here` conflicts;
- `/clean` combined with `/addto:index:N`;
- one-based index validation;
- unknown switch status messaging.

### 3. File Controller Workflows Have Thin Coverage

`src/app/services/file_service.rs` has focused tests for snapshot saves,
encoding failures, BOM/line-ending policy, and mixed line endings. That is good
coverage for low-level file serialization.

The higher-level controller paths are mostly untested:

- open/deduplicate existing paths in `src/app/services/file_controller/open.rs`;
- open-here and add-to-tab flows;
- save conflict gating in `src/app/services/file_controller/save.rs`;
- missing/stale/conflicted disk-state transitions;
- reload and reopen-with-encoding result application;
- session persistence side effects after open/save.

These are exactly the paths a user experiences when files change on disk or
when multiple files are opened from Explorer/startup.

### 4. Background I/O Coordination Is Not Covered

The async coordination layer in `src/app/app_state/background_io.rs` and
`src/app/services/background_io/worker.rs` has no direct tests. That layer owns
important operational correctness:

- partial streaming open results;
- result ordering after concurrent file reads;
- request ID/action matching;
- fallback when the background channel is unavailable;
- duplicate metadata and encoding-compliance refresh suppression;
- session restore/persist result application.

The code is careful, but without tests it is vulnerable to regressions that do
not appear in pure file-service tests.

### 5. Tab, Pane, Drag, and Context-Menu Workflows Are Under-Covered

The README emphasizes multi-pane workspace tabs, split creation/resizing, tile
promotion, tab combining, drag/drop ordering, and context-menu commands.
Coverage is much thinner than that promise:

- `src/app/commands.rs` and `src/app/commands/tab_transfer.rs` have no tests.
- `src/app/domain/tab.rs`, `src/app/domain/panes.rs`, and related layout,
  promotion, and repair modules have no direct tests.
- `src/app/ui/tab_drag/*` has no tests despite owning drag state, drop targets,
  autoscroll, and commit behavior.
- `src/app/ui/tab_strip/context_menu/close.rs` has no tests for close-all,
  close-others, close-saved, skipped dirty tabs, or settings-tab interaction.

This is the largest mismatch between advertised operation and automated
correctness evidence.

### 6. Session Persistence Is Only Partly Proven

There are a few targeted restore tests in
`src/app/services/session_store/restore.rs`, but the broader lifecycle is not
covered end to end:

- capturing a session from live tabs and views;
- persisting buffer temp files and manifests;
- pruning stale buffer files;
- applying restored sessions through `src/app/services/session_manager.rs`;
- async startup restore through `src/app/app_state/background_io.rs`;
- interaction between restored sessions, startup files, and legacy settings.

Session restore is one of Scratchpad's safety promises, so this deserves
integration-level coverage.

### 7. App Launch and Native Frame Behavior Lack Smoke Tests

`src/main.rs` builds native eframe options, loads settings, handles help/version
early exits, creates stores, and constructs `ScratchpadApp`. There are no tests
around launch decisions or viewport construction.

The UI itself has many pure helper tests, but there is no headless or scripted
smoke test that creates an app, runs representative commands, and verifies
state after a frame-level workflow.

### 8. Measurement Dashboard and Viewer Are Largely Untested

Scratchpad treats measurement as part of the product. The dashboard server and
viewer are operational tools, but coverage is minimal:

- `viewer/data-viewer.js` is a large browser script with no JS test harness.
- `scripts/dashboard_server.py` has no Python tests for run queuing, active-run
  conflicts, timeout handling, log serving, or app-package routes.
- Most Python producer scripts are exercised indirectly by CI, but their
  behavior is not unit tested.

If the dashboard is expected to be reliable product infrastructure, this is a
real coverage gap.

## Recommended Test Spine

The fastest path to whole-operation confidence is not a huge UI automation
suite. Start with a small integration spine:

1. Restore a green baseline by resolving the widget-ID guard failure.
2. Add the missing `tests/` directory with app-level integration tests that use
   temp directories and public/testable app constructors.
3. Cover startup parsing first; it is pure, high-value, and currently uncovered.
4. Add file-controller integration tests around open, save, stale disk state,
   reload, and reopen-with-encoding.
5. Add command-level tab/pane tests for split, combine, promote, reorder, close,
   and dirty-tab protection.
6. Add background I/O tests with fake or fallback channels to prove partial
   result ordering and request/action matching.
7. Add a minimal dashboard/viewer smoke harness with seeded `target/analysis`
   artifacts.

## Bottom Line

Scratchpad has meaningful unit tests around important internals, especially text
history, search, scrolling, visible control characters, diagnostics, and a few
settings behaviors. It does not yet have whole-operation coverage. The absent
`tests/` directory is the clearest signal: cross-module workflows are mostly
unchecked, and several README-level promises rely on manually trusted behavior
rather than automated integration evidence.
