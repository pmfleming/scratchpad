# Quality Tab — Gaps & Opportunities Review

Date: 2026-05-15
Scope: `viewer/index.html` (Quality section), `viewer/data-viewer.js` (quality-* renderers), and the measurement scripts that feed it (`scripts/hotspots.py`, `type_health.py`, `clone_alert.py`, `rust_escape_hatches.py`, `locality_bench.py`, `leverage_metrics.py`).

The tab is in good shape structurally — six dimensions are surfaced, every dataset has a table, and the locality/leverage quadrant plus the type-health scatter both give useful at-a-glance pictures. The gaps below are mostly about (a) signals the measurement code already collects but the UI throws away, (b) inconsistencies between panels that erode trust in the scores, and (c) a few outright missing dimensions.

---

## 1. Shape & layout gaps

### 1.1 Escape Hatches has no distribution curve
Every other dimension has a risk distribution panel: hotspots, clones, structural (type_health), locality, leverage. Escape Hatches gets a card in the overview row and a dataset table at the bottom, but no curve / counts / worst-items view in the distribution band. The `escape_hatch_score` field is already computed per file (see [rust_escape_hatches.py:131-139](scripts/rust_escape_hatches.py)). Dropping it into a fifth `renderRiskDistribution` call (warn ≈ 20, bad ≈ 50, matching the table's `riskClass` call at [data-viewer.js:579](viewer/data-viewer.js:579)) would close the symmetry.

### 1.2 Overview cards are decorative — no drill-through
[renderQualityOverview](viewer/data-viewer.js:302) builds six rich `.quality-overview-card` blocks ("Records 124 · Files 38 · Worst quality 612 · Large items 14 · Wide structs 5 · Far Dependencies 6 …"). None of them are clickable. The natural drill-throughs are:

- Clicking "Wide structs 5" → flip dataset to `typeHealth` and pre-filter `field_count ≥ 16`.
- Clicking "Cross-file Groups" → switch to `clones` and pre-filter `file_count ≥ 2`.
- Clicking "Unsafe Modules" → switch to `escapeHatches` and pre-filter `unsafe_count > 0`.

The state plumbing already exists (`state.qualityDatasetView`, the filter input boxes). This is wiring, not new infra.

### 1.3 Overview row mixes counts with absent risk totals
Each card except "Maintainability" lacks a single headline risk number. Maintainability shows "612 worst score"; Structure shows "44.0 worst type"; Duplication shows "23 clone groups" (a count, not a risk); Escape Hatches shows "78 uses" (also a count); Locality / Leverage show "N module probes" (just the row count). Show a normalized 0-100 *category risk* on each card so the user can compare dimensions at a glance — the per-dimension `_risk` fields already exist on every record.

---

## 2. Score-trust gaps

### 2.1 Quality score components are not broken down
[hotspots.py:92-98](scripts/hotspots.py:92) computes `quality_score = (cognitive + cyclomatic + maintainability + effort) * 1.12`, but the dashboard only renders the four raw inputs (Cog, Cyc, MI, Halstead Effort) — not the four *capped, weighted* components that actually built the score. A user reading "Quality 612" cannot tell whether it's cognitive-driven (≤260), cyclomatic-driven (≤220), maintainability-driven (≤150), or effort-driven (≤60). Emit the four sub-scores in the JSON and render them as a stacked mini-bar in the row, the way escape-hatch overview already does.

### 2.2 Quality "bad" threshold is unreachable in practice
Maximum quality_score under the current formula is `(260 + 220 + 150 + 60) * 1.12 ≈ 770`. The `bad` cutoff is 600, i.e. ~78% of the saturation ceiling. Combined with caps that bite quickly (MI penalty saturates at 150 once MI < ~-60, which never happens; cognitive saturates at cog=70+), real-world top hotspots cluster in 300-500. The Gaussian curve in [riskDistributionCurve](viewer/data-viewer.js:6666) then sits left of the warn marker and the bad marker looks like a phantom line. Two options:
- Recalibrate `warn`/`bad` empirically from the current corpus (e.g. p90 and p98).
- Or rescale `quality_score` to be 0-100 like every other metric.

The same calibration audit applies to type_health (`structural_risk` caps at 100, signals only fire at field≥16/variant≥12 — see [type_health.py:214-226](scripts/type_health.py:214); but the *distribution-counts* warn/bad of 25/40 is much lower than the signal-firing thresholds, so users see "warn" buckets full of stable types).

### 2.3 Distribution curve assumes a normal distribution
[riskDistributionCurve](viewer/data-viewer.js:6666) fits and draws a Gaussian (`density = exp(-0.5 * ((s - mean) / sd)^2)`). Every quality dimension here is heavy-tailed (most items near zero, a handful in the tail) — the curve is misleading and pulls the eye to a bell that isn't there. Either:
- Drop the curve and lean on the histogram bars (already drawn behind it), or
- Use an empirical kernel-density estimate, or
- Plot the empirical CDF instead — for risk scores the CDF answers the actually-useful question "what fraction of items are above threshold X?" directly.

### 2.4 Locality / Leverage quadrant variable names are inverted
[renderLocalityLeverageQuadrants](viewer/data-viewer.js:4889) sets

```js
const lowLocality = localityRisk(pair.locality) >= 30;   // HIGH risk
const lowLeverage = leverageRisk(pair.leverage) >= 40;   // HIGH risk
```

…then assigns `low-locality-low-leverage` (in the variable sense) the `triage` tone — meaning *high risk on both*. The four quadrant tones (good, local, architecture, triage) and the variable names disagree about what "low" means. Quadrant keys also conflict with the axis labels (`Non-locality risk` / `Leverage risk`, increasing rightward / upward). This is hard to reason about in code and impossible to reason about as a reader of the chart. Rename the variables (`highLocalityRisk`, `highLeverageRisk`) and re-check the four `key=...` assignments.

### 2.5 Locality/leverage/typeHealth distributions all share one mode toggle
[data-viewer.js:5076, 5085, 6324](viewer/data-viewer.js:5076) all pass `modeKey: "qualityDistributionMode"`. Toggling "Counts ↔ Worst items" on the hotspots panel silently flips the type-health, locality, and leverage panels too. Only `cloneDistributionMode` is independent. Either give each its own mode key, or hide the per-panel toggle and add one global toggle in the section heading.

---

## 3. Data already collected, never shown

### 3.1 Clone snippets
`CloneInstance.snippet` is gathered in [clone_alert.py:79](scripts/clone_alert.py:79) and emitted in the JSON, but [renderClones](viewer/data-viewer.js:449) and [renderCloneDetail](viewer/data-viewer.js:6861) only show `file_path:start-end`. The whole point of a clone is "look how similar these are" — a side-by-side or expandable code preview would turn the table from a list of coordinates into something actionable.

### 3.2 Hotspot density metrics
`abc_density` and `complexity_density` are computed in [hotspots.py:86-88](scripts/hotspots.py:86) (score-per-SLOC) but never rendered. A small/intense 40-line function with quality 250 is more interesting than a sprawling 800-line module with quality 280; density is what tells you that. Add a "Density" column or sort option.

### 3.3 Hotspot `bugs` Halstead estimate
`bugs` is collected and asdict'd, then ignored. It's a single defect-likelihood number — even if you don't trust the absolute value, the relative ordering is signal.

### 3.4 Locality `signal_weights`
[locality_bench.py:152](scripts/locality_bench.py:152) emits weighted signals (per-signal numeric contribution to risk). [data-viewer.js:6565-6570](viewer/data-viewer.js:6565) reads `signalWeights` when present — but ONLY in the count-mode signal-bar widget, and only for locality. Type-health, hotspots, escape-hatches and leverage emit unweighted signal strings. Either propagate weights everywhere (per-record `signal_weights: {signal: contribution}`), or remove the special case so the inconsistency stops being a UX puzzle ("why does locality look different?").

### 3.5 Leverage style data is silently optional
[leverage_metrics.py:58-93](scripts/leverage_metrics.py:58) calls `cargo run --bin leverage_ast`. If cargo isn't on PATH or the binary fails, it prints a stderr warning and proceeds with `parse_status="not_measured"`. The dashboard never communicates this — the Leverage card just looks legitimate but is missing `style_leverage_score`, `iterator_method_count`, `heap_allocating_type_count`, etc. Surface a banner / pill ("style data unavailable for N modules") on the Leverage panel when `parse_status != "ok"` is non-zero.

### 3.6 Escape-hatch pattern weights
[rust_escape_hatches.py:17-38](scripts/rust_escape_hatches.py:17) attaches a per-pattern weight (unsafe_block=10, static_mut=14, transmute=12, glob_import=2, …). The dashboard's [escape-hatch-bars](viewer/data-viewer.js:558-563) shows raw counts. A module with 1 `static_mut` (score 14) ranks below a module with 5 glob imports (score 10) on the bar chart, even though the first is the riskier one. Either chart `count × weight`, or add a weighted-score column to the bar.

---

## 4. Missing dimensions

### 4.1 Churn × complexity (classic hotspot map)
The most cited code-archaeology view — "files that are both complex and frequently changed" — isn't anywhere on the Quality tab. Locality has `churn`, hotspots has `quality_score`, joinable by file path. A small scatter (X=churn, Y=quality_score) would surface the highest-leverage refactor targets in one glance and would slot nicely above the Locality/Leverage quadrant.

### 4.2 Trend / delta vs previous run
`runMetricSeries("quality_risk_count")` is consumed by the Overview gauge ([data-viewer.js:4027](viewer/data-viewer.js:4027)) — the data is there. The Quality tab itself shows no Δ vs the last run. "Hotspots 124 (▲ 6 since last run)" on the overview card is a 30-second add and answers the question every reviewer actually asks first.

### 4.3 Measurement coverage / scope alignment
Different tools cover different sets of files. hotspots covers `unit` (file) and `function` records; type_health covers struct/enum declarations only; escape_hatches covers any `.rs` file; locality/leverage cover modules resolved by `ArchitectureMapper`. A user reading the Quality tab has no way to ask "is this module measured everywhere?" A small "coverage matrix" panel (modules × tools, ✓/✗) would also expose silent measurement drift (e.g. a new module that hasn't been picked up by one tool).

### 4.4 Combined per-module quality view
The Map tab cross-cuts by module, but inside the Quality tab there's no module-level rollup that says "module X has 3 hotspots, 2 clones touching it, 1 wide struct, 4 unsafe blocks, locality risk 35, leverage risk 42." Today you have to do that mental join across six dataset tabs. A "by module" pivot — even a simple grouped table — would turn the tab from six lists into a triage queue.

### 4.5 Accepted-risk / ignore list
There's no way to annotate "this hotspot is a parser table, deliberately large" or "this clone is template boilerplate". Without it, the same items dominate every weekly review and dull the signal. A `.quality-ignore.toml` (or simple list of `module::name` keys) consumed by the scripts and surfaced in the UI ("3 items hidden by ignore list") would let the tab focus on regressions.

---

## 5. UX / interaction gaps

### 5.1 Free-text filters only
Every dataset filter is `matchesFilter = JSON.stringify(item).toLowerCase().includes(query)` ([data-viewer.js:250](viewer/data-viewer.js:250)). Useful, but you can't say "risk ≥ 40" or "kind = enum" without typing exact substrings. Either add field-scoped tokens (`risk:>40 kind:enum`) or a small dropdown row beside each dataset for kind/risk-bucket/has-signals.

### 5.2 MI column has no risk class
Hotspots table renders MI as a plain number ([data-viewer.js:285](viewer/data-viewer.js:285)). The script considers `mi < 40` a signal ([hotspots.py:109-110](scripts/hotspots.py:109)), yet the column is uncolored. A `riskClass(item.mi, 60, 40)` (inverted thresholds — lower is worse) would make the column scannable.

### 5.3 Distribution histogram → dataset click-through
Clicking a histogram bar opens a popover with the bin's worst items ([renderRiskDistributionBinPanel](viewer/data-viewer.js:6635)) — nice — but there's no "show all in dataset" link. Add a button at the bottom of the popover that flips `qualityDatasetView` to the matching dataset and pre-fills a score-range filter.

### 5.4 No deep-link / URL state
`state.qualityDatasetView`, expansion keys, filters — all in-memory only. Refresh or a Ctrl-click on a teammate's bookmark loses everything. Mirror the relevant state to `location.hash` so a review can be linked.

### 5.5 Type-health scatter encodes risk via color only
The X (fields/variants), Y (method count), and dot size (impl files) leave risk hidden in a color class. Either let the user pick which dimension is on each axis, or add a "Risk" toggle that re-encodes Y as `structural_risk`.

---

## 6. Cross-cutting / methodology

### 6.1 Six independent risk scales
Quality (0-770ish), clones (0-100ish — but threshold is 40), type_health (0-100), escape_hatch_score (unbounded, table threshold 50), locality_risk (0-100), leverage_risk (0-100). Each panel uses different warn/bad cutoffs. They cannot be compared without effort. Worth standardizing on 0-100 with a documented mapping from raw score → risk percentile, or — more pragmatically — adding a "normalized risk" column to each table that uses the dataset's own p90/p98.

### 6.2 Clone groups aren't related to file SLOC
A 60-token clone in a 30-line file is a different story from a 60-token clone in a 1500-line file. The clones table has token_count and max_line_span but no "% of file duplicated" or "clone density per module". The data needed is in the hotspots `sloc` field for the same file path; a derived column would close the loop.

### 6.3 No category-level "what's improving / worsening"
A future-state vision: each of the six dimensions should answer "is this dimension getting better, worse, or holding?" via a small sparkline and Δ in the overview card. Today the question is invisible unless you open the Run Log tab.

---

## Priority recommendation (effort × impact)

| Tier | Item | Effort | Why |
|------|------|--------|-----|
| **P0 — quick wins** | 2.4 quadrant naming fix; 3.6 weight-aware escape-hatch bars; 5.2 MI column risk class; 3.1 clone snippet preview | Hours each | Pure wiring or rename. Each fixes a "the chart is lying to me" issue. |
| **P0** | 1.1 Escape-hatches distribution curve | < 1 day | Field exists, render path exists, just a 5th call to `renderRiskDistribution`. |
| **P1** | 2.1 hotspot sub-score breakdown; 2.2 quality_score recalibration; 3.4/3.5 weight + parse-status surfacing | 1-2 days | Restores trust in the scores. |
| **P1** | 4.1 Churn × complexity scatter; 4.2 Δ vs last run on cards | 1-2 days | Highest-impact missing dimensions. |
| **P2** | 1.2 overview drill-through; 4.4 by-module pivot; 5.1 scoped filters | Days each | Big UX improvements, larger touch surface. |
| **P2** | 6.1 normalized risk; 6.2 clone density | Larger | Methodology work, needs cross-team alignment on what the scales should mean. |
| **P3** | 4.5 accepted-risk ignore list; 5.4 URL state | Larger / requires policy | Worth doing eventually but not blocking. |
