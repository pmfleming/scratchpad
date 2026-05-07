# Performance Review Page — Improvement Plan

The page's job is to answer one question on first sight: *can the app handle the workloads we promise?* Today it answers that question, but only after the reader walks past five summary grids, three near-identical line charts, a wall of mixed-unit bars, four redundant tile rows, and a triage panel whose pills mostly describe how the row was constructed instead of what the row found.

This plan is long; the page it produces is short. The plan is long because every cut needs to be defended — the data is good, the framing isn't.

---

## Operating principle

> Every visual element on the page must report a *capacity result*. If it reports an *ability to measure* — that we ran a probe, that a profile exists, that N rows were collected — it is metadata and belongs in tooltips, expanded rows, or `<details>`, never in the headline view.

Concretely, this rule rejects: count-tiles whose value is structurally zero, pills that name a measurement type, bars whose width is normalised against unrelated units, and grids that re-state inventory we already counted two sections earlier.

---

## What is on the page now (with the screenshots in mind)

The page renders, top to bottom:

1. **Performance Overview cards** (4): "Search Result", "Worst Latency", "Scale Result", "Memory Result". Mixed units inside each card, all framed pessimistically.
2. **Performance Insights cards** (4): "Worst Budget Misses", "Search Outcomes", "Capacity Breakpoints", "Peak Resource Use". Re-renders most of the values from row 1 with different titles.
3. **Coverage Matrix** with a 7-tile summary grid (`Scenarios 7`, `Covered 7`, `Thin 0`, `Implementations 46`, `Scale targets 0`, `Budget misses 26`, `Failed sources 0`) and a wall of scenario coverage cards. The cards include an `implementation-graph` panel with one bar per measurement (e.g. `File size ceiling sweep 128.0 MB`, `Layout bytes ceiling sweep 32.0 MB`, `document_snapshot_creation_latency/4194304 31.2 ms`, `Profile coverage 3/3 SVGs`) all rendered on per-row normalised tracks — the bar lengths are not comparable.
4. **Promise Board / scenario list** with 7 rows, each ending in a Met/Watch badge and a caption like `7 measurements · 4 misses · 2 ceilings`.
5. **Performance Datasets** segmented control with four panels:
   - **Search**: per-panel summary grid (7 cells), budget bars, three line charts ("Tabs Against Time", "Files Against Time", "File Size Against Time"), dependency multipliers, full table.
   - **Editor & Tabs**: scenario-card grid with normalised evidence bars per scenario, budget bars, summary grid, full table.
   - **Capacity & Resources**: triage snapshot (severity bar + four ranked cards with pills like `search`, `cpu`, `2 scale points`, `search_current_app_state`, `over budget > 140.0ms`, `full-scan latency`, `profile coverage`), `Report summary` 7-tile grid (`Search rows 51 · Editor rows 27 · Tabs / splits 10 · Capacity scenarios 9 · Over budget 7 · Coverage gaps 0 · Near ceilings 7`), capacity table, resource summary 7-tile grid, resource scenarios table, resource samples table.
   - **Flamegraphs**: coverage table, methodology, sidebar + viewer.

The redundancies and ability-pills are visible in your screenshots: `Promise Board` summary repeats with `Report summary`; the implementation-graph bars are decorative; the triage pills `profile coverage`, `2 scale points`, and the bare `search_current_app_state` chip add no information about whether the engine is fast.

---

## Cuts — remove, do not replace

Each item here is justified by the operating principle: it does not report a capacity result.

### C1. Implementation-graph (`renderImplementationGraph`)

In your first screenshot the rows include `File size ceiling sweep 128.0 MB`, `Layout bytes ceiling sweep 32.0 MB`, `File-backed open allocation 128.0 MB peak=128.9 MB elapsed=578.7 ms`, `document_snapshot_creation_latency/4194304 31.2 ms (budget=40.0 ms)`, `scroll_stress_latency/4194304 26.4 ms (budget=16.7 ms)`, `Profile coverage 3/3 SVGs`. The bar lengths are normalised per unit (`maxByUnit`), so a 32 MB sweep and a 128 MB sweep look proportional, while a 31.2 ms latency and a 128 MB capacity do not relate to each other at all. The longest bars are usually the *least pressured* metrics. The whole component should go: the underlying data lives in the implementations table, which can stay behind a collapsed `<details>` for auditors.

### C2. Both top-level "overview/insights" rows

`renderPerformanceOverview` (Search Result / Worst Latency / Scale Result / Memory Result) and `renderPerformanceInsights` (Worst Budget Misses / Search Outcomes / Capacity Breakpoints / Peak Resource Use) compute the same handful of derivations twice: worst budget row, top resource row, throughput leader, search-over-budget count, ceiling count. Both rows are framed as worst-case. Delete both rows. Their data is reused in the new headline strip (R1) and the existing triage panel.

### C3. Five summary grids

Drop in entirety:

- `#performance-review-summary` (Promise Board count tiles).
- `#speed-report-summary` (Report summary tiles).
- `#search-speed-summary`, `#slowspots-summary` (per-panel inventory tiles).
- `#capacity-report-summary`, `#resource-profiles-summary` (per-panel inventory tiles).

These grids exist to count the data we have, not to evaluate the app. Cards reading `0`, `0`, `0` ("Thin", "Scale targets", "Failed sources", "Coverage gaps") in normal operation are visual noise. The few cells that *are* result metrics (`Over budget`, `Near ceilings`, `Worst elapsed`, `Peak working set`, `Best throughput`) move into the headline strip or into one-line captions per panel.

### C4. Per-scenario evidence bars in the Editor & Tabs scenario grid

`renderPerformanceScenarios` draws three normalised bars per scenario (Speed / Capacity / Resource) where the bar widths are coverage counts dressed up as performance. Promote the Met/Watch badge from the Promise Board to be the only per-scenario indicator and delete the bars. Whatever the bars used to show is captured by the existing table immediately below.

### C5. Ability-pills in the triage cards

In your screenshot, card 1 shows `search`, `cpu`, `2 scale points`, `search_current_app_state`, `over budget > 140.0ms`, `full-scan latency`, `profile coverage`. Card 3 shows `viewport`, `cpu`, `viewport_extraction`, `slow > 16.7ms`, `profile coverage`. Of these:

- **Keep** (report a result): family chip (`search`, `viewport`), suspected resource (`cpu`), threshold pill (`over budget > 140 ms`, `slow > 16.7ms`).
- **Drop** (reference an ability or duplicate the title): `2 scale points`, `search_current_app_state`, `viewport_extraction`, `full-scan latency`, `profile coverage`. If the user wants the underlying scenario id, that's the title; if they want the profile, the row needs a single "open profile" link, not a pill that says profile coverage exists.

### C6. The four overview/insights "cards" CSS class chain

`performance-overview`, `performance-insights`, `app-package-insights performance-insights` — all gone with C2. Recover the vertical space.

### C7. Triage rank score number

The bold `37.26`, `32.12`, `31.8`, `31.58` shown to the right of each triage card title is `rank_score` — a unit-free composite. It steals the space the title needs (your screenshot truncates `Current App State Complet…`). Delete the score from the header; if it is needed for sorting, surface it on hover.

---

## Merges — same data, fewer surfaces

### M1. Five summary grids → one **headline strip**

A single 5-cell strip directly under the `<h2>Performance Review</h2>`. Cells, in order, with values pulled from existing payloads:

1. `Scenarios met 2/7` (from `performanceReview.scenarios[].coverage_status` mapped to met/watch/miss).
2. `Budget misses 26` (from `summary.budget_misses` or recount of `mean_ms > threshold_ms`).
3. `Near ceilings 7` (from `speedReport.summary.near_failure_ceilings`).
4. `Worst latency 37 ms (1.4× budget)` (from worst `searchSpeed`/`slowspots` row, only if `> 1×`; otherwise `All within budget`).
5. `Peak working set 1.2 GB` (from `max(resources.max_working_set_bytes, capacity.peak_working_set_bytes)`).

Every cell is a result. No cell counts measurements. The strip replaces the two top rows (C2) and all five summary grids (C3).

### M2. Promise Board row caption

Today: `7 measurements · 4 misses · 2 ceilings` mixes inventory with result. Replace with `4 over budget · 2 ceilings hit` (omit when both are zero). The Met/Watch badge already conveys overall status; the caption is now strictly the actionable count.

### M3. Three search line charts → one chart with a scope toggle

`Tabs Against Time`, `Files Against Time`, `File Size Against Time` differ only in which scope's `mode`/`scaling_axis` filter applies. Render one `chart-frame`, render one `<select>` or three-button toggle above it (`Tabs · Files · File size`), and re-emit the SVG on toggle change. The dependency-multiplier card stays — it answers a different question.

### M4. Two resource profile tables → one expandable table

`#resource-profiles-table` lists scenarios, `#resource-profiles-samples` lists every sample. Today they sit stacked, the second one ~5× the height of the first. Merge: each scenario row gains a chevron; clicking expands inline to show the sample rows. The samples `<details>` block goes away.

### M5. Editor & Tabs scenario grid → drop, table stays

The scenario-card grid (C4) and the editor table below it cover the same data twice in opposite orientations. Keep the table.

### M6. Per-panel inventory captions

After C3, each remaining panel (Search, Editor & Tabs, Capacity, Resources) gains a one-line caption above its first chart or table summarising what's inside, using only result-shaped numbers:

- Search: `51 rows · 7 over budget · best 412 MB/s`.
- Editor & Tabs: `37 rows · 0 over budget · slowest 31 ms`.
- Capacity: `9 scenarios · 7 ceilings reached · 4 memory-bound`.
- Resources: `peak working set 1.2 GB · worst elapsed 580 ms`.

These replace the deleted summary grids without bringing back card chrome.

---

## Replacements — the small set of new visuals that earn their place

### R1. Headline strip (M1)

Specified above. One row, five plain cells, no card backgrounds, monospace numbers. Result-only.

### R2. Budget-headroom strip — one chart for both Search and Editor

Replaces the existing `#search-budget` and `#editor-budget` `budget-bars` panels and the worst-misses bars in the deleted insights row.

- Single horizontal bar per scenario.
- X-axis: `mean_ms / budget_ms`, fixed range `0 – 2×`, with the `1.0` line drawn.
- Bar fill: green up to `0.6`, amber `0.6 – 1.0`, red `> 1.0`.
- Sorted by ratio descending so the over-budget rows are at the top of the chart.
- Search rows above a divider, Editor rows below — one figure, no panel duplication.

This is the chart that lets a reader answer "where is the pressure?" in one glance.

### R3. Capacity ladder

Replaces the meaningless `value: failed ? 1 : 0.45` bar from `capacityBreakpointRows`.

- One small vertical SVG per capacity scenario in a responsive grid.
- Rungs at every measured scale point.
- Last successful rung in green, first failure rung in red labelled with `failure_mode` (`memory_bound`, `cpu_bound`, etc.).
- Title: scenario label. Caption: `<last_ok> ok → <first_failure> breaks`.

This is the only piece of net-new chart work and the only place the page currently lacks an actionable view of capacity.

### R4. Latency distribution column (in-table)

Add one column, ~80 px wide, to the existing search and editor tables, drawing a tiny whisker per row from `median_ns`, `mean_ns`, `dispersion_ns`. Mean as a dot, median as a tick, ±dispersion as a span, budget as a vertical line. No new panel, no new chart card — the distribution lives where the row lives.

### R5. Promise Board interaction

Existing rows, tightened caption (M2). Make the Met/Watch badge a button that filters the dataset browser below to that scenario's families. Today the Promise Board is the most informative element on the page; making it a navigation surface lets it pull more weight without taking more space.

---

## The page after this plan lands

```
[Performance Review heading + Refresh button]
[Headline strip · 5 cells · result-only]                 ← R1

[Triage snapshot — severity bar + top 4 risk cards]      ← existing, with C5 + C7 cleanup
[Promise Board — 7 scenario rows]                        ← existing, with M2 + R5

[Budget-headroom strip · Search above, Editor below]     ← R2
[Capacity ladder grid]                                   ← R3
[Search line chart with scope toggle] [Dependency card]  ← M3 + existing dependency

[Dataset browser · 4 segments]                           ← existing
  · Search   : caption + table (with R4 column)
  · Editor   : caption + table (with R4 column)
  · Capacity : caption + capacity table
  · Resources: caption + merged expandable table         ← M4
  · Flamegraphs: unchanged

<details> Implementations audit table                    ← from C1's deleted graph
<details> Methodology
```

Everything not in this outline is gone: the two top card rows, all five summary grids, the wall of implementation bars, the editor scenario-card grid, the duplicate resource samples table, the ability-pills, the rank-score numbers.

---

## Implementation order

The order matters: each step removes more page than it adds, so applying them in sequence keeps the page coherent at every commit.

1. **Refactor digest.** Extract one helper `computePerformanceDigest(state)` that returns the five headline strip values plus the per-panel caption values. Both new and old renderers consume it; this avoids double-counting bugs while old surfaces are still alive.
2. **Build R1 headline strip.** Render above the existing two card rows for one commit so the new strip can be reviewed against the live data.
3. **Delete C2** (overview + insights card rows). Strip becomes the headline.
4. **Delete C3** (five summary grids), add **M6** captions.
5. **Build R2 budget-headroom strip**, delete the old `budget-bars` mounts.
6. **Tighten triage cards**: cut C5 pills, drop C7 score, give the title the freed width.
7. **Promise Board cleanup**: M2 caption, R5 click-to-filter wiring.
8. **Apply M3** scope toggle to the search line charts; remove the two extra chart sections.
9. **Apply M4** expandable resource table; remove samples block.
10. **Delete C4** Editor scenario grid.
11. **Delete C1** implementation graph; collapse the implementations table behind `<details>`.
12. **Build R3 Capacity ladder**, replace the capacity insight chart and capacity-breakpoint bars.
13. **Add R4** distribution column to search and editor tables.

After step 13 the page is the outline above and the digest helper is the single source of truth for headline numbers.

---

## Acceptance test

The reader who lands on the tab can answer, without scrolling, in this order:

1. *How many of our promises do we currently keep?* — headline cell 1.
2. *Where is the pressure?* — headline cells 2/3/4 + triage severity bar.
3. *What should I look at first?* — top triage card.
4. *Are any scenarios about to fail?* — Promise Board status badges.

The reader who scrolls once can answer:

5. *Which scenarios eat their budget and which have headroom?* — budget-headroom strip.
6. *How far past target before each capacity scenario broke?* — capacity ladder.
7. *Is search throughput flat or curving?* — search line chart.

Everything else — the full tables, methodology, sample rows, audit measurements — exists, but only for the reader who opens a `<details>` block.

If, at the end, any element on the page is *counting how much data we collected*, instead of *reporting what we found*, it is the next thing to remove.
