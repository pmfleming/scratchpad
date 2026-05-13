# Scratchpad Measurement Tools (2)

As of May 8, 2026, Scratchpad's measurement tool is a local, dashboard-first
measurement suite. The Python and Rust producers emit JSON artifacts under
`target/analysis/`; the viewer reads those artifacts; the dashboard server can
refresh all, category, or individual catalog tasks and records run logs.

The goal is not a polished public report. It is fast local evidence for quality,
performance, correctness, scale ceilings, resource cost, and module health while
Scratchpad evolves.

## Current Shape

- Producer scripts live under `scripts/` and write JSON.
- Profile and probe binaries live under `src/bin/`.
- Shared report mode handling lives in `scripts/report_modes.py`.
- Shared benchmark metadata, workload families, and flamegraph configs live in
  `scripts/perf_report_shared.py`.
- The task catalog lives in `scripts/measurement_catalog.py`.
- The local dashboard server lives in `scripts/dashboard_server.py`.
- The static viewer lives under `viewer/`.
- The launcher is `scripts/open-overview.ps1`.
- CI-oriented checks are in `scripts/ci.ps1`.

Most producer scripts support:

- `--mode cli`: print a readable summary.
- `--mode analysis`: emit JSON to stdout, or to `--output` when supplied.
- `--mode visibility`: write the viewer-ready artifact under `target/analysis/`
  and print where it went.

On Windows examples below use `.venv\Scripts\python.exe`. On other platforms use
`.venv/bin/python`.

## Dashboard Workflow

Fast open, using existing artifacts:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1
```

Full refresh, then open the dashboard:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -FullUpdate
```

Targeted launcher refreshes:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -SearchSpeedOnly
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -CloneOnly
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -FlamegraphOnly
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -AppPackage
```

Useful launcher details:

- `-Port` sets the requested server port; the launcher moves to the next open
  port if needed.
- `-FullUpdate` runs the standard producer set, including flamegraphs.
- `-Flamegraph` is a legacy alias for flamegraph-only refresh.
- `-LegacyStaticServer` starts `python -m http.server` instead of the dashboard
  API server, so refresh buttons and App Package APIs are unavailable.
- On Windows the launcher restarts itself as Administrator before serving, so
  flamegraph and process-inspection workflows have the privileges they need.

Start only the dashboard API server:

```powershell
.venv\Scripts\python.exe scripts\dashboard_server.py --port 8000
```

Then open `http://localhost:8000/viewer/`.

## Firebase Snapshot Publishing

The Firebase publish path is planned as a static snapshot of the current
`viewer/` pages and current `target/analysis/` artifacts. Firebase Hosting
should not refresh measurements or call the local dashboard API. See
[Firebase Overview Publish Plan](firebase-overview-publish-plan.md) for the
implementation plan, including hosted-mode refresh disabling and credential
`.gitignore` reminders.

## Dashboard API

The dashboard server binds to `127.0.0.1` and serves both static files and local
JSON APIs.

Refresh routes:

- `POST /api/run/all`
- `POST /api/run/category/{category}`
- `POST /api/run/item/{id}`

Read routes:

- `GET /api/catalog`
- `GET /api/runs`
- `GET /api/run/{run_id}/log`
- `GET /api/app-package`

App package maintenance route:

- `POST /api/app-package/clear-buffers`

Only one refresh run is active at a time. If a run is queued or running, a second
refresh returns `409` with the active run id. Runs are persisted in
`target/analysis/measurement_runs.json`; per-run logs are written under
`target/analysis/logs/`. An all-run on Windows also cleans up stale Scratchpad
measurement processes from `target/` before starting.

## Viewer Tabs

The viewer currently exposes:

- Overview
- Quality Review
- Performance Review
- Correctness Review
- Map
- App Package
- Run Log

The App Package tab is operational diagnostics, not a benchmark. It reads the
Scratchpad session package under the temp directory, including `session.json`,
buffer snapshot files, and `error.log`, and it can clear persisted buffers when
Scratchpad is not running.

## Task Catalog

`scripts/measurement_catalog.py` is the source of truth for dashboard refresh
buttons and run routing. It supports all, category, and item refresh. Subcategory
is present as task metadata for grouping, but it is not a dashboard API selector
today.

| Task id | Category | Producer | Primary output |
| --- | --- | --- | --- |
| `quality.hotspots` | quality | `scripts/hotspots.py` | `target/analysis/hotspots.json` |
| `quality.clones` | quality | `scripts/clone_alert.py` | `target/analysis/clones.json` |
| `quality.escape_hatches` | quality | `scripts/rust_escape_hatches.py` | `target/analysis/rust_escape_hatches.json` |
| `quality.locality_dynamic` | quality | `scripts/locality_bench.py` | `target/analysis/locality_metrics.json` |
| `quality.locality_leverage` | quality | `scripts/leverage_metrics.py` | `target/analysis/leverage_metrics.json` |
| `performance.slowspots` | performance | `scripts/slowspots.py` | `target/analysis/slowspots.json` |
| `performance.frame_metrics` | performance | `scripts/frame_metrics.py` | `target/analysis/frame_metrics.json` |
| `performance.search` | performance | `scripts/search_speed.py` | `target/analysis/search_speed.json` |
| `performance.capacity` | performance | `scripts/capacity_report.py` | `target/analysis/capacity_report.json` |
| `performance.resources` | performance | `scripts/resource_profiles.py` | `target/analysis/resource_profiles.json` |
| `performance.flamegraphs` | performance | `scripts/generate_flamegraphs.py` | `target/analysis/flamegraphs.json` and SVGs |
| `performance.report` | performance | `scripts/speed_efficiency_report.py`, `scripts/performance_review.py` | `target/analysis/speed_efficiency_report.json`, `target/analysis/performance_review.json` |
| `correctness.catalog` | correctness | `scripts/test_catalog.py` | `target/analysis/correctness_review.json`, `target/analysis/test_catalog.json` |
| `correctness.all` | correctness | `scripts/test_catalog.py --run` | `target/analysis/correctness_review.json` |
| `map.architecture` | map | `scripts/map.py` | `target/analysis/map.json` |
| `map.project_code_metrics` | map | `scripts/project_code_metrics.py` | `target/analysis/project_code_metrics.json` |

## Output Artifacts

The viewer currently attempts to load:

- `target/analysis/measurement_catalog.json`
- `target/analysis/measurement_runs.json`
- `target/analysis/hotspots.json`
- `target/analysis/slowspots.json`
- `target/analysis/frame_metrics.json`
- `target/analysis/search_speed.json`
- `target/analysis/capacity_report.json`
- `target/analysis/resource_profiles.json`
- `target/analysis/speed_efficiency_report.json`
- `target/analysis/performance_review.json`
- `target/analysis/clones.json`
- `target/analysis/rust_escape_hatches.json`
- `target/analysis/locality_metrics.json`
- `target/analysis/leverage_metrics.json`
- `target/analysis/map.json`
- `target/analysis/project_code_metrics.json`
- `target/analysis/flamegraphs.json`
- `target/analysis/flamegraphs/*.svg`
- `target/analysis/correctness_review.json`
- `target/analysis/test_catalog.json`
- `target/analysis/logs/*.log`

`flamegraphs.json`, `measurement_runs.json`, `measurement_catalog.json`, and the
App Package payload may be absent without producing the same prominent load
warning as core review artifacts.

## Script Roles

Quality producers:

- `scripts/hotspots.py` ranks file and function complexity. SLOC is reported as
  size context rather than being folded into the quality score.
- `scripts/clone_alert.py` detects token-based clones and can use token, AST,
  MIR, or combined engines.
- `scripts/rust_escape_hatches.py` audits unsafe blocks, FFI, raw memory,
  `Deref`/`DerefMut` impls, glob imports, immutable container-reference return
  types, layout/linkage attributes, mutable globals, and lint suppressions.
- `scripts/locality_bench.py` measures dependency spread, hidden coupling,
  interface explicitness, test proximity, and change locality.
- `scripts/leverage_metrics.py` measures module reach, invariant surface,
  divergence pressure, co-change ripple, and unsafe surface. It uses
  `scripts/leverage_ast.rs` for AST style counts.

Performance producers:

- `scripts/slowspots.py` reads or refreshes broad Criterion benchmark results and
  ranks latency rows by score, stability, workload family, budget, and matching
  profile coverage.
- `scripts/frame_metrics.py` runs the headless editor frame probe and reports
  120 Hz p50/p95/p99 frame timing plus prepare, chrome, active-surface, gutter,
  scroll, paint, background-poll, dialog, shortcut, and finish phase ownership.
- `scripts/search_speed.py` owns the dedicated search dataset for Active,
  Current, and All scopes. It separates full completion, first response, single
  file growth, aggregate corpus growth, and dispatch overhead.
- `scripts/capacity_report.py` runs or falls back for threshold sweeps and records
  the first unusable ceiling plus resource guidance.
- `scripts/resource_profiles.py` runs or falls back for allocation, working-set,
  page-fault, handle, and session-cost probes.
- `scripts/generate_flamegraphs.py` indexes CPU profiles and generates SVGs with
  `cargo flamegraph` when the tool, privileges, and disk space are available.
- `scripts/speed_efficiency_report.py` combines slowspots, search speed,
  flamegraph coverage, capacity ceilings, and resource profiles into triage rows.
- `scripts/performance_review.py` turns those artifacts into scenario-first
  coverage for product promises.

Correctness and map producers:

- `scripts/test_catalog.py` catalogs inline and integration tests by architecture
  layer. With `--run`, it also runs `cargo test` and attaches status.
- `scripts/map.py` builds the module dependency and risk map, enriched by
  quality, performance, correctness, locality, and leverage evidence.
- `scripts/project_code_metrics.py` emits application, test, and other Rust line
  splits plus first-parent GitHub history samples.

Operational scripts:

- `scripts/measurement_catalog.py` emits the dashboard refresh catalog.
- `scripts/dashboard_server.py` serves the viewer, APIs, run queue, and run logs.
- `scripts/open-overview.ps1` initializes Python tooling, optionally refreshes
  artifacts, starts the server, and opens the viewer.
- `scripts/ci.ps1` runs `cargo fmt --check`, `cargo clippy`, `cargo test`, then
  selected measurement checks. Use its skip switches for targeted CI runs.
- `scripts/app_package.py` provides the App Package diagnostics payload consumed
  by the dashboard server.

## Current Coverage

Quality coverage includes complexity hotspots, clone groups, Rust escape hatches,
code locality, architecture leverage, and module-risk overlays.

Performance coverage includes:

- Broad Criterion slowspots.
- Dedicated search scaling across Active, Current, and All modes.
- Search first-response and full-completion latency.
- Search dispatch and target collection cost before worker-side matching.
- Capacity ceilings for file size, layout bytes, many files, search file size,
  search target count, tab count, split count, view count, and paste size.
- Resource profiles for file-backed first visible paint, many-file tracking,
  search allocation, paste allocation, tab/view working-set growth, and session
  persist/restore/startup-visible restore cost.
- Scenario review for Large Files, Many Files, Search, Many Tabs, Many Views,
  Large Text Mutation, and Session Persistence Restore.

Correctness coverage includes architecture-layer test catalogs and optional full
Rust test execution.

Map coverage includes dependency topology, maintainability risk, change risk,
performance risk, architectural risk, correctness signals, locality signals, and
leverage signals.

## Flamegraph Profiles

`scripts/perf_report_shared.py` defines the flamegraph configs consumed by
`scripts/generate_flamegraphs.py`, capacity guidance, slowspot/search metadata,
and performance review coverage.

| Profile id | Binary | Focus |
| --- | --- | --- |
| `tab_operations_profile` | `profile_tab_operations` | Tab activation, reorder, and movement |
| `tab_tile_layout_profile` | `profile_tab_tile_layout` | Split resize, rebalance, and tile layout |
| `view_navigation_profile` | `profile_view_navigation` | Navigation through duplicated and distinct editor views |
| `search_current_app_state_profile` | `profile_search_current_app_state` | Active and current-tab search through app state |
| `search_all_tabs_profile` | `profile_search_all_tabs` | All-open-tabs search through the tab manager |
| `search_dispatch_profile` | `profile_search_dispatch` | Search request building and target collection |
| `document_snapshot_profile` | `profile_document_snapshot` | Large piece-tree document snapshot creation |
| `viewport_extraction_profile` | `profile_viewport_extraction` | Visible-range and overscanned text-window extraction |
| `scroll_stress_profile` | `profile_scroll_stress` | Repeated scroll layout and repaint work |
| `paste_stress_profile` | `profile_paste_stress` | Large insert, metadata refresh, and undo state updates |
| `split_stress_profile` | `profile_split_stress` | Repeated split and rebalance work |
| `search_capacity_profile` | `profile_search_capacity` | Upper-end large-text search capacity |

Preferred command:

```powershell
.venv\Scripts\python.exe scripts\generate_flamegraphs.py --mode visibility
```

The script writes `target/analysis/flamegraphs.json` and SVGs under
`target/analysis/flamegraphs/`. If `cargo-flamegraph` is missing, privileges are
insufficient, disk space is low, or a profile fails, the index still records the
coverage config. Existing SVGs are treated as usable baseline evidence; transient
refresh failures are only attached as row issues when no SVG is available.

To rebuild the flamegraph index from already-generated SVGs without invoking
`cargo-flamegraph`, use:

```powershell
.venv\Scripts\python.exe scripts\generate_flamegraphs.py --mode visibility --index-only
```

This is the preferred cleanup step after an interrupted or untrusted flamegraph
refresh, such as a run from a dirty working tree or a run stopped by low disk
space. Follow it by regenerating the coordinated reports:

```powershell
.venv\Scripts\python.exe scripts\speed_efficiency_report.py --mode visibility
.venv\Scripts\python.exe scripts\performance_review.py --mode visibility
```

## Direct Commands

Catalog and dashboard data:

```powershell
.venv\Scripts\python.exe scripts\measurement_catalog.py --mode visibility
.venv\Scripts\python.exe scripts\project_code_metrics.py --mode visibility
.venv\Scripts\python.exe scripts\map.py --mode visibility
```

Quality:

```powershell
.venv\Scripts\python.exe scripts\hotspots.py --mode visibility --paths src --scope all
.venv\Scripts\python.exe scripts\clone_alert.py --mode visibility --paths src
.venv\Scripts\python.exe scripts\rust_escape_hatches.py --mode visibility --paths src
.venv\Scripts\python.exe scripts\locality_bench.py --mode visibility
.venv\Scripts\python.exe scripts\leverage_metrics.py --mode visibility --paths src
```

Performance:

```powershell
.venv\Scripts\python.exe scripts\slowspots.py --mode visibility
.venv\Scripts\python.exe scripts\search_speed.py --mode visibility
.venv\Scripts\python.exe scripts\capacity_report.py --mode visibility
.venv\Scripts\python.exe scripts\resource_profiles.py --mode visibility
.venv\Scripts\python.exe scripts\speed_efficiency_report.py --mode visibility
.venv\Scripts\python.exe scripts\performance_review.py --mode visibility
```

Correctness:

```powershell
.venv\Scripts\python.exe scripts\test_catalog.py --mode visibility
.venv\Scripts\python.exe scripts\test_catalog.py --mode visibility --run
```

Dashboard item refresh from PowerShell:

```powershell
Invoke-WebRequest -Method POST http://127.0.0.1:8000/api/run/item/performance.search
Invoke-WebRequest -Method POST http://127.0.0.1:8000/api/run/category/quality
Invoke-WebRequest -Method POST http://127.0.0.1:8000/api/run/all
```

## Maintenance Notes

- Add or rename dashboard tasks in `scripts/measurement_catalog.py`; then refresh
  `target/analysis/measurement_catalog.json`.
- Add or rename viewer input artifacts in `viewer/data-viewer.js`.
- Add or rename flamegraph profile configs in `scripts/perf_report_shared.py`;
  the generator, performance review, and capacity guidance all read from there.
- Add dashboard run summary metrics in `scripts/dashboard_server.py` when a new
  artifact should appear in run trends.
- Keep measurement scripts decoupled from the viewer. Scripts should write JSON;
  the viewer should interpret the artifacts.
