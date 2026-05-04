# Locality & Leverage Metrics Dashboard Integration Plan

This document outlines the plan for adding Locality & Leverage metrics to the existing Scratchpad Measurement Dashboard, maintaining the established architectural patterns, UI conventions, and data formats seen in the current Quality and Performance reviews.

## 1. Data Generation & Ingestion (Backend)

The dashboard relies on static JSON artifacts in `target/analysis/`. We will add a new ingestion pipeline to produce these artifacts.

### 1.1 Dynamic Locality Metrics
- **Tooling:** Wrap the existing Criterion (`cargo bench`) suite in `perf stat` during CI to measure cache misses and branch mispredictions.
- **Commands:** `perf stat -e L1-dcache-loads,L1-dcache-load-misses,branch-misses <benchmark_binary>`
- **Artifact:** `target/analysis/locality_metrics.json`
- **Schema:**
  ```json
  [
    {
      "benchmark_name": "tab_stress_operations",
      "l1_miss_ratio": 2.4,
      "branch_mispredict_ratio": 0.8,
      "locality_score": 95.5,
      "signals": ["high branch mispredict"]
    }
  ]
  ```

### 1.2 Static Leverage Metrics
- **Tooling:** A new custom analyzer (similar to `scripts/ast_hasher.rs`) using `syn` to parse AST for indirection and iterator metrics, combined with `cargo-geiger` and `cargo-tree` for ecosystem leverage.
- **Artifact:** `target/analysis/leverage_metrics.json`
- **Schema:**
  ```json
  [
    {
      "module_name": "src/app/text_history.rs",
      "indirection_ratio": 15.2,
      "iterator_leverage_score": 88.0,
      "unsafe_blocks": 0,
      "total_leverage_score": 92.5,
      "signals": ["high indirection"]
    }
  ]
  ```

## 2. Dashboard UI Updates (`viewer/index.html`)

We will add a new primary tab to surface the new data, matching the layout of the existing review pages.

### 2.1 Tab Navigation
Add a new tab button alongside Quality and Performance:
```html
<button class="tab" data-tab="locality-leverage">Locality &amp; Leverage</button>
```

### 2.2 Tab Panel Layout
Create a new section `<section id="locality-leverage" class="tab-panel">` containing:

- **Category Header:** Title and "Refresh Locality/Leverage" actions.
- **Summary Grid:** `<div id="locality-leverage-summary" class="summary-grid"></div>` to display high-level cards (e.g., Average L1 Miss %, Average Iterator Score, Total Unsafe blocks).
- **Distribution Charts:** Two side-by-side `<div class="panel-card">` containers (like the quality layout) showing the Locality Score Distribution and the Leverage Score Distribution. This visually matches the normal distribution/counts segmented controls.
- **Details Tables (Disclose pattern):**
  - **Locality Breakdown Table:** Ranked table of dynamic metrics by benchmark.
  - **Leverage Breakdown Table:** Ranked table of static metrics by module/file.

## 3. Dashboard Logic (`viewer/data-viewer.js`)

We will integrate the new JSON sources and build the corresponding render pipelines.

### 3.1 State & Loading
- Add `locality: `../target/analysis/locality_metrics.json?v=${viewerVersion}`` and `leverage` to the `sources` mapping.
- Add `locality: []` and `leverage: []` to the application `state`.

### 3.2 Render Functions
Create `renderLocalityLeverage()` which invokes:
- **`renderSummary`**: Output summary cards via `metricCard()` (e.g., Worst Locality, Average Leverage, etc.).
- **`renderDistribution`**: Create stacked bar charts/counts for Locality and Leverage scores, similar to the existing `quality-distribution`.
- **`renderTable("locality-table", ...)`**: Map dynamic locality data to table rows, coloring the scores using `riskClass()`.
- **`renderTable("leverage-table", ...)`**: Map static leverage data to table rows, using `<span class="pill">` for signals and coloring thresholds.

### 3.3 Map Integration
- Add "Locality Risk" and "Leverage Risk" as select options in the Architecture Map metric dropdown (`#map-metric`).
- Update the layout logic to visually flag modules suffering from high pointer indirection (low Leverage) or high cache miss rates (low Locality) within the `renderRiskTreemap()` and node rendering.

## 4. Implementation Sequence

1. **Scripts:** Write `scripts/locality_bench.py` and `scripts/leverage_ast.rs` to generate the required JSON files.
2. **Viewer HTML:** Update `viewer/index.html` with the new tab and layout skeleton.
3. **Viewer JS:** Add the fetching, state management, and `renderLocalityLeverage()` logic to `viewer/data-viewer.js`.
4. **Integration:** Update `scripts/report_modes.py` and build pipelines to ensure the new artifacts are published alongside the existing measurement catalogs.
