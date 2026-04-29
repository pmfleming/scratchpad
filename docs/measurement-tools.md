# Scratchpad Measurement Tools

Scratchpad treats measurement as part of the product, not as a side task.

This toolchain exists to document the performance, capacity, resource cost, and maintainability of a safe-by-design Rust Notepad replacement while it evolves.

## Scripts

- `scripts/hotspots.py`: complexity and maintainability analysis
- `scripts/slowspots.py`: benchmark-oriented performance and degradation analysis
- `scripts/search_speed.py`: dedicated search scaling analysis for full-completion speed and first-response latency
- `scripts/capacity_report.py`: threshold sweeps for file size, tabs, splits, and paste ceilings with failure-mode and resource hints
- `scripts/resource_profiles.py`: allocation-heavy, working-set, page-fault, and session-cost probes for large-file, paste, tab-count, and session workloads
- `scripts/speed_efficiency_report.py`: coordinated performance triage that merges latency, flamegraph coverage, and capacity ceilings
- `scripts/clone_alert.py`: token-based clone and duplication analysis
- `scripts/map.py`: architecture map output enriched with dependencies and analysis signals
- `scripts/generate_flamegraphs.py`: flamegraph index generation for dedicated single-workload profile binaries
- `scripts/test_catalog.py`: correctness review catalog for integration and inline tests by architecture layer
- `scripts/measurement_catalog.py`: dashboard task catalog for full, category, subcategory, and item refreshes
- `scripts/dashboard_server.py`: local-only dashboard server with refresh APIs and run logs
- `scripts/ci.ps1`: local and CI entry point for formatting, linting, tests, and analysis checks
- `scripts/open-overview.ps1`: launches the dashboard server and viewer against the analysis output

## Output Location

The analysis scripts write JSON artifacts under `target/analysis/`.

The static viewer under `viewer/` reads those files directly.

Expected artifacts:

- `target/analysis/hotspots.json`
- `target/analysis/slowspots.json`
- `target/analysis/search_speed.json`
- `target/analysis/capacity_report.json`
- `target/analysis/resource_profiles.json`
- `target/analysis/speed_efficiency_report.json`
- `target/analysis/clones.json`
- `target/analysis/map.json`
- `target/analysis/flamegraphs.json`
- `target/analysis/correctness_review.json`
- `target/analysis/test_catalog.json`
- `target/analysis/measurement_catalog.json`
- `target/analysis/measurement_runs.json`

## Common Commands

```powershell
.venv\Scripts\python.exe scripts\hotspots.py --mode cli --paths src --scope all
.venv\Scripts\python.exe scripts\clone_alert.py --mode cli --paths src
.venv\Scripts\python.exe scripts\clone_alert.py --mode analysis --paths src --output target/analysis/clones.json
.venv\Scripts\python.exe scripts\clone_alert.py --mode analysis --paths src --engine all --output target/analysis/clones.json
.venv\Scripts\python.exe scripts\hotspots.py --mode visibility --paths src
.venv\Scripts\python.exe scripts\slowspots.py --mode analysis --skip-bench --output target/analysis/slowspots.json
.venv\Scripts\python.exe scripts\search_speed.py --mode cli --skip-bench
.venv\Scripts\python.exe scripts\search_speed.py --mode analysis --output target/analysis/search_speed.json
.venv\Scripts\python.exe scripts\search_speed.py --mode visibility
.venv\Scripts\python.exe scripts\capacity_report.py --mode visibility
.venv\Scripts\python.exe scripts\resource_profiles.py --mode visibility
.venv\Scripts\python.exe scripts\speed_efficiency_report.py --mode visibility
.venv\Scripts\python.exe scripts\generate_flamegraphs.py --mode cli
.venv\Scripts\python.exe scripts\generate_flamegraphs.py --mode visibility
.venv\Scripts\python.exe scripts\test_catalog.py --mode visibility
.venv\Scripts\python.exe scripts\test_catalog.py --mode visibility --run
.venv\Scripts\python.exe scripts\measurement_catalog.py --mode visibility
cargo flamegraph --dev --bin profile_tab_operations -o target/analysis/flamegraphs/tab_operations_profile.svg
cargo flamegraph --dev --bin profile_tab_tile_layout -o target/analysis/flamegraphs/tab_tile_layout_profile.svg
cargo flamegraph --dev --bin profile_view_navigation -o target/analysis/flamegraphs/view_navigation_profile.svg
cargo flamegraph --dev --bin profile_search_current_app_state -o target/analysis/flamegraphs/search_current_app_state_profile.svg
cargo flamegraph --dev --bin profile_search_all_tabs -o target/analysis/flamegraphs/search_all_tabs_profile.svg
cargo flamegraph --dev --bin profile_search_dispatch -o target/analysis/flamegraphs/search_dispatch_profile.svg
cargo flamegraph --dev --bin profile_document_snapshot -o target/analysis/flamegraphs/document_snapshot_profile.svg
cargo flamegraph --dev --bin profile_viewport_extraction -o target/analysis/flamegraphs/viewport_extraction_profile.svg
cargo flamegraph --dev --bin profile_large_file_scroll -o target/analysis/flamegraphs/large_file_scroll_profile.svg
cargo flamegraph --dev --bin profile_large_file_paste -o target/analysis/flamegraphs/large_file_paste_profile.svg
cargo flamegraph --dev --bin profile_large_file_split -o target/analysis/flamegraphs/large_file_split_profile.svg
cargo bench --bench anchor_storage
.venv\Scripts\python.exe scripts\map.py --mode visibility
.venv\Scripts\python.exe scripts\map.py --refresh --mode visibility
```

## Dashboard

Start the dashboard server:

```powershell
.venv\Scripts\python.exe scripts\dashboard_server.py --port 8000
```

Then browse to `http://localhost:8000/viewer/`.

The dashboard has top-level views for:

- Overview
- Quality Review
- Performance Review
- Correctness Review
- Map
- Run Log

Refresh controls call local API routes exposed by `scripts/dashboard_server.py`:

- `POST /api/run/all`
- `POST /api/run/category/{category}`
- `POST /api/run/item/{id}`
- `GET /api/catalog`
- `GET /api/runs`
- `GET /api/run/{run_id}/log`

## Overview Launcher

Fast mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1
```

Full update mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -FullUpdate
```

Flamegraph-only update mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -FlamegraphOnly
```

Search-speed-only update mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -SearchSpeedOnly
```

Clone-only update mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -CloneOnly
```

Legacy static server mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -LegacyStaticServer
```

## Notes

- The Python tools produce JSON rather than HTML.
- The dashboard is category-first, but the scripts remain the measurement producers.
- `scripts/open-overview.ps1` now starts `scripts/dashboard_server.py` by default so refreshes can be initiated from the dashboard.
- The old static HTTP server path is still available with `-LegacyStaticServer`.
- Quality Review separates SLOC from the hotspot score. SLOC is displayed as size context, while `quality_score` uses cognitive, cyclomatic, maintainability index, and Halstead effort.
- Correctness Review catalogs integration tests under `tests/` and inline tests under `src/`, groups them by architectural layer, and keeps descriptions under 10 words.
- Map nodes include quality, performance, and correctness health fields while retaining dependency layout and existing risk metrics.
- `scripts/search_speed.py` uses the same mode contract as the other Python tools: `cli`, `analysis`, and `visibility`.
- `scripts/generate_flamegraphs.py` writes `target/analysis/flamegraphs.json` and the referenced SVG files under `target/analysis/flamegraphs/`.
- `scripts/capacity_report.py` keeps threshold sweeps out of the ordinary latency leaderboard and records the first unusable ceiling separately.
- `scripts/resource_profiles.py` adds allocation profiling for real file-backed large-file open, large paste into a large buffer, working-set and page-fault tracking while scaling tab count, and session persist/restore cost with hundreds or thousands of tabs.
- `scripts/speed_efficiency_report.py` consumes `slowspots`, `search_speed`, `flamegraphs`, `capacity_report`, and `resource_profiles` to emit a coordinated triage artifact.
- Flamegraph generation now targets dedicated single-entry profile binaries instead of whole Criterion suites, which keeps traces narrower and easier to interpret.
- Recommended single-entry profile series:
	- `profile_tab_operations`: active-tab switching plus reversible tab reordering on a 64-tab, multi-view, loaded workspace
	- `profile_tab_tile_layout`: resize-split and rebalance work on a loaded 16-tile workspace tab
	- `profile_view_navigation`: repeated view switching inside a heavily split tab with both duplicated and distinct buffers
	- `profile_search_current_app_state`: current-workspace-tab search through the full app-state pipeline on a file-heavy tab with extra duplicate views
	- `profile_search_all_tabs`: all-open-tabs search across a many-tab workspace where each tab also has duplicate editor views into the same buffer
	- `profile_search_dispatch`: search target collection and request-building cost before worker-side matching
	- `profile_document_snapshot`: cheap-clone snapshot creation cost on large piece-tree-backed documents
	- `profile_viewport_extraction`: visible-line and overscanned text-window extraction cost for large buffers
	- `profile_large_file_scroll`: headless editor layout and redraw work representative of large-file scroll latency
	- `profile_large_file_paste`: large insert into an already large buffer, including metadata refresh work
	- `profile_large_file_split`: repeated split and rebalance work on large file tiles
- The search-speed dataset separates:
	- Active / Current / All scope modes
	- full completion latency vs first-response latency
	- single-file growth vs aggregate corpus growth
- The viewer is intentionally decoupled from the analysis scripts.
- The current workflow is aimed at local review and CI visibility rather than polished end-user reporting.
- `cargo bench --bench anchor_storage` measures point-anchor insert/remove cost at 1, 10, 100, 1,000, and 10,000 live anchors.

## What The Suite Covers Today

Today the measurement workflow covers:

- allocation profiling for large-file open
- allocation profiling for large paste into a large buffer
- working-set and page-fault tracking while scaling tab count
- real file-backed large-file tests alongside synthetic in-memory probes
- session persist and restore cost with hundreds or thousands of tabs
- snapshot creation baselines for large piece-tree-backed documents
- explicit search request-build and target-collection baselines separate from scan cost
- viewport extraction baselines for piece-tree visible-range reads

Those measurements sit alongside slowspots, search-speed, flamegraphs, clone analysis, and architecture mapping so large-buffer and high-tab regressions are visible before they turn into user-facing failures.
