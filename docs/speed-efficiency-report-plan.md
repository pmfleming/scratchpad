# Speed And Efficiency Report Plan

## Purpose

This plan describes how to improve Scratchpad's speed and efficiency reporting without changing code yet.

The immediate goal is to coordinate three existing performance views:

- broad Criterion-based slowdown detection via `scripts/slowspots.py`
- search-specific scaling analysis via `scripts/search_speed.py`
- root-cause profiling via `scripts/generate_flamegraphs.py`

The larger goal is to expand the report so it covers not only current search and tab-motion latency, but also large-file behavior, editing stress, scrolling, and failure ceilings such as maximum tabs, splits, and file sizes.

## Current State

### What already works

- `benches/tab_stress.rs` covers large file load, control-character workflows, tab-count scaling, tile-count scaling, and a tab stress micro benchmark.
- `benches/search_speed.rs` gives a richer search dataset than `slowspots.py`, including:
  - active/current/all scope
  - completion latency vs first-response latency
  - single-file growth vs aggregate corpus growth
- `scripts/generate_flamegraphs.py` already targets dedicated profile entrypoints instead of whole Criterion suites, which is the right direction for readable traces.
- Current profile binaries align reasonably well with search and tab-navigation workflows:
  - `profile_tab_operations`
  - `profile_tab_tile_layout`
  - `profile_view_navigation`
  - `profile_search_current_app_state`
  - `profile_search_all_tabs`

### Coordination gaps

1. `slowspots.py` reads all Criterion output under `target/criterion`, including search benchmarks, but it only loads metadata from `benches/benchmark_targets.json`.
   Result: search rows appear in `slowspots.json` as `unmapped benchmark`, even though `search_speed.py` already knows how to classify them.

2. The flamegraph set is narrower than the slowdown surface.
   Result: search and tab movement have profile coverage, but large-file load, scrolling, paste, and extreme-capacity scenarios do not.

3. There is no explicit policy for when a slow benchmark should trigger a new flamegraph.
   Result: flamegraph creation is manual and memory-based rather than driven by measured pain.

4. The current report is mostly latency-centric.
   Result: it says little about memory growth, capacity ceilings, crash thresholds, or the system resource that actually fails first.

5. The `p95_ns` field in `slowspots.py` is currently populated from Criterion's `median_abs_dev`, which is not a percentile.
   Result: the report should not present that field as a real p95 until the metric semantics are corrected.

## Answer To The Core Question

Yes. Slowspots should inform which flamegraphs to create.

But they should not do so blindly.

The right model is:

- `slowspots` and `search_speed` identify which scenarios are slow, regressing, or unstable
- flamegraphs explain why a chosen scenario is slow
- memory or resource-focused tracing explains why a scenario crashes, thrashes, or degrades without obvious CPU saturation

In other words:

- benchmarks select candidates
- flamegraphs explain hot CPU paths
- resource metrics and memory profiling explain ceilings and failure modes

## External Guidance To Adopt

The plan should explicitly use the following performance practices:

1. Use statistical benchmarking for regression detection.
   Criterion is already the right base for repeated measurement and run-to-run comparison.

2. Keep measured loops focused on the interaction under study.
   Existing profile harnesses already do this well by building state outside the hot loop; new workloads should follow the same rule.

3. Use flamegraphs for hot-path explanation, not for ranking scenarios by themselves.
   Flamegraphs show where time is spent in sampled stacks; they do not replace benchmark thresholds and budgets.

4. Distinguish CPU hot paths from memory growth and resource saturation.
   CPU flamegraphs alone will not explain every crash or scalability ceiling. Memory growth, allocation, page-fault, and system saturation views are separate tools.

5. Evaluate utilization, saturation, and errors for the limiting resource.
   For crash-threshold and scalability work, the report should ask which resource failed first: CPU, memory capacity, page faults, file handles, thread limits, I/O, or application-imposed limits.

## Reporting Model To Move Toward

The speed/efficiency report should be organized into five layers.

### 1. Detection Layer

This is the wide net.

- Keep `slowspots.py` as the broad workflow detector.
- Keep `search_speed.py` as a dedicated specialist report for search.
- Add explicit benchmark family metadata so every benchmark is classified as one of:
  - search
  - large-file-load
  - edit-paste
  - scroll
  - tab-management
  - split-layout
  - capacity-stress
  - control-char or encoding

### 2. Diagnosis Layer

This is where flamegraphs belong.

- Each flamegraph should map to a single benchmark family and a named scenario.
- The report should state which benchmark row triggered or justifies that flamegraph.
- Flamegraphs should exist only for scenarios that are:
  - over budget
  - newly regressed
  - structurally important
  - or ambiguous enough to need root-cause analysis

### 3. Resource Layer

This answers why the app slows or fails.

For each stress family, the report should eventually record:

- elapsed time
- peak memory or working set
- allocation growth if measurable
- page-fault or paging indicators if measurable
- failure mode
- first saturated resource

### 4. Capacity Layer

This covers breaking points.

The report should track ceilings for:

- maximum file size opened successfully
- maximum file size editable without unusable latency
- maximum tab count
- maximum split count or tile count
- maximum split/combine cycle count
- maximum paste size into an already large file

### 5. Decision Layer

This layer turns raw data into priorities.

The report should explicitly answer:

- what is slow now
- what regressed recently
- what is near a failure ceiling
- which slow scenario lacks a flamegraph
- which scenario is CPU-bound vs memory-bound vs structurally bounded

## How Slowspots Should Drive Flamegraph Creation

Adopt a simple selection policy.

### Trigger conditions

A benchmark scenario becomes a flamegraph candidate when any of these are true:

1. Mean latency exceeds its budget.
2. The run-to-run delta crosses a defined regression threshold.
3. Variance is high enough to suggest unstable internal behavior.
4. The scenario is on a top-user-path and is approaching the budget even before a regression.
5. A capacity test shows nonlinear degradation before failure.

### Candidate ranking

Rank flamegraph candidates by:

1. user impact
2. budget overrun size
3. repeatability or stability of the benchmark
4. presence or absence of an existing matching flamegraph
5. whether the likely bottleneck is CPU rather than memory or I/O

### Mapping examples for the current suite

- `tab_stress_operations` should map to `profile_tab_operations`
- `tile_count_scale` should map to `profile_tab_tile_layout`
- `search_current_app_state_completion_aggregate_size` should map to `profile_search_current_app_state`
- `search_all_completion_aggregate_size` should map to `profile_search_all_tabs`
- `view_navigation_profile` currently has no matching broad benchmark family and should either:
  - gain one, or
  - be documented as exploratory profile coverage rather than report-driven coverage

## Workloads Missing From The Current Report

Your ideas are directionally correct and should become first-class workload families.

### Large file workflows

Add or expand report coverage for:

- large file open or load latency
- time to first editable state for a large file
- scroll latency through a large file
- split latency when repeatedly splitting a large file
- paste latency for large inserts into a large file
- post-paste recovery latency such as syntax-independent redraw, search readiness, or undo availability

### Capacity and crash thresholds

Add dedicated stress studies for:

- maximum file size before crash or unusable behavior
- maximum tabs before crash or unusable behavior
- maximum splits before crash or unusable behavior
- maximum repeated split/combine cycles before failure
- maximum repeated paste workload before failure or pathological slowdown

### Failure attribution

Every threshold study should capture:

- exact workload size at first failure
- error text or panic if any
- RSS or working-set trend before failure if measurable
- whether failure was:
  - out of memory
  - allocation pressure
  - paging or page-fault storm
  - excessive object graph growth
  - timeout or unusable UI latency
  - recursion depth or stack limit
  - OS resource exhaustion such as handles or temp files

## Proposed Benchmark Families

This is the benchmark matrix the report should eventually cover.

### Existing families to keep

- search completion latency
- search first-response latency
- large file load
- control-character cleanup and visualization
- tab stress operations
- tab count scaling
- tile count scaling

### New families to add next

- large file scroll latency
- large file repeated split latency
- large file paste latency
- undo after large paste
- search-after-paste latency on a large buffer
- reopen or reload latency for large files
- settings or UI transition latency only if they are on critical paths

### Capacity families to add after that

- file size ceiling sweep
- tab count ceiling sweep
- split count ceiling sweep
- split/combine endurance sweep
- paste size ceiling sweep

## Flamegraph Expansion Plan

The current flamegraphs cover search and tab interaction well enough for a first tier.

Add future flamegraph families in this order:

1. large-file scroll profile
2. large-file paste profile
3. large-file split-heavy profile
4. large-file load profile if open/load latency starts dominating
5. memory-growth or allocation-focused profiling for ceiling failures

Important constraint:

- create CPU flamegraphs only for CPU-bound scenarios
- when a scenario is dominated by memory growth, page faults, or allocator pressure, use memory or allocation profiling instead of adding another CPU flamegraph

## Report Improvements

The report should evolve from a flat list of benchmark rows into a coordinated performance review artifact.

### Section structure

The improved report should include:

1. Summary of current top regressions and budget failures
2. Search section with the existing dedicated scaling view
3. Editor and file-size section
4. Tabs and splits section
5. Capacity and failure ceilings section
6. Flamegraph coverage section
7. Methodology and environment notes

### Per-scenario fields

Each scenario row should eventually include:

- scenario id
- workload family
- workload size label
- mean latency
- budget
- stability indicator
- targeted modules
- matching flamegraph id if present
- last known failure ceiling if relevant
- suspected limiting resource

### Metric hygiene

Before the report is treated as authoritative, fix or clarify:

- `slowspots` classification for search benchmarks
- the mislabeled `p95_ns` field in `slowspots.py`
- whether broad slowspot ranking should include search rows directly or link out to `search_speed` instead

## Recommended Process

Use a three-pass review loop.

### Pass 1: Wide scan

- run the broad report
- run the search-specific report
- identify over-budget, regressing, and high-variance scenarios

### Pass 2: Root cause selection

- pick a small number of top candidates
- decide whether each needs:
  - CPU flamegraph
  - memory or allocation profiling
  - system-resource saturation check

### Pass 3: Capacity review

- run ceiling workloads separately from ordinary latency benchmarks
- record the first failure size and limiting resource
- keep these results out of the normal latency leaderboard so the signal stays clean

## Phased Implementation Plan

### Phase 1: Report coordination

- unify benchmark metadata coverage across `slowspots` and `search_speed`
- add an explicit benchmark-to-flamegraph mapping table
- show which slow scenarios already have profile coverage
- correct or relabel the false `p95` metric

### Phase 2: Missing latency workloads

- add benchmark coverage for scroll, paste, and repeated large-file splitting
- define budgets for those workflows
- add corresponding report sections

### Phase 3: Capacity and failure reporting

- add threshold sweeps for file size, tabs, splits, and paste size
- capture failure mode and limiting resource
- add a dedicated capacity section to the report

### Phase 4: Resource diagnosis

- add memory-growth or allocation profiling guidance for capacity failures
- add a lightweight USE-style checklist for CPU, memory, I/O, and OS resource limits during stress runs

## Concrete Deliverables

The finished reporting system should produce or summarize:

- a broad speed report
- a search-specific scaling report
- a flamegraph coverage index
- a capacity ceiling report
- a short performance triage summary that names the next scenarios to investigate

## Recommended First Changes When Implementation Starts

When code work begins, the first changes should be:

1. stop leaving search benchmarks as unmapped in `slowspots`
2. add a benchmark-to-flamegraph mapping table in the analysis output
3. add scroll and paste workloads for large files
4. separate latency benchmarks from crash-threshold sweeps
5. correct the misleading `p95` label before using it in prioritization

## Expected Outcome

If this plan is followed, the speed and efficiency report will stop being a collection of separate artifacts and become a coordinated workflow:

- benchmarks decide what is slow
- specialized search metrics explain scaling shape
- flamegraphs explain hot code paths
- resource and capacity checks explain why the application eventually fails

That is the right foundation for answering both of these questions with evidence:

- what should we optimize next
- what should we profile next