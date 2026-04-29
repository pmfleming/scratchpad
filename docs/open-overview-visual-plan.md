# Open Overview — Visual & Usefulness Plan

Make `viewer/` (launched by `scripts/open-overview.ps1`) a **monitoring dashboard for quality, capacity, and correctness** rather than a stack of similar tables. Reduce the "long list following another long list" pattern that dominates Quality, Performance, and Correctness review tabs today.

This plan complements the existing `docs/measurement-dashboard-improvement-plan.md` (which covers the control-plane, catalog, and run-manifest work) and is intentionally narrow: **layout, visualization, and Overview-as-monitor**.

## Problem Statement

Current state of the dashboard (`viewer/index.html`, `viewer/data-viewer.js`, `viewer/styles.css`, `scripts/open-overview.ps1`, `scripts/dashboard_server.py`):

1. **The Overview tab is shallow.** Four small bar bands (`renderOverviewCharts`) plus four six-row tables that mirror what already lives in deeper tabs. It does not answer "is the app healthy *right now*?" at a glance.
2. **Deep tabs are wall-of-table.** Performance Review stacks ~10 panel-cards, each a `summary-grid` followed by a sortless `<table>`. Same shape repeats for Quality, Correctness, Run Log. There is no progressive disclosure.
3. **No trend or delta visualization.** Every artifact is rendered as "now". `measurement_runs.json` is loaded but only used for run rows. Capacity and search-speed history is invisible.
4. **Low signal on the metrics that matter.** Capacity ceilings, near-budget search benchmarks, failing tests, and risky modules are all *present* in JSON but require a tab change and a scroll to find.
5. **Legacy file-grid loader** (12 file inputs) was redundant with the dashboard server and confused the loader-panel framing. *(Already removed.)*

## Design Principles

- **One question per pane.** Each panel answers a single monitoring question (is search getting slower? are any layers regressing? which modules are red?). No dual-purpose lists.
- **Visual first, table on demand.** Default to a small chart, sparkline, gauge, or risk-grid. Tables go behind a "Show details" disclosure or on the deep-dive tab.
- **Status > raw counts.** Cards lead with `OK / Watch / Regressed / Stale` and a delta vs the previous run, not just a number.
- **Three top-of-page health gauges** on Overview: Quality, Capacity, Correctness. Everything else feeds these.
- **Stable layout grammar.** Three reusable card shapes only: `gauge-card`, `trend-card`, `top-list-card`. Stop hand-rolling sections.

## Target Layout

### Overview tab — "are we healthy?"

A single dense, non-scrolling-on-1080p hero:

| Row | Content |
|---|---|
| 1 (Health row) | Three large `gauge-card`s: **Quality**, **Capacity**, **Correctness**. Each shows current status pill (`OK/Watch/Regressed/Stale`), the dominant signal driver, a delta vs previous run, and a 10-run sparkline. |
| 2 (Risk map row) | Compact treemap or honeycomb of modules colored by `total_score` (reuse `state.map.modules`). Click → Map tab pre-filtered. Replaces the four mini bar bands. |
| 3 (Top concerns row) | Three side-by-side `top-list-card`s: **Top 5 Quality Risks**, **Top 5 Slowest vs Budget**, **Failing or Stale Tests**. Each row links to its deep tab. |
| 4 (Recent runs strip) | Horizontal timeline of last ~10 runs with status dots and durations. Click a dot to load its log inline (replaces the Run Log tab as the primary entry point). |

No more "Overview Quality / Performance / Correctness / Runs" four-table block.

### Quality Review tab

Replace the current "summary cards → big hotspots table → big clones table" stack with a two-pane layout:

- **Left:** quality risk distribution. Histogram of `quality_score` buckets (good/warn/bad), and a stacked bar of *signals* (high-cog, large-sloc, low-mi, halstead-effort) so you see *why* the bad bucket is bad.
- **Right:** unified "worst items" feed mixing hotspots and clones, sorted by score. One row per item with a kind pill and inline signal pills. Filter input narrows; "Show all hotspots" / "Show all clones" buttons swap the right pane to the full table only when needed.

### Performance Review tab — capacity-first

The current ten panel-cards collapse into three navigable sections with anchor pills at the top (Search • Editor & Tabs • Capacity & Resources • Flamegraphs):

- **Search panel:** keep the line/bar charts (already present), but lead with a **budget chart**: each scenario as a horizontal bar showing `mean_ms / threshold_ms` ratio (red > 1.0). Single visual replaces the long search-speed table on first view.
- **Editor & tabs panel:** scenario tiles (one per `workload_family`) with pass/fail vs threshold, dispersion, and the linked flamegraph badge. Defaults to tiles; "Show full benchmark table" expands.
- **Capacity & resources panel:** keep the dedicated capacity table — capacity ceilings *do* deserve table form — but pair it with a small **per-axis ceiling chart** (file size, tab count, etc.) so a regression is visible without reading numbers.
- **Flamegraphs panel:** unchanged sidebar+SVG, just demoted to the bottom anchor.

### Correctness Review tab

- Lead with a **layer matrix**: a grid of architectural layers × test status, cells colored by failure ratio. One look tells you which layer is regressing.
- Below the matrix, the existing `correctness-table` becomes a filterable list under a "Show all tests" disclosure. Default view shows only failed and unknown tests.

### Run Log tab

Becomes a thin tab — most of its value moves to the Overview run strip. Keep it for full log inspection of a selected run.

## Reusable Components To Add

Implemented in `viewer/data-viewer.js` + `viewer/styles.css`. No new dependencies — use inline SVG, just like the existing search-speed charts.

1. `renderGaugeCard(targetId, { title, status, driver, delta, sparkline })` — solid status pill, large headline metric, mini sparkline from `measurement_runs.json` history.
2. `renderRiskTreemap(targetId, modules, metric)` — proportional cells from `state.map.modules`, one click hands off to the Map tab.
3. `renderBudgetBars(targetId, items, { meanKey, budgetKey, labelKey })` — horizontal bars normalized to budget; reused by search-speed and editor scenarios.
4. `renderLayerMatrix(targetId, layers)` — grid of cells with failed/unknown/passed colors.
5. `renderRunStrip(targetId, runs)` — last-N runs as click-able status dots.
6. `disclose(panelEl, label)` — generic "Show details / Hide details" toggle so deep tables can be hidden by default.

`renderOverview` shrinks to a thin orchestrator that calls those component functions on the existing `state.*` artifacts. No new JSON shapes are required for phase 1.

## Data Producers — Small, Targeted Changes

Most changes are viewer-side, but the following back-end changes are needed to feed the gauges and sparklines:

- `scripts/measurement_catalog.py` / `scripts/dashboard_server.py`: in each `measurement_runs.json` entry, capture **headline metrics** from each artifact (e.g. worst quality score, worst search budget ratio, capacity ceiling pass-count, test pass/fail counts). The viewer reads `runs[].metrics.<key>` to draw sparklines without re-fetching old artifacts.
- `scripts/speed_efficiency_report.py`: emit a flat `triage_summary` block with `{ critical, watch, ok }` counts that the Capacity gauge can consume directly.
- `scripts/test_catalog.py`: emit a `layers[].failed_ratio` so the layer matrix can color cells without recomputing.
- `scripts/map.py`: emit `meta.summary.{good, warn, bad}` counts (already partly there) so the Quality gauge has a single number.

No script consolidation, no new artifacts. Each producer adds a few summary fields to the JSON it already writes.

## Removed: Manual Load JSON Backup

The 12 `<input type="file">` controls in the loader panel and the `readJsonFile` handlers in `viewer/data-viewer.js` were a fallback for the era before `scripts/dashboard_server.py` existed. With the local server starting by default from `scripts/open-overview.ps1`, the inputs were dead weight that bloated the loader panel and split the user model ("am I refreshing or uploading?").

**Done in this change:** removed the file-grid markup, the `readJsonFile` function, the twelve `readJsonFile(...)` registrations, the `.file-grid` styles, and the file-grid responsive override. Loader copy now points only at `target/analysis/` and the Refresh controls.

## Phasing

- **Phase 1 — Overview as monitor.** Implement the six reusable components, rewire `renderOverview` to use the new layout, keep all deep tabs untouched. Adds the producer summary fields needed for gauges. Smallest unit of value.
- **Phase 2 — Quality & Performance restructure.** Apply the two-pane Quality view and the three-anchor Performance view. Wrap remaining giant tables in `disclose(...)`.
- **Phase 3 — Correctness matrix & Run strip.** Layer matrix and run timeline; demote the Run Log tab.
- **Phase 4 — Trend persistence.** Backfill historical run metrics so sparklines have more than one data point; add a per-card "Open trend" detail view.

## Out Of Scope

- New measurement scripts or new measurement domains.
- Charting libraries (kept inline-SVG to match the codebase).
- Theming changes beyond reusing existing CSS variables.
- Anything in `assets/architecture_map_viewer.js` — the Map tab keeps its current behavior.
