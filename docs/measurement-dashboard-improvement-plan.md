# Measurement Dashboard Improvement Plan

## Goal

Move measurement from a script-first workflow into a dashboard-first review surface.

`scripts/open-overview.ps1` should become a bootstrapper rather than the place where users choose what to run. The dashboard should let a user refresh every artifact, refresh one review category, refresh one subcategory, and run individual checks where that is practical.

The dashboard should organize measurement into four top-level views:

- Quality Review
- Performance Review
- Correctness Review
- Map

The current JSON-producing scripts should remain the measurement layer. The dashboard should orchestrate them, display their status, and explain their results.

## Current State

The repository already has a useful measurement stack:

- `viewer/` is a static dashboard that reads JSON from `target/analysis/`.
- `scripts/open-overview.ps1` starts a local HTTP server and optionally refreshes selected artifacts before opening the viewer.
- Quality-like signals come from `scripts/hotspots.py` and `scripts/clone_alert.py`.
- Performance signals come from `scripts/slowspots.py`, `scripts/search_speed.py`, `scripts/capacity_report.py`, `scripts/resource_profiles.py`, `scripts/generate_flamegraphs.py`, and `scripts/speed_efficiency_report.py`.
- The map comes from `scripts/map.py` and is already enriched with maintainability and performance risk.
- Correctness is currently visible through Rust test files and inline tests, but not as a dashboard artifact.

The main gap is not raw measurement coverage. The gap is control, categorization, and traceability from dashboard category to script, artifact, module, and test.

## Target Experience

The dashboard opens directly into a measurement workspace with a compact run toolbar:

- Refresh All
- Refresh Quality
- Refresh Performance
- Refresh Correctness
- Refresh Map
- Run selected item
- Show last run log
- Show stale artifacts

Each category page should show:

- Last run time
- Artifact freshness
- Pass/fail or risk summary
- Items that can be refreshed individually
- Links to source modules, test files, profile binaries, reports, and flamegraphs
- A clear explanation of what each item measures

The dashboard should never require the user to remember a script flag for ordinary review work.

## Architecture

### 1. Keep Scripts As Producers

Retain the current rule: Python and Rust tools produce JSON, the viewer renders JSON.

Do not make individual scripts generate HTML. That separation is already healthy.

### 2. Add A Local Dashboard Control Plane

A static page cannot safely launch local processes on its own. Add a small local control server that starts with the dashboard.

Recommended shape:

- `scripts/dashboard_server.py`
- Serves `viewer/`
- Exposes local-only API routes
- Runs measurement tasks as subprocesses
- Streams task progress and logs
- Writes artifacts under `target/analysis/`

`scripts/open-overview.ps1` can remain as a compatibility launcher, but it should start the dashboard server and pass the initial port. Over time, explicit switches such as `-SearchSpeedOnly` and `-CloneOnly` can become optional shortcuts into the same task catalog used by the UI.

### 3. Add A Measurement Catalog

Create a single machine-readable catalog that defines every runnable dashboard item.

Suggested artifact:

- `target/analysis/measurement_catalog.json`

Suggested source:

- `scripts/measurement_catalog.py`, or
- `scripts/measurement_catalog.json` if static data is enough at first

Each item should include:

- `id`
- `category`
- `subcategory`
- `title`
- `description`
- `commands`
- `output_artifacts`
- `depends_on`
- `expensive`
- `supports_individual_run`
- `related_modules`
- `related_tests`
- `related_profiles`

The dashboard should render controls from this catalog instead of hardcoding every button.

### 4. Add A Run Manifest

Create a run manifest after every refresh.

Suggested artifact:

- `target/analysis/measurement_runs.json`

It should record:

- Task id
- Started time
- Finished time
- Exit code
- Duration
- Artifacts written
- Log path
- Error summary
- Git commit or working-tree marker

This lets the dashboard show stale data, failed refreshes, and partial refreshes honestly.

## Category Plan

## Quality Review

Quality Review replaces the current scattered Hotspots and Clones views.

It should contain two subcategories:

- Hotspots
- Clones

### Hotspots

The hotspot table should keep Cognitive Complexity, Cyclomatic Complexity, Maintainability Index, and Halstead Effort as the main scoring inputs.

Move SLOC out of the hotspot score.

SLOC should still be displayed as context because it helps explain scale, but it should not increase a module's quality risk score by itself. A large but simple module should not rank as a quality problem just because it is large.

Recommended scoring model:

- Quality risk score: cognitive + cyclomatic + maintainability + Halstead effort
- SLOC: separate size column
- Size warning: separate badge only when size is extreme

Dashboard changes:

- Rename the current Hotspots tab into Quality Review > Hotspots.
- Show score components as separate columns.
- Show SLOC in a separate Size section or column.
- Add filters for architectural layer, module path, and score band.
- Add an item-level refresh button for hotspots.

Producer changes:

- Update `scripts/hotspots.py` to emit score components distinctly.
- Emit both `quality_score` and `sloc`.
- Keep any old `score` field during migration if existing viewer code still consumes it.

### Clones

Clones should sit next to hotspots as a quality risk source.

Dashboard changes:

- Move the current Clones tab into Quality Review > Clones.
- Summarize clone groups, cross-file groups, largest span, and repeated ownership area.
- Add module and architectural-layer filters.
- Link clone instances back to affected modules.
- Add an item-level refresh button for clone analysis.

Producer changes:

- Keep `scripts/clone_alert.py`.
- Add module/layer ownership metadata if not already present.
- Add a stable clone id so clone trends can be compared across runs.

## Performance Review

Performance Review should be scenario-first instead of script-first.

Top-level performance items:

- Large Files: Loading and Manipulating
- Large Amount of Tabs: Loading and Manipulating
- Cutting/Pasting: Large Amounts of Data
- Splitting: Large Amount of Tabs
- Session Persistence Restore
- Searching: Large Files and Lots of Files

Each item should contain:

- Speed tests
- Capacity reports
- Resource profiles
- Flamegraphs

### Large Files: Loading and Manipulating

Existing inputs:

- `scripts/slowspots.py`
- `scripts/capacity_report.py`
- `scripts/resource_profiles.py`
- `profile_large_file_scroll`
- `profile_viewport_extraction`
- `profile_document_snapshot`

Expected dashboard rows:

- Open large file
- Scroll large file
- Extract viewport
- Snapshot document
- Edit large document

Useful metrics:

- Median latency
- P95 latency where available
- Maximum tested file size
- First unusable ceiling
- Allocated bytes
- Peak live bytes
- Working set
- Page faults
- Flamegraph availability

### Large Amount of Tabs: Loading and Manipulating

Existing inputs:

- `benches/tab_stress.rs`
- `scripts/capacity_report.py`
- `scripts/resource_profiles.py`
- `profile_tab_operations`
- `profile_tab_tile_layout`

Expected dashboard rows:

- Open many tabs
- Switch active tab
- Reorder tabs
- Render tab strip
- Manipulate loaded workspace

Useful metrics:

- Tab count ceiling
- Active-tab switch latency
- Reorder latency
- Working set growth
- Page faults
- Flamegraph availability

### Cutting/Pasting: Large Amounts of Data

Existing inputs:

- `profile_large_file_paste`
- `scripts/resource_profiles.py`
- `scripts/capacity_report.py`
- Relevant slowspot rows

Expected dashboard rows:

- Paste into empty file
- Paste into large file
- Cut large selection
- Undo paste
- Redo paste

Useful metrics:

- Paste latency
- Cut latency
- Undo/redo latency
- Insert size
- Buffer size before operation
- Allocation cost
- Metadata refresh cost
- Flamegraph availability

### Splitting: Large Amount of Tabs

Existing inputs:

- `profile_large_file_split`
- `profile_tab_tile_layout`
- `benches/tab_stress.rs`
- `scripts/capacity_report.py`

Expected dashboard rows:

- Split loaded tab
- Rebalance split tree
- Close split
- Promote tile
- Restore multi-pane workspace

Useful metrics:

- Split operation latency
- Rebalance latency
- Tile count ceiling
- Memory growth
- Flamegraph availability

### Session Persistence Restore

Existing inputs:

- `scripts/resource_profiles.py`
- `scripts/capacity_report.py`
- `tests/session_store_tests.rs`
- `tests/startup_tests.rs`

Expected dashboard rows:

- Persist session with many tabs
- Restore session with many tabs
- Restore split workspace
- Handle missing files
- Handle restore conflicts

Useful metrics:

- Save duration
- Restore duration
- Manifest size
- Tab count
- Tile count
- Working set
- Page faults
- Correctness test links

### Searching: Large Files and Lots of Files

Existing inputs:

- `scripts/search_speed.py`
- `scripts/speed_efficiency_report.py`
- `profile_search_current_app_state`
- `profile_search_all_tabs`
- `profile_search_dispatch`
- `tests/search_tests.rs`

Expected dashboard rows:

- Active file search
- Current workspace search
- All open tabs search
- Regex search
- First-response latency
- Full-completion latency
- Search dispatch overhead

Useful metrics:

- First response
- Full completion
- Corpus size
- Match count
- Scope
- Regex/plain-text mode
- Flamegraph availability
- Correctness test links

## Correctness Review

Correctness Review should turn Rust tests into a navigable dashboard artifact.

It should include:

- Integration tests under `tests/`
- Inline `#[cfg(test)]` modules under `src/`
- Test status from the latest run
- Architectural layer
- Owning module
- One-line explanation under 10 words

### Test Catalog

Add a correctness catalog producer.

Recommended script:

- `scripts/test_catalog.py`

Recommended artifacts:

- `target/analysis/correctness_review.json`
- `target/analysis/test_catalog.json`

The first version can parse test names and paths, then use a small override file for hand-written descriptions.

Suggested override file:

- `scripts/test_descriptions.json`

Each test item should include:

- `id`
- `name`
- `path`
- `line`
- `layer`
- `module`
- `description`
- `kind`: `integration` or `inline`
- `last_status`
- `last_duration`
- `command`

Description rule:

- Less than 10 words
- Plain language
- No duplicate boilerplate

Examples:

- `search_tests.rs`: "Verifies search scopes and matches."
- `session_store_tests.rs`: "Checks saved workspace restoration."
- `piece_tree_tests.rs`: "Validates piece-tree editing behavior."
- `file_controller_tests.rs`: "Covers open/save controller flows."

### Architectural Layers

Use these initial layers:

- App Shell and State
- Commands and Transactions
- Domain Model
- Buffer and Text Storage
- Services and Persistence
- Search
- UI and Editor Interaction
- Startup and Settings

Initial mapping:

- `src/app/app_state*`, `tests/app_tests.rs`: App Shell and State
- `src/app/commands*`, `tests/transaction_tests.rs`: Commands and Transactions
- `src/app/domain/tab*`, `src/app/domain/panes*`, `tests/tab_tests.rs`, `tests/tab_manager_tests.rs`: Domain Model
- `src/app/domain/buffer*`, `tests/buffer_tests.rs`, `tests/piece_tree_tests.rs`: Buffer and Text Storage
- `src/app/services*`, `tests/file_service_tests.rs`, `tests/file_controller_tests.rs`, `tests/session_store_tests.rs`: Services and Persistence
- `src/app/services/search*`, `src/app/app_state/search_state*`, `tests/search_tests.rs`: Search
- `src/app/ui*`: UI and Editor Interaction
- `src/app/startup*`, `src/app/app_state/settings_state*`, `tests/startup_tests.rs`, `tests/settings_store_tests.rs`: Startup and Settings

Dashboard changes:

- Add Correctness Review as a top-level category.
- Show test counts by layer.
- Show failing, skipped, and stale tests first.
- Provide filters for layer, file, test kind, and status.
- Allow running all tests, a layer, a file, or an individual test.

Producer changes:

- Capture `cargo test -- --format json` when available.
- Fall back to parsed text output if JSON format is unstable locally.
- Store last test results in `target/analysis/correctness_review.json`.

## Map

The Map should remain a dependency and architecture map, but evolve into the module health view for Quality, Performance, and Correctness.

It should retain:

- Current dependency relationships
- Current layout modes
- Current metric coloring
- Current module detail functionality
- Current filtering and zoom controls

It should add:

- Quality score per module
- Performance risk per module
- Correctness status per module
- Test count per module
- Failed or stale tests per module
- Related flamegraphs per module
- Related capacity/resource profile scenarios
- Related clone groups

The module detail panel should become a compact module health dossier:

- Module path
- Architectural layer
- Dependencies
- Dependents
- Quality summary
- Performance summary
- Correctness summary
- Related dashboard items

The map should be able to answer:

- Which modules are complex?
- Which modules are slow?
- Which modules lack tests?
- Which modules have failing tests?
- Which modules are risky across more than one category?

## Dashboard Information Design

Top-level navigation:

- Overview
- Quality Review
- Performance Review
- Correctness Review
- Map
- Run Log

Overview should show:

- Overall health by category
- Last full refresh time
- Stale artifacts
- Highest quality risks
- Highest performance risks
- Failing correctness areas
- Modules with combined risk

Quality Review should show:

- Hotspots
- Clones
- Quality trend
- Refresh controls

Performance Review should show:

- Scenario cards
- Per-scenario detail tables
- Capacity ceilings
- Resource profiles
- Flamegraphs
- Refresh controls

Correctness Review should show:

- Test layers
- Test list
- Latest status
- Individual run controls
- Refresh controls

Map should show:

- Existing dependency graph
- Metric coloring for total, quality, performance, correctness, change, architecture
- Module detail health dossier

Run Log should show:

- Current task queue
- Completed runs
- Failed runs
- Command output
- Artifact paths

## Refresh Model

Refresh granularity should be:

- Full dashboard
- Category
- Subcategory
- Individual item

Suggested API:

- `GET /api/catalog`
- `GET /api/runs`
- `POST /api/run/all`
- `POST /api/run/category/{category}`
- `POST /api/run/item/{id}`
- `GET /api/run/{run_id}`
- `GET /api/run/{run_id}/log`

Task execution rules:

- Run cheap static analysis in parallel.
- Run expensive benchmarks sequentially unless explicitly safe.
- Mark flamegraphs as expensive and optional.
- Preserve old artifacts if a refresh fails.
- Record failures in the run manifest.
- Never hide stale data; label it.

## Data Contracts

New or updated artifacts:

- `target/analysis/measurement_catalog.json`
- `target/analysis/measurement_runs.json`
- `target/analysis/quality_review.json`
- `target/analysis/performance_review.json`
- `target/analysis/correctness_review.json`
- `target/analysis/map.json`

Compatibility rule:

- Existing artifacts should keep working while the new category artifacts are introduced.
- The dashboard can initially derive category views from existing artifacts.
- Once stable, scripts can emit richer category-native JSON.

## Implementation Phases

### Phase 1: Reframe The Existing Viewer

- Rename the dashboard title and navigation around the four target categories.
- Move Hotspots and Clones under Quality Review.
- Move slowspots, search speed, speed report, capacity, resource profiles, and flamegraphs under Performance Review.
- Keep Map as a top-level category.
- Add a placeholder Correctness Review fed by a static catalog.
- Keep current file-input fallback.

Exit criteria:

- The dashboard reads the same artifacts as today.
- The user can browse by the new categories.
- No measurement behavior changes yet.

### Phase 2: Separate SLOC From Quality Score

- Update `scripts/hotspots.py` output to distinguish complexity score from SLOC.
- Update the viewer to show SLOC separately.
- Update map enrichment to use the new quality score.
- Keep backward compatibility with existing `score` fields during migration.

Exit criteria:

- Large simple files stop ranking as quality hotspots due to size alone.
- SLOC remains visible as context.

### Phase 3: Add Correctness Catalog

- Add `scripts/test_catalog.py`.
- Parse integration tests in `tests/`.
- Parse inline tests in `src/`.
- Add layer mapping.
- Add short descriptions through heuristics and overrides.
- Emit `target/analysis/correctness_review.json`.
- Render Correctness Review in the dashboard.

Exit criteria:

- Every discovered test appears in a categorized list.
- Every test has a short description.
- Tests can be filtered by layer and path.

### Phase 4: Add Dashboard-Controlled Refresh

- Add `scripts/dashboard_server.py`.
- Move task definitions into a measurement catalog.
- Add dashboard buttons for full, category, subcategory, and item refreshes.
- Add task progress and run logs.
- Keep `scripts/open-overview.ps1` as the launcher.

Exit criteria:

- Ordinary refreshes are started from the dashboard.
- Script switches are no longer required for day-to-day use.
- Failed tasks are visible without losing old successful artifacts.

### Phase 5: Build Scenario-First Performance Review

- Group existing performance artifacts into the six scenario items.
- Add missing benchmarks or profile binaries where coverage is thin.
- Link each scenario to speed, capacity, resource, and flamegraph evidence.
- Show flamegraph availability inline in each scenario.

Exit criteria:

- Performance Review answers user workflow questions, not script questions.
- Each scenario has speed, capacity, resource, and flamegraph coverage.

### Phase 6: Enrich The Map

- Merge Quality, Performance, and Correctness signals into map nodes.
- Add correctness color mode.
- Add module health detail sections.
- Link map modules back to category items.

Exit criteria:

- The map remains useful as a dependency view.
- The map also shows per-module quality, performance, and correctness health.

### Phase 7: Polish And Guardrails

- Add stale artifact warnings.
- Add expensive-run confirmation for flamegraphs and long benchmarks.
- Add run cancellation if the control server can support it cleanly.
- Add trend snapshots for repeated runs.
- Document the new workflow in `docs/measurement-tools.md`.

Exit criteria:

- The dashboard feels like the primary measurement tool.
- The old script-first flow remains available for automation and CI.

## Risks And Decisions

### Static Viewer Versus Local Server

The dashboard needs a local server if it is going to start refreshes. Browser-only JavaScript cannot launch Python, PowerShell, or Cargo safely.

Decision:

- Keep a static rendering model.
- Add a local-only control server for refresh orchestration.

### Expensive Tasks

Flamegraphs, capacity sweeps, and large performance probes can be slow or platform-sensitive.

Decision:

- Mark expensive tasks clearly.
- Keep them out of default quick refresh unless the user chooses Full or Performance Deep Refresh.

### Correctness Descriptions

Automatically generating good descriptions from test names will be imperfect.

Decision:

- Use heuristics for first coverage.
- Add `scripts/test_descriptions.json` for important hand-authored descriptions.
- Enforce the less-than-10-words rule in the catalog script.

### Backward Compatibility

The existing dashboard works from current artifact names.

Decision:

- Keep existing artifacts during migration.
- Add category artifacts alongside them.
- Remove old viewer assumptions only after the new contracts are stable.

## Recommended First Cut

Start with a low-risk dashboard reframe:

1. Add the four-category navigation.
2. Put current views under their new categories.
3. Update hotspot scoring so SLOC is separate.
4. Add a generated Correctness Review catalog without run buttons.
5. Add the local refresh server after the information architecture is stable.

This sequence gives immediate clarity without entangling UI reorganization, test discovery, and process orchestration in one large change.
