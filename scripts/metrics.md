# Code Metrics Overview

This document explains the key metrics used by `hotspots.py` to identify complex or hard-to-maintain areas in the codebase.

## Core Formulas
These are the formulas currently used by the scripts and the overview viewer.

- **Complexity score** (`hotspots.py`)
  ```python
  complexity_score = (
      (cognitive * 4.0)
      + (cyclomatic * 2.5)
      + (max(0.0, 70.0 - mi) * 1.5)
      + min(30.0, effort / 1000.0)
      + min(20.0, sloc / 10.0)
  )
  ```

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

- **Maintainability risk** (`map.py`)
  ```python
  maintainability_risk = (
      complexity_score
      + min(120.0, sloc * 0.22)
      + min(80.0, public_api_count * 7.0)
      + min(90.0, outbound_dependencies * 12.0 + inbound_dependencies * 10.0)
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

---

### Analysis Tools
- **hotspots.py:** Analyzes static code complexity and maintainability.
- **slowspots.py:** Analyzes dynamic execution performance and latency.
- **search_speed.py:** Analyzes search scaling across Active, Current, and All scopes, with separate completion and first-response timings.
- **clone_alert.py:** Detects structural and renamed code clones.
- **map.py:** Aggregates complexity, git history, benchmark, and dependency data into maintainability, change, performance, and architectural risk.

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
- `target/analysis/map.json`
- `target/analysis/flamegraphs.json`

It then starts a local HTTP server and opens the viewer under `http://localhost:<port>/viewer/`.
