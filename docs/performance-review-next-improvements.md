# Performance Review — Next Improvements

The performance review tab is now organised the same way as the quality review tab: a small set of readable graphs at the top, then full filterable tables underneath. This plan continues that direction. The aim is to keep the visual surface small while pushing every chart to answer one specific capacity question.

Anchoring observation: the quality tab works because it shows the **same chart shape four times** (one risk-distribution curve per metric: hotspots, clones, locality, leverage), then a **single scatter** to find outliers, then **tables**. The reader learns the curve once and applies it four times. Performance can do the same — but the questions it answers are different from quality's, and the chart's anatomy needs adapting.

---

## What quality does that performance can copy

1. **One repeatable distribution glyph.** Histogram with a smooth density overlay, two threshold markers (warn / bad), a mean dot, and a stats line (`N items · mean X · std Y`). Reused across all four quality metrics so the reader's eye is calibrated.
2. **A counts ↔ worst-items toggle on the same chart.** Counts mode shows bucket totals + the top driver signals as bars. Worst-items mode shows the top-10 ranked feed inline. One panel, two questions.
3. **A quadrant scatter** that turns two metrics into a 2D map with thresholds drawn as cross-hairs, dot size by combined risk, and a side list of the worst combined offenders.
4. **A segmented dataset browser** below the graphs. One table per dataset, every table filterable, no per-table summary tiles.

Performance currently has the third and fourth (in spirit), but no repeatable distribution glyph and no quadrant.

---

## What is *different* about performance — and why we adapt rather than copy

- Quality items have a *score* against an arbitrary scale; performance items have a *ratio against a target* (budget, scale ceiling, working-set limit). The natural distribution is centred on `1.0` (the target), not on the mean.
- Performance has a second axis quality lacks: **scale**. A scenario can be fast at small N and slow at large N. The page must show *how* metrics scale, not just where they currently sit.
- Performance has fewer items than quality (7 promises, ~80 measurement rows total, 9 capacity scenarios). Density curves can be sparse. Some charts work better as ordered strip charts (lollipops) than as histograms.
- Performance has paired metrics whose *interaction* is the diagnosis: latency × scale-growth, peak memory × elapsed time, observed × target. A scatter is the right tool for these.

---

## The four distributions performance should show

Each is a single chart instance of the same component (a "performance distribution glyph"). Each has a counts↔worst-items toggle, the same way quality's curves do. Each chart answers one question.

### D1. Budget pressure
- **Source rows.** All `searchSpeed` and `slowspots` rows that have a `threshold_ms`.
- **Metric.** `mean_ms / budget_ms`.
- **Scale.** Linear x-axis, fixed range `0 – 2×`. Threshold markers at `0.7` (watch) and `1.0` (over). The mean dot sits where the average headroom is.
- **Question answered.** *Is most of the app comfortably under budget, or sitting on top of it?*
- **Counts mode.** Buckets: `< 0.7 healthy · 0.7–1.0 watch · ≥ 1.0 over`. Driver bars: top suspected limiting resources by row count (cpu, memory, io).
- **Worst-items mode.** Top-10 rows by ratio, each with mean / budget / family / link to flamegraph.

### D2. Scaling growth
- **Source.** Existing `calculateDoublingMultiplier` already computes per-series growth. Run it for every scaling-axis series in `searchSpeed` and `slowspots`.
- **Metric.** Doubling multiplier per series.
- **Scale.** Log x-axis, range `0.8× – 4×`. Threshold markers at `1.5×` (sub-linear / good) and `2.2×` (super-linear / bad).
- **Question answered.** *When work doubles, does time double?*
- **Counts mode.** Buckets: `flat (<1.2) · sub-linear (1.2–1.8) · linear (1.8–2.2) · super-linear (≥2.2)`. Driver bars: workload families.
- **Worst-items mode.** Top-10 series by multiplier with their two anchor points labelled (e.g., `paste 1 MB → 16 MB · 2.7× per 2×`).

### D3. Capacity headroom
- **Source.** `capacityReport.scenarios`.
- **Metric.** For not-failed scenarios: `last_successful_value / target_value`. For failed scenarios: `first_failure_value / target_value` (negate sign for ordering, or render as a divergent strip chart).
- **Shape.** Strip / lollipop, not a curve, because there are only 7–9 scenarios. Vertical line at `1.0`. Lollipops to the right of `1.0` are proven past target; to the left are failed at or before target.
- **Question answered.** *Did each capacity promise prove out, and by how much?*
- **Counts mode.** Buckets: `failed below target · failed past target · proven without ceiling`. Driver bars: failure modes (`memory_bound`, `cpu_bound`, `not_reached`).
- **Worst-items mode.** Each scenario as a row with `last_ok → first_failure` and the failure mode pill.

### D4. Resource intensity
- **Source.** `resourceProfiles.scenarios` and `capacityReport.scenarios` with non-zero `parameter_value`.
- **Metric.** `peak_live_bytes / parameter_value` — bytes consumed per unit of work (per file, per row, per MB of input). One value per scenario.
- **Scale.** Log x-axis. Threshold markers at the project's documented per-unit budget; if not set, derive a tentative one as `2× median`.
- **Question answered.** *Is memory linear in the work, or does it amplify?*
- **Counts mode.** Buckets relative to median; driver bars: workload families.
- **Worst-items mode.** Top-10 scenarios by per-unit cost, each with `peak / scale = bytes-per-unit`.

> Why these four and not others: each maps to one of the four shapes of capacity failure — **too slow now**, **too slow at scale**, **breaks before the promised size**, **eats too much memory per unit of work**. Together they cover what "speed capacity" means for this app.

---

## The two scatters performance should add

### S1. Pressure × scaling quadrant
- **Axes.** X = budget pressure (D1 metric, `0–2×`). Y = scaling growth (D2 metric, `0.8–4×`).
- **Threshold lines.** X = 1.0, Y = 2.0. The four quadrants read as:
  - Top-left: "fine today, breaks tomorrow" (in budget, super-linear).
  - Top-right: "actively over and getting worse" (over budget, super-linear).
  - Bottom-right: "over but flat" (over budget, scales OK — likely a constant-factor problem).
  - Bottom-left: "healthy" (in budget, scales OK).
- **Dot size.** Combined `pressure + growth/2`.
- **Side list.** Top-10 worst combined offenders, mirroring quality's quadrant side list.
- **Why.** This is the single most actionable performance picture: it separates "fix now" from "watch" from "redesign before it gets worse".

### S2. Memory × elapsed scatter (resources only)
- **Axes.** X = `max_working_set_bytes` (log). Y = `max_elapsed_ms` (log).
- **Markers.** Vertical at any documented working-set ceiling; horizontal at any elapsed-time budget the resource probes have.
- **Why.** Resource probes are easy to under-use because the table is dense. A scatter pulls outliers (high memory + high time) up visually and gives the reader a single picture of the resource-cost frontier.

These two scatters replace today's per-card "worst" bars with two charts that spend their pixels on locating outliers, not on enumerating them.

---

## What the page becomes (with quality as the template)

```
[Performance Review heading + Refresh]
[Headline strip · 5 result cells]                    ← already planned

[D1 Budget pressure curve]   [D2 Scaling growth curve]
[D3 Capacity headroom strip] [D4 Resource intensity curve]

[S1 Pressure × scaling quadrant]
[S2 Memory × elapsed scatter]

[Promise board · 7 rows · status pills filter the dataset browser]

[Dataset browser · 5 segments · filterable tables only]
  · Search    : table with whisker column
  · Editor    : table with whisker column
  · Capacity  : table
  · Resources : merged expandable table
  · Flamegraphs : list + viewer

<details> Implementations audit
<details> Methodology
```

Note the deliberate symmetry with quality:

- Two paired distribution rows (quality has `Quality / Clones` then `Locality / Leverage`; performance has `Pressure / Scaling` then `Capacity / Resources`).
- One quadrant scatter (quality: locality × leverage; performance: pressure × scaling).
- Datasets browser at the bottom.

The extra S2 scatter is the only place performance leans heavier than quality, and it earns the slot because resources are otherwise hard to read in a table.

---

## Component reuse — write once, instantiate four times

The four distribution charts share enough anatomy that they should be one component, parameterised:

- `metric(item)` — extracts the value plotted.
- `bounds` — `{ min, max, scale: linear | log }`.
- `markers` — `[{ value, kind: "warn" | "bad" | "target" }]`.
- `centre` — `mean` (default) or `1.0` (for ratio metrics) or `null`.
- `mode` — `counts` | `worst`.
- `bucketLabels(value)` — for the counts panel.
- `driverFor(item)` — string used to build the driver bars in counts mode.
- `rowFor(item)` — renderer used for the worst-items feed.

The same component can power D1–D4, the existing quality risk-distribution, and any future metric. **Build the component, not four similar charts.**

The two scatters share anatomy with the existing locality/leverage quadrant — the same component should accept different axes, thresholds, and dot-size functions. **Extend the existing component, do not fork it.**

---

## Cleanups this plan also enables

When the four distributions land, several existing surfaces lose their reason to exist and should be removed in the same pass to keep the page short:

- **Triage panel.** Its severity bar is a worse version of D1's counts-mode buckets; its top-4 cards are a worse version of S1's worst-list. Replace with a simple `Risk register` — top-15 rows ranked by combined `pressure × growth`, sourced from S1's data, rendered as a single feed. No card chrome.
- **Per-panel budget bars** (`#search-budget`, `#editor-budget`). D1 covers both panels in one chart. Delete.
- **Search dependency multiplier card.** Folds into D2 (it is the same data viewed differently). Delete or move into D2's worst-items mode as the default sort.
- **Performance scenario grid** (`#performance-scenarios`). The Promise Board already shows scenario status; the grid duplicates it with bars whose lengths mean nothing. Delete (already in the prior plan but re-flagged here because it now has even less reason to live).
- **Search line charts.** D2 covers the *shape* of scaling. The line charts give the *trace*, which matters for one-off investigation. Demote them: keep the scope-toggle line chart from the prior plan, but move it inside the Search dataset segment (under the table), not above it.

---

## Implementation order

1. Build the **performance distribution component** with the parameter list above. Validate against existing quality risk-distribution data (it should be able to render the existing quality charts as a sanity check).
2. Land **D1 (Budget pressure)** as the first instance. This replaces the current budget-headroom strip and the per-panel budget bars.
3. Land **D2 (Scaling growth)**. Wire `calculateDoublingMultiplier` over every scaling series; expose growth as a per-series field on `searchSpeed`/`slowspots` so D2 and S1 share inputs.
4. Land **D3 (Capacity headroom)** strip, replacing the capacity-ladder section from the prior plan. The ladder is still valuable for one scenario at a time but is heavier than needed for the headline view; demote it inside the Capacity dataset segment.
5. Land **D4 (Resource intensity)**.
6. Build **S1 (Pressure × scaling quadrant)** by extending the locality/leverage quadrant component.
7. Build **S2 (Memory × elapsed scatter)** as a second instance of S1's component.
8. Replace the **Triage panel** with the Risk register feed sourced from S1.
9. Demote the search line charts and the dependency card into the Search dataset segment.
10. Final pass: every panel either shows results (curve, scatter, status) or is a filterable table; no panel counts inventory or describes how a measurement was built.

---

## Acceptance test

A reader landing on the tab can answer in this order, without scrolling:

1. *Are we within budget?* — D1's mean dot vs. the `1.0` line.
2. *Are we scaling well?* — D2's mean dot vs. the `2.0` line.
3. *Did our capacity promises prove out?* — D3's strip vs. the `1.0` line.
4. *Is memory linear in work?* — D4's mean dot vs. the threshold marker.

A reader who scrolls once can answer:

5. *Which scenarios are both close to budget and scaling badly?* — S1's top-right quadrant.
6. *Which resource scenarios are pareto-worst?* — S2's top-right.

A reader who clicks once can answer:

7. *Which exact rows fail D1/D2/D3/D4?* — toggle the chart from counts to worst-items.
8. *Where in the code?* — open the relevant dataset table, every row already linked to its flamegraph.

If a chart on this page does not appear in the answer chain above, it should be moved into a `<details>` block or deleted.
