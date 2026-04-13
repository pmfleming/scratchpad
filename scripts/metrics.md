# Code Metrics Overview

This document explains the key metrics used by `hotspots.py` to identify complex or hard-to-maintain areas in the codebase.

## Core Formulas
These are the formulas currently used by the scripts and the overview viewer:

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

- **Map performance contribution** (`map.py`)
  ```python
  module_perf_score = mean_ns / 100_000.0
  ```
  The architecture map keeps the **highest** benchmark contribution for each targeted module.

- **Impact score** (`map.py`)
  ```python
  impact_score = complexity_score + module_perf_score
  ```
  In the overview map JSON this is emitted as `total_score`.

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

## 7. Code Clones (Clone Alert)
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
- **clone_alert.py:** Detects structural and renamed code clones.
- **map.py:** Combines complexity and performance into module-level impact (`total_score`).

### Overview Viewer
The overview launcher supports three modes:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1
```
Fast mode: uses the existing JSON files under `target/analysis/` and just opens the viewer.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -Refresh
```
Refresh mode: rebuilds the standard JSON files, then opens the viewer.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -CloneCheck
```
CloneCheck mode: rebuilds the JSON files and uses the extended clone check (`--engine all`) before opening the viewer.

The rebuild modes refresh:
- `target/analysis/hotspots.json`
- `target/analysis/slowspots.json`
- `target/analysis/clones.json`
- `target/analysis/map.json`

It then starts a local HTTP server and opens the viewer under `http://localhost:<port>/viewer/`.
