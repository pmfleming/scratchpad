# Performance Review Coverage Plan

## Goal

Make the performance review answer the product promise directly:

- load enormous text quickly
- search enormous text and many files quickly
- scroll enormous files quickly
- manipulate large text quickly
- scale to very large file, tab, and view counts

The implementation stays in measurement code only: scripts, generated analysis artifacts, viewer rendering, and measurement docs.

## Current Gaps

- The dashboard presents performance as datasets first. It has scenario cards, but the cards count evidence by broad workload family, which misses capacity rows such as `file_size_ceiling`, `tab_count_ceiling`, and `paste_size_ceiling`.
- The benchmark metadata files under `benches/` were removed, but `scripts/perf_report_shared.py` still depends on them for mapping benchmark keys to workload families, budgets, targets, and resource hints.
- There is no single artifact that says whether the app is covered for GB-class files, more than 10,000 files, more than 10,000 tabs, many views into the same files, and large text mutations.
- Resource, capacity, latency, and flamegraph evidence are rendered separately, so gaps and next measurements are easy to miss.
- Presentation lacks a coverage matrix, scale-target callouts, and a concise list of missing measurements.

## Target Review Model

Add one scenario-first performance artifact:

- `target/analysis/performance_review.json`

It should derive from the existing artifacts:

- `slowspots.json`
- `search_speed.json`
- `capacity_report.json`
- `resource_profiles.json`
- `flamegraphs.json`
- `speed_efficiency_report.json`

The artifact groups evidence into these review scenarios:

- Large files: loading, scrolling, viewport extraction, snapshots, and edits
- Many files: opening, workspace scale, and all-file workflows
- Search: huge files, many files, first response, completion, and dispatch overhead
- Many tabs: opening, switching, reordering, and tab-strip manipulation
- Many views and splits: large numbers of views into loaded buffers
- Large text manipulation: paste, cut, undo, redo, and metadata refresh
- Session persistence and restore: saving and reopening huge workspaces

Each scenario should report:

- latency rows
- capacity rows
- resource rows
- flamegraph coverage
- target scale coverage
- gaps
- opportunities
- recommended next measurement

## Implementation Steps

1. Add built-in benchmark metadata fallback so performance rows stay mapped after `benches/*_targets.json` disappears.
2. Add `scripts/performance_review.py` to build the scenario matrix and gap/opportunity list from existing JSON artifacts.
3. Update `scripts/measurement_catalog.py` so Performance Review refresh emits both the coordinated speed report and the new scenario review.
4. Update the dashboard loader and Performance Review tab to render coverage, gaps, opportunities, and target-scale status.
5. Update dashboard run metrics so history can track coverage gaps and missing scale targets.
6. Verify with lightweight script generation and syntax checks. Avoid expensive benchmarks unless explicitly requested.

## Exit Criteria

- Performance Review has a scenario matrix that calls out GB files, 10,000+ files, 10,000+ tabs, many views, search, scroll, and text mutation coverage.
- Missing evidence is visible without reading raw JSON.
- Existing scripts still work if old benchmark metadata files are absent.
- No Rust app/source behavior is changed.
