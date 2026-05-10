# Performance Review Coherence Report

Reviewed on 2026-05-09 against the current Performance Review tab and generated artifacts in `target/analysis`.

## Summary

The tab has moved in the right direction. It is now scenario-first, uses a compact "answer chain", and keeps the detailed evidence behind a selected promise instead of stacking every dataset at once. The page is much easier to approach than the earlier inventory-heavy version.

The remaining opportunity is to make each graph own exactly one decision. Right now the strongest signals are repeated across the pressure scatter, risk register, budget distribution, per-promise latency chart, and latency table. That repetition is understandable, but it can make the page feel busier than the data itself.

## Possible Improvements

### 1. Make the top answer chain more sequential

Keep the current order, but make the visual reading path explicit:

1. Verdict strip: "Are we healthy?"
2. Pressure x scaling scatter: "What must be fixed first?"
3. Four compact distributions: "Which class of risk is dominant?"
4. Promise selector: "Which product promise is affected?"
5. Evidence detail: "What exact rows prove it?"

The current pieces support this flow, but the risk register sits between the scatter and distributions and repeats the scatter side list. Consider making the risk register a compact companion to the scatter only, or moving it below the four distributions as the first drill-down list.

### 2. Replace smooth distribution curves where the sample is sparse

Budget pressure has enough rows for a distribution. Capacity headroom and resource intensity are much sparser and less comparable.

Better graph choices:

- Budget pressure: keep as a distribution, but add a rug or ranked tail so the worst rows are visibly real observations.
- Scaling growth: use an ordered strip or lollipop chart instead of a smoothed curve. The question is "which series grows badly?", not "what is the population shape?"
- Capacity headroom: use a ladder or divergent strip. Separate "proved past target" from "failed after target" so a failure beyond target does not read as a clean success.
- Resource intensity: split by unit or workload family before comparing. Bytes per byte, bytes per file, bytes per tab, and bytes per view should not share one ranked scale without clear faceting.

### 3. Reduce repeated latency evidence

The same over-budget rows currently appear in several forms:

- Pressure x scaling scatter
- Worst combined offenders list
- Budget pressure worst-items mode
- Per-promise latency chart
- Per-promise latency table

Suggested ownership:

- Scatter owns prioritization.
- Budget pressure owns overall budget health.
- Per-promise latency chart owns trend under stress.
- Latency table owns audit detail.

If a row is already in the scatter side list, the risk register can show the next action or owning promise rather than the same score and label again.

### 4. Turn capacity into pass/fail progress, not just ratio

Capacity data is actionable when it shows the last safe rung and first failure rung. A single ratio hides the operational story.

Recommended view:

- One row per capacity promise.
- Target marker always visible.
- Last successful point in green.
- First failure point in red, labelled with `memory`, `cpu`, or `unusable latency`.
- If there is no failure, show "proved through X" rather than treating it like the same category as a failure row.

This would make the seven ceiling hits much easier to act on.

### 5. Keep implementation and coverage data collapsed by default

The current selected-promise detail already helps by folding profiles and implementation audit sections. Keep going in that direction. Coverage counts, implementation counts, and profile availability are useful for trust, but they should stay secondary to results: over budget, failed ceiling, peak memory, and next profile target.

### 6. Add one explicit "next action" per promise

Each selected promise could end with a single generated line:

`Next action: profile Search Current App State at 10K files because it is 21.5x over budget and memory-bound.`

This would make the page more actionable without adding another chart. It also gives the evidence browser a clear reason to exist: it explains the recommendation.

## Highest-Value Next Pass

Do not add more panels. The best next pass is a pruning pass:

1. Make the pressure scatter the sole top-level prioritization view.
2. Convert sparse distributions to ranked strips or ladders.
3. Split resource intensity by unit/family.
4. Keep repeated row-level evidence only inside the selected promise detail.
5. Add one "next action" sentence per promise.

That should make the tab feel less like a performance archive and more like a decision surface.
