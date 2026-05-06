# Code Metrics Overview

This document explains the key metrics used by `hotspots.py` to identify complex or hard-to-maintain areas in the codebase.

## Core Formulas
These are the formulas currently used by the scripts and the overview viewer.

- **Complexity score** (`hotspots.py`)
  ```python
  cognitive_component = min(260.0, cognitive * 3.7)
  cyclomatic_component = min(220.0, cyclomatic * 2.0)
  maintainability_component = min(150.0, max(0.0, 65.0 - mi) * 1.2)
  effort_component = min(60.0, log1p(max(0.0, halstead_effort)) * 4.0)
  complexity_score = (
      cognitive_component
      + cyclomatic_component
      + maintainability_component
      + effort_component
  ) * 1.12
  ```
  Each component is capped so no single raw metric can dominate the composite indefinitely. SLOC is reported separately as size context, not folded into this score.

- **Performance score** (`slowspots.py`)
  ```python
  slowspot_score = mean_ms * (1.0 + (std_dev_ns / mean_ns))
  ```

- **Search speed score** (`search_speed.py`)
  ```python
  search_speed_score = ns_per_kb * (1.0 + (std_dev_ns / mean_ns))
  ```
  `ns_per_kb` normalizes a benchmark's mean latency by the total decoded text size scanned.
  The report also distinguishes:
  - `completion`: full-scan latency for the whole scope
  - `first_response`: initial keypress-response latency for a partial-result path where remaining work continues in the background

- **Map performance contribution** (`map.py`)
  ```python
  module_perf_score = mean_ns / 100_000.0
  ```
  The architecture map keeps the **highest** benchmark contribution for each targeted module.

- **Code locality score** (`locality_bench.py`)
  ```python
  dependency_spread = min(
      48.0,
      far_dependencies * 9.0
      + layer_violations * 16.0
      + max(0, outbound_dependencies - 5) * 3.0
      + max(0, inbound_dependencies - 12) * 0.75,
  )
  hidden_coupling = min(24.0, hidden_coupling_count * 8.0)
  interface_penalty = (
      10.0
      if interface_explicitness_ratio < 0.25
      and outbound_dependencies + inbound_dependencies >= 4
      else 0.0
  )
  test_distance = 0.0 if has_inline_tests else 0.5 if has_tests else 1.0
  change_spread = min(18.0, churn / 160.0 + max(0, contributor_count - 3) * 2.0)
  non_locality_risk = min(100.0,
      dependency_spread
      + hidden_coupling
      + interface_penalty
      + test_distance
      + change_spread
  )
  locality_score = 100.0 - non_locality_risk
  ```
  Higher scores mean related code is more locally organized: dependencies stay nearby, architectural layers are respected, state coupling is visible, and public interfaces are explicit. The JSON exposes both `locality_score` and `non_locality_risk` so tables can rank by risk while summaries can still show a positive score.

- **Leverage score** (`leverage_metrics.py`, with AST style counts from `leverage_ast.rs`)
  ```python
  pressure_scale = 0.35 + min(0.65, inbound_dependencies / 6.0 * 0.65)
  reach_score = min(22.0, inbound_dependencies * 2.5 + caller_area_count * 4.0)
  invariant_ratio = public_type_count / max(1, public_type_count + public_function_count)
  invariant_score = min(18.0, public_type_count * 3.0 + invariant_ratio * 8.0)
  leaf_fit_bonus = 14.0 if inbound_dependencies <= 1 and divergence_count == 0 and unsafe_blocks == 0 else 0.0
  ripple_penalty = min(
      24.0,
      max(0.0, avg_cochanged_modules - 2.0) * 1.1
      + max(0, cochanged_module_count - 12) * 0.35,
  ) * pressure_scale
  divergence_penalty = min(28.0, divergence_count * 9.0)
  unsafe_penalty = min(20.0, unsafe_blocks * 4.0)
  surface_penalty = 8.0 if inbound_dependencies >= 3 and public_type_count == 0 and public_function_count >= 6 else 0.0
  leverage_score = clamp(
      68.0
      + reach_score
      + invariant_score
      + leaf_fit_bonus
      - ripple_penalty
      - divergence_penalty
      - unsafe_penalty
      - surface_penalty,
      0.0,
      100.0,
  )
  leverage_risk = 100.0 - leverage_score
  ```
  Lower leverage scores mean a module has a poor tradeoff between shared value and the pressure it creates. Low reach is not itself a defect: self-contained leaf modules receive a fit bonus unless they also show divergence, unsafe surface area, or ripple pressure. The JSON keeps the old AST fields as secondary style evidence: iterator method count, `for` loop count, indirection ratio, heap-allocating type count, and unsafe surface counts.

- **Rust escape hatch score** (`rust_escape_hatches.py`)
  ```python
  escape_hatch_score = sum(weight[kind] * count[kind])
  ```
  This is an audit score, not a general quality penalty. It tracks non-conventional Rust that should stay visible during review: `unsafe`, FFI, `static mut`, `union`, raw borrows, inline assembly, `transmute`, `MaybeUninit`, layout/linkage attributes, and lint suppressions including Clippy `allow`/`expect` attributes.

- **Maintainability risk** (`map.py`)
  ```python
  maintainability_risk = (
      complexity_score
      + min(70.0, sloc * 0.12)
      + min(30.0, public_api_count * 2.5)
      + min(35.0, outbound_dependencies * 4.0 + inbound_dependencies * 1.0)
  )
  ```

- **Change risk** (`map.py`)
  ```python
  change_risk = (
      min(160.0, churn / 12.0)
      + min(100.0, commit_count * 2.5)
      + min(80.0, contributor_count * 14.0)
      + min(90.0, defect_commits * 18.0)
      + (90.0 if not has_test_evidence else 0.0)
  )
  ```

- **Performance risk** (`map.py`)
  ```python
  performance_risk = (
      module_perf_score
      + min(120.0, perf_mean_ms * 2.5)
      + min(90.0, perf_variance * 180.0)
  )
  ```

- **Architectural risk** (`map.py`)
  ```python
  architectural_risk = (
      min(120.0, outbound_dependencies * 10.0)
      + min(120.0, inbound_dependencies * 8.0)
      + min(120.0, layer_violations * 32.0)
      + (110.0 if cycle_member else 0.0)
      + (60.0 if sloc >= 250 else 0.0)
  )
  ```

- **Total risk** (`map.py`)
  ```python
  total_risk = (
      maintainability_risk
      + change_risk
      + performance_risk
      + architectural_risk
  )
  ```
  In the overview map JSON this remains `total_score`.

## 1. Cognitive Complexity
Cognitive Complexity measures how difficult a piece of code is to understand for a human. Unlike Cyclomatic Complexity, it penalizes nested control flows (e.g., nested `if` statements or loops) and rewards clean abstractions.
- **Score Impact:** High (`x4.0` multiplier).
- **Warning Signal:** Triggered when the value is **8** or higher.

## 2. Cyclomatic Complexity
Cyclomatic Complexity measures the number of linearly independent paths through a program's source code. It is a structural metric that counts the number of decision points (like `if`, `while`, `for`, `case`).
- **Score Impact:** Moderate (`x2.5` multiplier).
- **Warning Signal:** Triggered when the value is **12** or higher.

## 3. Maintainability Index (MI)
The Maintainability Index is a composite metric that calculates a score between 0 and 100 representing the relative ease of maintaining the code. It is based on Halstead Volume, Cyclomatic Complexity, and SLOC.
- **Scale:** 100 is excellent; lower is worse.
- **Score Impact:** Inverse penalty (`70 - MI` with `x1.5` multiplier).
- **Warning Signal:** Triggered when the value drops below **40**.

## 4. Effort
Derived from Halstead Complexity Measures, "Effort" estimates the mental time and energy required to understand or implement the logic. It is calculated using the number of unique operators and operands.
- **Score Impact:** Low (Capped at 30.0 points).
- **Warning Signal:** Triggered when the value is **15,000** or higher.

## 5. SLOC (Source Lines of Code)
Source Lines of Code counts the number of physical lines in a file or function, excluding comments and blank lines. While simple, larger functions are statistically more prone to bugs.
- **Score Impact:** Low (Capped at 20.0 points).
- **Warning Signal:** Triggered when the value is **150** or higher.

## 6. Performance (Slowspots)
Performance metrics are collected from Criterion benchmarks to identify slow execution paths.
- **Mean Latency:** The average time taken to execute the benchmarked code. Measured in milliseconds (ms).
- **Standard Deviation:** Indicates the consistency of performance. High variance may suggest unpredictable behavior or external interference.
- **Score Calculation:** The "Slowspot Score" is based on mean latency and weighted by its relative standard deviation:
  ```python
  score = mean_ms * (1.0 + (std_dev_ns / mean_ns))
  ```
- **Map Contribution:** The architecture map converts a benchmark into a module-level performance score with:
  ```python
  module_perf_score = mean_ns / 100_000.0
  ```
  If multiple benchmarks target the same module, the map uses the highest score.
- **Warning Signal:** Triggered when the mean latency exceeds its defined **threshold_ms** (default is 50ms).

## 7. Change Risk Inputs
The architecture map now blends git history and test heuristics into a dedicated change-risk score.
- **Churn:** Sum of added and deleted lines from `git log --numstat` for each module.
- **Commit Count:** Files changed frequently are more likely to keep changing.
- **Contributor Count:** Multi-author modules tend to have more coordination risk.
- **Defect History:** Commit subjects containing terms like `fix`, `bug`, `crash`, or `regress` raise change risk.
- **Test Evidence:** Inline `#[cfg(test)]` blocks or matching files under `tests/` lower change risk.

## 8. Architectural Risk Inputs
The architecture map also estimates structural risk directly from module relationships.
- **Layer Violations:** Dependencies that point "downward" against the intended layering.
- **Circular Dependencies:** Modules participating in a dependency cycle get a strong penalty.
- **Oversized Modules:** Very large modules raise architectural drag even when they are not yet cyclic.
- **Dependency Hub Pressure:** Heavy inbound/outbound coupling raises architectural risk.

## 9. Code Clones (Clone Alert)
Clone Alert identifies redundant code segments that have been copied and pasted, which can lead to "semantic divergence" if one copy is updated while another is not.
- **Type-1 Clones:** Exact copies of code, ignoring whitespace and comments.
- **Type-2 Clones:** Structural copies where identifiers (variables, functions) or literals have been renamed.
- **Detection Method:** Uses a sliding window of normalized tokens (default length is 50 tokens) to find matching sequences.
- **Clone Score:** Calculated based on the number of instances and the length of the clone.
  ```python
  score = (InstanceCount * TokenCount) / 10.0
  ```

## 10. Locality & Leverage
Locality and leverage are static quality measurements surfaced in the Quality Review.
- **Code Locality:** Estimates how locally organized each module is by combining dependency spread, layer violations, hidden coupling, interface explicitness, nearby test evidence, churn, and contributor spread.
- **Leverage:** Architecture analysis that balances module reach and invariant surface against divergence pressure, co-change ripple, and unsafe surface area. AST iterator/indirection counts remain as secondary style evidence.
- **Triage Direction:** Locality and leverage scores are "higher is better"; the dashboard exposes explicit `non_locality_risk` and `leverage_risk` values for worst-first triage.

---

### Analysis Tools
- **hotspots.py:** Analyzes static code complexity and maintainability.
- **slowspots.py:** Analyzes dynamic execution performance and latency.
- **search_speed.py:** Analyzes search scaling across Active, Current, and All scopes, with separate completion and first-response timings.
- **clone_alert.py:** Detects structural and renamed code clones.
- **locality_bench.py:** Emits Code Locality metrics from dependency structure, hidden coupling, interface explicitness, test proximity, and git history.
- **leverage_metrics.py / leverage_ast.rs:** Emit architecture leverage metrics with AST style counts as supporting evidence.
- **map.py:** Aggregates complexity, git history, benchmark, dependency, locality, and leverage data into maintainability, change, performance, architectural, locality, and leverage overlays.

### Overview Viewer
The overview launcher supports fast mode plus explicit refresh scopes:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1
```
Fast mode: uses the existing JSON files under `target/analysis/` and just opens the viewer.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -FullUpdate
```
FullUpdate mode: rebuilds the standard JSON files, then opens the viewer.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -FlamegraphOnly
```
FlamegraphOnly mode: refreshes only the flamegraph index and SVGs before opening the viewer.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -SearchSpeedOnly
```
SearchSpeedOnly mode: refreshes only the dedicated search scaling report.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -CloneOnly
```
CloneOnly mode: refreshes only clone analysis.

The rebuild modes refresh:
- `target/analysis/hotspots.json`
- `target/analysis/slowspots.json`
- `target/analysis/search_speed.json`
- `target/analysis/capacity_report.json`
- `target/analysis/speed_efficiency_report.json`
- `target/analysis/clones.json`
- `target/analysis/locality_metrics.json`
- `target/analysis/leverage_metrics.json`
- `target/analysis/map.json`
- `target/analysis/flamegraphs.json`

It then starts a local HTTP server and opens the viewer under `http://localhost:<port>/viewer/`.
