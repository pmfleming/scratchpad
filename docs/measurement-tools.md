# Measurement Tools

Scratchpad is the application under measurement. The reusable measurement
engines and the dashboard now live in sibling repositories, so this repo should
only keep the Rust code, probes, benches, and small config files that need to
compile against the Scratchpad crate.

This split keeps the editor clean: product code stays here, reusable Rust
quality analysis lives in one lens, Scratchpad-specific performance analysis
lives in another lens, and the dashboard has its own React/Vite app.

## Repository Boundary

Expected sibling checkout layout:

```text
C:\Code\scratchpad
C:\Code\rust-quality-lens
C:\Code\scratchpad-performance-lens
C:\Code\project-management-board
```

Repository pointers:

- Scratchpad app:
  [github.com/pmfleming/scratchpad](https://github.com/pmfleming/scratchpad)
  at `C:\Code\scratchpad`.
- Rust Quality Lens:
  [github.com/pmfleming/rust-quality-lens](https://github.com/pmfleming/rust-quality-lens)
  at `C:\Code\rust-quality-lens`.
- Scratchpad Performance Lens:
  [github.com/pmfleming/scratchpad-performance-lens](https://github.com/pmfleming/scratchpad-performance-lens)
  at `C:\Code\scratchpad-performance-lens`.
- Project Management Board:
  [github.com/pmfleming/project-management-board](https://github.com/pmfleming/project-management-board)
  at `C:\Code\project-management-board`.

## What Each Repo Owns

`scratchpad` owns:

- the Windows text editor application
- Rust probe binaries under `src/bin/`
- Criterion benches under `benches/`
- direct-run lens configs: `rqlens.toml` and `splens.toml`
- application docs that describe the measurement boundary

`rust-quality-lens` owns reusable Rust project analysis:

- complexity hotspots
- clone detection
- escape-hatch and unsafe-surface reporting
- type-health reporting
- correctness catalog generation
- architecture map data
- locality and leverage analysis

`scratchpad-performance-lens` owns Scratchpad-specific measurement producers:

- search speed reports
- slowspot and speed-efficiency reports
- capacity reports
- resource profiles
- frame metrics
- performance review synthesis
- flamegraph indexing
- app-package and project-code helper tools

`project-management-board` owns the dashboard:

- React/TypeScript/Vite app
- `/viewer/` measurement UI
- local dashboard API under `/api/`
- refresh buttons and run orchestration
- measurement task catalog
- run history and logs
- Firebase Hosting build and deploy configuration

## Scratchpad-Owned Measurement Surface

Scratchpad keeps the Rust-side measurement targets that need direct access to
the application crate:

- `src/bin/capacity_probe.rs`
- `src/bin/frame_metrics.rs`
- `src/bin/resource_probe.rs` and its modules
- `src/bin/profile_*.rs`
- `benches/search_speed.rs`
- `benches/frame_budget.rs`
- `benches/search_benchmark_targets.json`

When a new measurement needs to compile against Scratchpad internals, add that
Rust target here. When a new report parses, orchestrates, visualizes, or combines
measurement output, add it to one of the sibling repos instead.

## Artifact Locations

There are two normal artifact locations, depending on how the tools are run.

Direct lens runs from this repo use the checked-in configs:

```text
C:\Code\scratchpad\rqlens.toml
C:\Code\scratchpad\splens.toml
C:\Code\scratchpad\target\analysis\
```

The local dashboard uses generated configs and writes artifacts into the
dashboard repo by default:

```text
C:\Code\project-management-board\target\analysis\.config\rqlens.toml
C:\Code\project-management-board\target\analysis\.config\splens.toml
C:\Code\project-management-board\target\analysis\
```

Those generated configs still point `project_root` at the Scratchpad checkout.
The dashboard is measuring Scratchpad, but it stores dashboard-consumed JSON in
its own `target/analysis` directory unless `PMB_ANALYSIS_ROOT` overrides it.

## Local Dashboard

Use the local dashboard for active work. It can refresh measurements, run tests,
call both lens repos, inspect the Scratchpad app package, and show live run logs.

Start it from the dashboard repo:

```powershell
cd C:\Code\project-management-board
npm install
npm run dev
```

Open:

```text
http://127.0.0.1:5173/
http://127.0.0.1:5173/viewer/
```

Important environment variables:

- `SCRATCHPAD_ROOT`: Scratchpad checkout to measure. Defaults to
  `..\scratchpad` from the dashboard repo.
- `RUST_QUALITY_LENS_ROOT`: quality lens checkout. Defaults to
  `..\rust-quality-lens` beside Scratchpad.
- `SCRATCHPAD_PERFORMANCE_LENS_ROOT`: performance lens checkout. Defaults to
  `..\scratchpad-performance-lens` beside Scratchpad.
- `PMB_ANALYSIS_ROOT`: where dashboard JSON artifacts are read and written.
  Defaults to `project-management-board\target\analysis`.
- `PMB_COMMAND_TIMEOUT_MS`: timeout for dashboard-launched measurement commands.

The local dashboard exposes API endpoints that only make sense on your machine:

- `GET /api/catalog`
- `GET /api/runs`
- `POST /api/run/all`
- `POST /api/run/category/:category`
- `POST /api/run/item/:id`
- `GET /api/run/:id/log`
- `GET /api/app-package`
- `POST /api/app-package/clear-buffers`

Use these controls when you want fresh measurements or when you need to inspect
why a run failed.

## Online Dashboard

The online dashboard is a Firebase Hosting deployment from
`project-management-board`. The configured Firebase site is `thisscratchpad`,
which corresponds to:

```text
https://thisscratchpad.web.app/
https://thisscratchpad.firebaseapp.com/
```

The online version is a static snapshot. It does not run measurements and does
not have the local `/api/` server. During `npm run build`, the dashboard Vite
plugin emits selected files from `project-management-board\target\analysis` into
the built site under:

```text
dist\target\analysis\
```

Firebase then serves those JSON and SVG artifacts as static files. The hosted
viewer detects non-localhost hosts and reads app-package data from the static
`target/analysis/app_package.json` artifact rather than from `/api/app-package`.

Use the online dashboard to share or inspect the latest published snapshot. Use
the local dashboard when you need to refresh data.

## Local vs Online Dashboard

| Capability | Local dashboard | Online dashboard |
| --- | --- | --- |
| Host | Vite dev server on `127.0.0.1` | Firebase Hosting site `thisscratchpad` |
| Data freshness | Can regenerate data on demand | Shows artifacts bundled at build/deploy time |
| Measurement execution | Runs `rqlens`, `splens`, Cargo probes, benches, and tests | Cannot run local commands |
| API routes | Full `/api/` routes are available | `/api/` routes are not available |
| Run logs | Reads live logs from `target/analysis/logs` | Only bundled run metadata is available unless logs are explicitly copied |
| App package | Can inspect and clear local Scratchpad session/package data | Reads static `app_package.json`; cannot clear local data |
| Best use | Development, triage, refresh, debugging | Published read-only snapshot |

If the online dashboard shows stale data, refresh locally first, then rebuild
and deploy the dashboard from `project-management-board`.

## Direct Lens Commands

Run these from `C:\Code\scratchpad` when you want to bypass the dashboard and
write artifacts to Scratchpad's own `target/analysis`.

Quality lens:

```powershell
python -m rust_quality_lens.cli measure --config rqlens.toml
python -m rust_quality_lens.cli measure hotspots --config rqlens.toml
python -m rust_quality_lens.cli measure correctness --config rqlens.toml
python -m rust_quality_lens.cli measure map --config rqlens.toml
```

Performance lens:

```powershell
python -m scratchpad_performance_lens.cli measure all --config splens.toml
python -m scratchpad_performance_lens.cli measure search --config splens.toml
python -m scratchpad_performance_lens.cli measure capacity --config splens.toml
python -m scratchpad_performance_lens.cli measure resources --config splens.toml
python -m scratchpad_performance_lens.cli telemetry --config splens.toml
```

Direct commands are useful for quick checks and CI-like scripting. The dashboard
is usually better for normal review because it keeps run history, logs, task
grouping, and visualization in one place.

## Local Dashboard Checks

Run these from `C:\Code\project-management-board` before publishing dashboard
changes:

```powershell
npm run typecheck
npm run build
```

The build step emits the static dashboard and any available hosted artifacts
into `dist/`.

## Publish Flow

Normal workflow:

1. Refresh measurements locally in `project-management-board`.
2. Confirm the local dashboard shows the intended data.
3. Run `npm run build` in `project-management-board`.
4. Deploy the built dashboard with Firebase Hosting.

Deploy command:

```powershell
firebase deploy --only hosting
```

The Scratchpad repo should not store Firebase credentials, generated dashboard
build output, or dashboard-owned source files.

## Where To Make Changes

- Add or update editor probes and benches in `scratchpad`.
- Add reusable Rust quality producers in `rust-quality-lens`.
- Add Scratchpad-specific performance producers in
  `scratchpad-performance-lens`.
- Add dashboard views, refresh orchestration, Firebase build behavior, or run-log
  UI in `project-management-board`.
- Update this document when repository ownership or artifact flow changes.
