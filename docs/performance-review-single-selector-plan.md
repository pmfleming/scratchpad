# Performance Review — Single Promise Selector Plan

## Problem

The Performance Review page now has two interactive promise selectors stacked on top of each other:

1. **Top:** the 7 colored promise tabs (`#performance-promise-board`) — clicking one sets `state.selectedPerformanceScenarioId` and shows the promise sentence + progress cells.
2. **Bottom:** the 7-bucket dataset stack (`#performance-dataset-stack`) — each scenario is a `<details>` that the user can independently open or close.

Both surfaces let the user "pick a promise to focus on", and clicking a top tab also auto-opens the matching bottom bucket. The page works, but the duplication makes it unclear which surface is the navigator and which is the content. Users have to learn two interaction models for the same task.

## Operating principle

> One selector. One detail surface. The selector tells you the state of all 7 promises at a glance; the detail surface shows the full evidence for the one you picked, with everything inside it collapsible so the page stays scannable.

## Decision: keep the top tab strip as the only selector

The top tab strip wins because it is **the** strongest at-a-glance status surface on the page:

- All 7 promises visible without scrolling.
- Color-coded by promise category (`--promise-color`).
- Status pill (met / watch / miss) and pressure caption (`N over budget · M ceilings`) on every tab simultaneously.
- One click switches focus.

The bottom 7-bucket disclosures cannot match this without being expanded, and expanding all of them defeats the purpose of collapsing them. So the bottom collapses to a single panel that follows the active tab.

## Target shape

```
[Performance Review heading + global Refresh button]

[Top: 7 promise tabs — the only selector]              ← unchanged

[Single detail panel for the active promise]
  · Promise sentence (one line)
  · Progress strip — 7 result cells (Status, Observed,
      Target, Budget misses, Ceilings hit, Worst latency,
      Peak working set)
  · Progress Toward Promise (scale checks grid)

  ── Evidence (each section is a <details>, default state per below) ──
  · Latency Tests              [open by default]
  · Capacity & Failure Ceilings [open by default]
  · Resource Profile Scenarios [closed by default]
  · Flamegraph Profiles        [closed by default]
  · Implementations Audit      [closed by default]

  ── Per-section toolbar (filter inputs, scoped to that section) ──
```

The `<details>` inside the panel solve the overwhelm problem the user is concerned about: the panel can hold every dataset for the promise without forcing the user to scroll past it. The most actionable evidence (latency + capacity) is open on first view; supporting evidence (resources, flamegraphs, implementations) is collapsed until needed.

## Why this beats keeping 7 buckets

| Concern | 7 collapsible buckets | 1 panel + collapsible sections |
|---|---|---|
| Selector duplication | Yes — top tabs + bottom buckets | No — top tabs only |
| At-a-glance status of all 7 | Only via top tabs | Only via top tabs (same result) |
| Browse-all evidence in one scroll | Possible (expand all) | Not possible — but rare in practice |
| Page length when collapsed | 7 disclosure rows of chrome | 1 panel header |
| Per-section filter inputs | 1 set per scenario, 7 copies | 1 set, scoped to active scenario |
| Per-promise filter memory | Yes (state keyed by scenario) | Yes (state keyed by scenario, restored on tab change) |
| Per-promise flamegraph memory | Yes | Yes |
| Click model | Click tab OR click `<details>` | Click tab |

The "browse all" use case is the only thing the 7-bucket layout did better, and it is rarely the actual task — the page exists to let the reader answer *"is this one promise pressured?"*, which is a single-promise-at-a-time question.

---

## Concrete changes

### 1. HTML — collapse the dataset stack to one panel

**File:** [viewer/index.html](viewer/index.html)

- Rename `#performance-dataset-stack` → `#performance-promise-detail`.
- Update the heading copy: drop "Performance datasets" + "Evidence below is grouped by the same promises as the tabs above". Replace with no heading at all (the tab strip above is the heading) or a thin divider.
- Keep the global refresh toolbar (`Refresh Search`, `Refresh Slowspots`, `Refresh Capacity`, `Refresh Resources`, `Refresh Flamegraphs`) where it is — those remain global since the underlying artifacts are global.

### 2. JS — replace the stack renderer with a single-scenario renderer

**File:** [viewer/data-viewer.js](viewer/data-viewer.js)

- **Replace** `renderPerformanceDatasetStack` and `renderPerformanceDatasetBucket` with `renderPerformancePromiseDetail(scenario)`. The function renders **one** scenario's full detail panel into `#performance-promise-detail`.
- **Move** the progress strip + scale checks (currently rendered by `renderScenarioPromisePanel` under the tab strip) into this single panel as its header. The top of the page becomes the tab strip + the one-line promise sentence; everything else lives in the bottom panel. (Alternative: leave progress cells under the tab strip and the bottom panel goes straight to evidence. Smaller diff, slightly more split layout. I prefer the move.)
- **Wrap each evidence section in `<details>`** with default open/closed state per the table above. Use the existing renderers untouched: `renderScenarioLatencyEvidence`, `renderScenarioCapacityEvidence`, `renderScenarioResourceEvidence`, `renderScenarioProfileEvidence`, `renderScenarioImplementationEvidence`, `renderScenarioFlamegraphBrowser`.
- **Per-section filter inputs** stay scoped to their section (latency, resources, profiles) — but there is now exactly one set, not seven.

### 3. State

- `state.selectedPerformanceScenarioId` — already exists, becomes the single source of truth for which promise is shown.
- `state.performanceBucketFilters` — keep keyed by scenario id so each promise remembers its filter text when the user tabs back to it.
- `state.selectedFlamegraphsByScenario` — keep, same reason: tabbing back to a promise restores its previously selected flamegraph.
- `state.performanceSectionOpen` — **new**, optional. Keyed by `{ scenarioId, sectionId }` if we want to remember which `<details>` the user expanded per promise. If we skip it, every section returns to its default open/closed state on tab switch — which is fine for a v1.

### 4. Click + scroll wiring

- `selectPerformanceScenarioTab(id, { scroll })` simplifies: drop the `scrollTarget` argument. With the page now much shorter, scrolling on tab click is probably unnecessary — the detail panel sits directly under the tabs. Default to no scroll; the user re-finds context naturally because the tab they just clicked is still in view.
- Delete the bucket open/close plumbing entirely. `<details>` open state inside the panel is native.
- The flamegraph click handler simplifies — it always targets the currently active scenario's panel, so `data-flamegraph-scenario-id` becomes redundant (but harmless to keep).

### 5. Cleanup

The following can be removed once the single-panel layout is in:

- The 7-bucket disclosure summary (`renderPerformanceDatasetBucket`).
- The per-scenario flamegraph dispatch (`scenarios.forEach(renderScenarioFlamegraphs)`) — call once for the active scenario.
- The dual scroll target logic in `selectPerformanceScenarioTab`.
- Any styling rules that only existed to lay out 7 stacked disclosures (e.g. the colored side stripes on every bucket — the active tab already carries the color).

### 6. Styling notes

- The tab strip stays as-is — it is doing its job.
- The single detail panel needs a clear visual association with the active tab. Suggested: thin top border in `--promise-color` of the active scenario, or a small color chip next to the panel title.
- The per-section `<details>` inside the panel should look quieter than the old 7-bucket disclosures — they are sub-sections within one promise, not 7 sibling promises. Use the existing `.disclose` style but at one indent level deeper.

---

## Implementation order

Each step is small, self-contained, and leaves the page coherent.

1. **Add `renderPerformancePromiseDetail(scenario)`** alongside the existing `renderPerformanceDatasetStack`. Mount it into a hidden second container so it can be diff-checked against the live stack.
2. **Move the progress strip + scale checks** into the new function. Verify the top tab panel still shows the promise sentence and nothing else duplicated below.
3. **Wrap evidence sections in `<details>`** with the default-open table from above. Verify per-section filters still work.
4. **Switch the live mount.** Replace `#performance-dataset-stack` with `#performance-promise-detail`, render the new function from `renderPerformanceReviewCoverage` and `renderPerformanceDatasetView`, delete `renderPerformanceDatasetStack` and `renderPerformanceDatasetBucket`.
5. **Simplify click + scroll wiring**: drop `scrollTarget`, drop the bucket open/scroll logic, simplify the flamegraph click handler.
6. **CSS pass**: tweak the `.disclose` styling for the in-panel sub-sections, add the promise-color accent to the panel header.
7. **Optional polish**: persist `<details>` open state per scenario in `state.performanceSectionOpen` if early use shows the default open/closed mix is wrong.

## What this plan does *not* change

- The 7 promise tabs at top — kept verbatim.
- Any of the evidence renderers (`renderScenarioLatencyEvidence`, etc.) — they are reused as-is.
- The global refresh toolbar — those buttons trigger global artifact refreshes and stay where they are.
- Any data layer or scenario payload structure.

The change is purely a UI shape change: 7 selectors → 1, 7 expandable buckets → 1 panel with collapsible sections inside.
