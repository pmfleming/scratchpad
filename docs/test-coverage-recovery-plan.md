# Test Coverage Recovery Plan

Review date: 2026-05-11

## Goal

Recreate the lost repository-level `tests/` integration suite and use it as the
spine for broader correctness coverage. Keep inline unit tests for small pure
helpers, but move user-level workflows back into integration tests where they
can prove that app state, domain objects, services, and command paths still work
together.

## Current Baseline

- `tests/` is currently absent (`Test-Path tests` returned `False`).
- `scripts/test_catalog.py` statically finds 178 Rust tests, all inline and 0
  integration tests.
- Current catalog layers are App Shell and State: 47, Buffer and Text Storage:
  32, Search: 3, Services and Persistence: 18, UI and Editor Interaction: 78.
- `scripts/project_code_metrics.py --mode cli` reports 41,572 application code
  lines and 3,074 test code lines, or roughly 7.4% test-to-application code by
  line count.
- `cargo test -- --list` cannot produce a live inventory in this checkout
  because the tree does not compile right now. The immediate compile blockers
  observed during this review are `src/app/ui/settings/widgets.rs` referring to
  inaccessible `preview` helpers, and `src/app/app_state/search_state/api_tests.rs`
  using `as_deref()` on `Option<SearchReplacementPreview>`.

## Historical Suite To Recover

Git history confirms that the integration suite existed and was deleted in
commit `a83f0bbdb46a876fe683952345bc0fe6ae09189b` on 2026-05-03. The parent of
that commit contains 11 files, 3,045 deleted lines, and 127 integration tests:

| Historical file | Test count | Recovery priority |
| --- | ---: | --- |
| `tests/startup_tests.rs` | 9 | First |
| `tests/tab_manager_tests.rs` | 1 | First |
| `tests/tab_tests.rs` | 12 | First |
| `tests/file_service_tests.rs` | 10 | First |
| `tests/settings_store_tests.rs` | 8 | First |
| `tests/buffer_tests.rs` | 14 | Second |
| `tests/search_tests.rs` | 20 | Second |
| `tests/session_store_tests.rs` | 9 | Second |
| `tests/file_controller_tests.rs` | 14 | Third |
| `tests/app_tests.rs` | 6 | Third |
| `tests/piece_tree_tests.rs` | 24 | Audit before restoring |

`tests/transaction_tests.rs` was removed one day earlier in commit
`43738d72e65e6d4514239325cb4629c3a90c49fa`; review it separately once the main
suite is compiling again.

Useful recovery references:

```powershell
git ls-tree -r --name-only a83f0bbdb46a876fe683952345bc0fe6ae09189b^ tests
git show a83f0bbdb46a876fe683952345bc0fe6ae09189b^:tests/startup_tests.rs
git show a83f0bbdb46a876fe683952345bc0fe6ae09189b^:tests/file_controller_tests.rs
```

## Proposed Test Folder Shape

```text
tests/
  common/
    mod.rs                  Shared temp-dir, app, path, and search wait helpers
    fixtures.rs             Small file contents and encoding fixtures
  startup_tests.rs          CLI and startup-option parsing
  tab_manager_tests.rs      Tab ordering and active-tab preservation
  tab_tests.rs              Pane, split, combine, promote, and repair invariants
  file_service_tests.rs     Low-level read/write encoding and line-ending cases
  settings_store_tests.rs   TOML settings load/save/migration cases
  buffer_tests.rs           Buffer metadata and format/artifact behavior
  search_tests.rs           Cross-tab and cross-buffer search/replace workflows
  session_store_tests.rs    Manifest, temp-buffer, restore, and pruning flows
  file_controller_tests.rs  Open, open-here, save, reload, conflict workflows
  app_workflow_tests.rs     App-level startup/settings/session integration
  background_io_tests.rs    Async open/save/reload result application
```

Keep `tests/common` intentionally small. It should provide helpers like
`test_app()`, `test_app_with_stores()`, `write_text_file()`,
`write_settings_file()`, `collect_leaf_area_fractions()`, and
`wait_for_search_matches()`. Do not hide assertions in helpers unless they are
true invariants shared by many tests.

## Recovery Plan

### Phase 0: Restore A Green Baseline

1. Fix the current compile blockers before adding or porting tests.
2. Run `cargo test -- --list` and capture the real current test inventory.
3. Run `cargo test` and record whether failures are product regressions or stale
   test expectations.
4. Regenerate `target/analysis/test_catalog.json` with
   `python scripts/test_catalog.py --mode analysis --output target/analysis/test_catalog.json`.

Acceptance: the existing inline suite builds, `cargo test -- --list` succeeds,
and the catalog has a trustworthy pre-restoration baseline.

### Phase 1: Recreate The Folder And Low-Drift Tests

1. Add `tests/common/mod.rs` and recreate the test folder with small, fast
   helpers.
2. Port the historical tests least likely to have API drift:
   `startup_tests.rs`, `tab_manager_tests.rs`, `tab_tests.rs`,
   `file_service_tests.rs`, and `settings_store_tests.rs`.
3. Prefer public, behavior-level APIs. If a test needs private state, first ask
   whether it belongs inline beside that module instead of forcing a wider app
   surface.
4. Mark stress-style tests as `#[ignore]` or gate them behind an explicit
   environment variable so default `cargo test` stays fast.

Acceptance: `tests/` exists, the catalog reports integration tests again, and
all Phase 1 tests pass in normal `cargo test`.

### Phase 2: Port Cross-Module Workflows

1. Restore and adapt `buffer_tests.rs`, `search_tests.rs`, and
   `session_store_tests.rs`.
2. De-duplicate against newer inline tests before porting. Keep integration
   tests only when they cross module boundaries or prove README-level behavior.
3. For search, keep pure matcher cases inline or in service tests, and reserve
   integration tests for all-open-tabs, active-workspace-tab, replacement, and
   repeated-buffer-name navigation.
4. For session storage, cover capture, persistence, temp-buffer recovery, stale
   buffer pruning, settings migration, and restored pane layout repair.

Acceptance: integration coverage includes startup, tabs, file service, settings,
buffers, search workflows, and session restore.

### Phase 3: Rebuild File Controller And App Workflow Coverage

1. Restore and adapt `file_controller_tests.rs` and `app_tests.rs`.
2. Cover open separate tabs, open here, active-tab target, CLI startup files,
   settings TOML focus loss, disk-state capture, save conflict gating, reload,
   reopen with encoding, and duplicate-path handling.
3. Add app workflow tests that compose startup settings, session restore, file
   open disposition, dirty buffers, and persisted layout.
4. Keep large tab stress as a separate ignored test or a profiling/measurement
   script, not a default unit gate.

Acceptance: the integration suite proves the main file-open/save/session paths
that users experience from the app shell.

### Phase 4: Add New Coverage That The Old Suite Did Not Have

1. Add `background_io_tests.rs` for request/action matching, partial open result
   ordering, fallback when the dispatcher is unavailable, async reload result
   application, and stale-result suppression.
2. Add command-level tests for split, combine, promote, close view, close saved,
   close others, close all, dirty-tab protection, and settings-tab interaction.
3. Add focused tests around tab drag/drop state where the logic is pure enough
   to exercise without rendering.
4. Add a small Python test path for dashboard server behavior: seeded analysis
   artifact serving, active-run rejection, timeout handling, and log retrieval.
5. Add a browser or JS harness for `viewer/data-viewer.js` only after the Rust
   integration spine is back in place.

Acceptance: new tests cover the app's current architecture, not just the
pre-deletion behavior.

### Phase 5: Make Coverage Loss Visible In CI

1. Teach `scripts/test_catalog.py` or a small companion gate to fail when
   `integration_count` drops below a floor.
2. Start the floor at the restored Phase 1 count, then raise it after Phases 2
   and 3.
3. Add the catalog generation to `scripts/ci.ps1` after `cargo test`.
4. Track both test count and layer distribution so a future deletion of `tests/`
   is obvious even if inline tests still pass.

Acceptance: CI fails when the integration suite disappears or when coverage
collapses into one layer.

## Coverage Targets

- Short term: restore at least 40 integration tests across startup, tabs, file
  service, settings, and basic app workflows.
- Medium term: restore or replace the old 127 integration tests, excluding any
  cases now better covered inline.
- Medium term: lift test code from 3,074 lines toward roughly 6,000 lines, which
  would return the project to about a 14% to 15% test-to-application line ratio.
- Long term: keep all README-level product promises represented by at least one
  integration test or explicit measurement/smoke test.

## Highest-Value Missing Scenarios

- Startup parser and startup application: `/clean`, `/addto`, `/addto:index:N`,
  `/files:`, help/version, saved open disposition, and restore suppression.
- File workflows: open, open here, open into active tab, save, stale disk state,
  conflict detection, reload, reopen with encoding, and session side effects.
- Workspace workflows: split, resize, combine, promote, close one view, close
  tabs by command/context menu, dirty-tab protection, and restore layout repair.
- Search workflows: all open tabs, active workspace tab, duplicate names in one
  workspace tab, replace all, replacement preview, and selection/current-file
  scope boundaries.
- Persistence workflows: session manifest round trip, temp-buffer content,
  stale temp-file pruning, settings migration, and startup restore conflicts.
- Background I/O workflows: partial result ordering, request ID matching,
  channel fallback, cancelled/stale results, and applying async reload/reopen
  results to the correct buffer.

## Working Rules For The Recreated Suite

- Integration tests should use `tempfile` and never rely on real user settings,
  global temp paths, or a fixed working directory.
- Tests should assert user-visible state and persisted data, not incidental UI
  implementation details.
- Keep pure helper and parser tests near the module when that gives clearer
  failure locality; use `tests/` for workflows crossing module boundaries.
- Default tests should be deterministic and fast. Stress, fuzz, and capacity
  cases belong behind `#[ignore]`, an environment variable, or the measurement
  tooling.
- Every new high-risk feature should add either an inline unit test for the pure
  logic or an integration test under `tests/`, with the catalog layer updated if
  the file introduces a new coverage area.
