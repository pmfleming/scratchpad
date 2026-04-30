# Editor Capacity And Parallelism Plan

Date: 2026-04-30

This plan combines three prior reports:

- `docs/zed-editor-performance-review.md`
- `docs/refterm-scratchpad-application-report.md`
- `docs/parallelism-performance-plan.md`

The goal is to improve Scratchpad's editor capacity by using modern PCs well: many CPU threads, large RAM, fast SSDs, and wide memory bandwidth. The plan is not just "add threads." The plan is to make Scratchpad's document, display, search, and persistence architecture able to feed parallel work efficiently without drowning the UI thread in cloning, flattening, or recomputation.

## Executive Summary

Scratchpad should target a capacity model where interactive latency is proportional to the viewport and active edit region, while background throughput scales with available CPU cores and memory.

The guiding synthesis is:

- From Zed: make the editor incremental almost everywhere. Use summarized trees, snapshots, layered display transforms, and narrow invalidation.
- From refterm: keep the hot path compact and boring. Render from compact display records, cache expensive text work, batch subsystem boundaries, and treat third-party layout APIs as expensive.
- From the parallelism plan: parallelize aggressively only after snapshots and piece-tree-native reads make dispatch cheap enough to benefit from many threads.

The intended end state:

- opening large files starts useful UI work quickly while background workers finish preparation
- scrolling and editing never require full-document layout
- search uses all useful cores across buffers and chunks without expensive UI-thread setup
- save, restore, session persistence, metadata, and previews run from revisioned snapshots
- memory is used intentionally for caches, snapshots, and precomputed display metadata, with bounded budgets and clear eviction

## Rendering Decision

Scratchpad should not maintain separate editor rendering paths.

The viewport-first, snapshot-backed renderer is the only target path. The existing full-document egui `Galley` path should be removed, not preserved as a small-file fallback. Small files should still be fast because the unified renderer has little work to do, not because they bypass the architecture.

This means all editor behavior must work through the same model:

- document snapshots and piece-tree spans
- viewport text slices plus overscan
- display-row records
- cached bounded layout records
- cursor, selection, search, gutter, and hit testing translated through viewport/display metadata

## Execution Policy

This plan should be executed as a fast, breaking migration, not a careful compatibility refactor.

Rules:

- move fast and break old editor assumptions
- delete the full-document galley path instead of wrapping it
- delete tests that only protect removed full-document behavior
- rewrite tests around the new viewport-first behavior when the new behavior exists
- allow interim commits/stages to be non-working while the migration is in flight
- keep one path only; do not add fallbacks, compatibility shims, or dual-renderer branches
- prefer obvious deletion over adapter layers when old code blocks the new architecture

The project should tolerate temporary red tests during the migration. The final checkpoint for the migration is not "all old tests pass"; it is "the old architecture is gone and the new single path has its own focused coverage."

## Capacity Goal

Scratchpad should become limited primarily by machine resources, not by single-threaded whole-buffer paths.

Practical targets:

- small files stay simple and instant on the same viewport-first path
- medium and large files use the same snapshot-backed rendering pipeline
- many open files can be searched, restored, and persisted using bounded fanout
- background workers should keep available cores busy when the work is large enough
- RAM should be used to cache useful summaries and layout records, not accidental duplicate `String` copies

The key product rule: the UI thread owns interaction and final state mutation, but it should not own heavy preparation.

## Architecture Principles

## 1. Incremental first, parallel second

Parallelism cannot rescue a path that first copies or lays out the entire document on the UI thread.

Every high-capacity workflow should be designed in this order:

1. avoid whole-buffer work
2. expose stable snapshots or bounded slices
3. split naturally independent work into coarse tasks
4. run those tasks on bounded workers
5. merge only current revision results back into UI state

This keeps the machine busy without turning the frame loop into a scheduler bottleneck.

## 2. The piece tree is the capacity engine

The piece tree should become the primary surface for:

- edits
- line lookup
- char and byte coordinate conversion
- range iteration
- visible-window extraction
- search snapshots
- save snapshots
- preview generation
- display metadata construction

Flattening to one contiguous string should be an explicit boundary operation, not the default internal representation.

Allowed flattening boundaries:

- final save/write buffer when required by an API
- clipboard export
- tests and diagnostics
- external integration points that demand owned text

## 3. Snapshots are the bridge to threads

Background work should consume immutable, revisioned snapshots.

A useful document snapshot must be:

- cheap to clone
- safe to send to workers
- tied to a document revision
- backed by shared storage, not copied full text
- able to iterate spans and bounded ranges
- able to answer line/offset metadata queries
- rejectable when stale

This is the central enabling abstraction for modern-PC utilization. Without it, extra threads merely move unnecessary copying around.

## 4. Display should be compact records plus cached layout

Zed's display model suggests layered transforms and narrow invalidation. Refterm suggests compact display records and cached expensive text work.

For Scratchpad, the combined target is:

- piece-tree text storage
- document snapshot reads
- display-row records for row metadata
- visible text slices for current viewport plus overscan
- cached line/display-row layout records
- egui painting from bounded, already-prepared inputs

The full-document egui `Galley` should be removed from the editor rendering path. Any egui layout that remains should be fed from bounded viewport slices.

## 5. Use memory deliberately

Modern PCs often have enough RAM to improve responsiveness, but only if memory is spent on reusable structure rather than duplicate full strings.

Good memory use:

- shared immutable source buffers
- piece-tree summaries
- row and wrap metadata
- layout caches for visible and nearby rows
- search indexes or chunk metadata where measured useful
- background-prepared snapshots for tabs likely to be used soon

Bad memory use:

- repeated full-document `String` clones
- per-worker copies of the same buffer
- full-document galley layout for hidden content
- unbounded caches with no revision or viewport awareness

## 6. Parallelize coarse, independent work

The app should parallelize as much as possible where work is large, independent, and snapshot-backed.

Good parallel units:

- independent files during open
- independent buffers during search
- large-document chunks during search or metadata scans
- background piece-tree construction
- display metadata building for cold regions
- deferred artifact inspection
- save/session serialization from snapshots
- restore preparation by tab or buffer

Poor parallel units:

- individual keystrokes
- tiny line operations
- per-match UI updates
- tasks that immediately need UI locks
- tasks that require full text cloning before dispatch

## Target System Shape

## Foreground UI Thread

Responsibilities:

- user input
- egui frame orchestration
- live document mutation
- cursor/selection state
- installing completed worker results
- rendering visible content
- rejecting stale worker results

Constraints:

- no blocking file IO in ordinary interaction
- no full-document layout for editor rendering
- no wide search dispatch that clones text
- no whole-document metadata scans after large edits

## Background Worker Pools

Use bounded pools with task categories rather than a single unstructured queue.

Suggested lanes:

- `interactive`: urgent preparation for the active buffer and visible viewport
- `search`: current query work across buffers/chunks
- `io`: file read, decode, save, and session persistence
- `restore`: startup and cold-tab preparation
- `analysis`: artifact detection, metadata, previews, future indexing

Scheduling rules:

- active viewport work outranks cold restore
- current search generation cancels or supersedes previous generations
- IO tasks are bounded to avoid SSD and memory pressure spikes
- background analysis yields to interactive and search work
- every result carries a revision/generation token

## Memory Budgeting

Add a central budget policy for editor caches and prepared state.

Track:

- total document backing storage
- snapshot count and age
- layout cache memory
- display metadata memory
- search/index metadata memory
- pending worker result memory

Evict by:

- document revision
- tab recency
- viewport distance
- cache hit rate
- memory pressure

The goal is to use large memory capacity for lower latency, while keeping behavior predictable on smaller machines.

## Implementation Plan

Implementation is allowed to pass through broken intermediate states.

During this migration, phases are directional demolition-and-rebuild steps, not independently shippable compatibility layers. If a phase removes the old full-document renderer before every feature is rebuilt, that is acceptable. Temporary failures should be tracked, but not used as a reason to keep fallback code alive.

## Phase 0. Capacity Measurement Baseline

Add measurements that distinguish CPU, memory, and UI-thread pressure.

Track:

- full text bytes flattened per workflow
- bytes submitted to egui layout per frame
- layout job count and layout time
- visible slice size versus document size
- UI-thread frame stall time
- worker queue depth by category
- worker active time and idle time
- search first-result and completion latency
- file open time split into read, decode, piece-tree build, install, and first paint
- save/session persist time split into snapshot, flatten, write, and commit
- peak working set and cache memory

Exit criteria:

- reports can show whether a change reduced whole-buffer work, improved core utilization, or merely moved time between phases

## Phase 1. Harden The Piece Tree For Capacity

Goal:

- make the piece tree safe enough to be the capacity backbone

Work:

- expand randomized edit-sequence validation against `String`
- verify line/column and char/byte invariants after large edit histories
- audit balancing under repeated local edits
- add metadata needed for viewport and search chunking
- ensure range iteration can return spans without allocation
- keep ASCII flags and newline counts accurate through edits

Exit criteria:

- piece-tree reads are trusted by search, display, save, and metadata workers
- whole-document extraction is no longer needed for correctness in ordinary internal paths

## Phase 2. Make Snapshots Cheap And Worker-Safe

Goal:

- allow workers to read document state without blocking or copying the live document

Work:

- ensure `DocumentSnapshot` shares backing buffers
- expose span iterators and bounded extraction APIs on snapshots
- expose line/chunk metadata from snapshots
- include revision identity in every worker request
- add stale-result rejection at every result installation point

Exit criteria:

- search, save, preview, metadata, and display-prep tasks can all start from cheap snapshots

## Phase 3. Build The Viewport-First Layout Path

Goal:

- replace full-document editor rendering with visible rows plus overscan

Work:

- delete full-document galley rendering as an editor architecture, even before every feature is fully restored
- introduce a `ViewportTextSlice` built from piece-tree line metadata
- build egui layout only for the visible slice and overscan
- translate cursor, selection, search highlights, and hit testing between document offsets and slice-local offsets
- add display-row records independent of full-document galley construction
- delete or rewrite tests that assume full-document galley behavior
- rebuild editing, selection, search highlights, gutter rows, cursor reveal, and hit testing on the single viewport path

Display-row record shape:

```text
DisplayRowRecord
  logical_line: u32
  char_range: Range<u32>
  y_top: f32
  height: f32
  wrap_index: u16
  flags: ascii / non_ascii / has_selection / has_search / long_line
```

Exit criteria:

- all files scroll and paint without full-document layout
- bytes submitted to egui layout per frame scale with viewport size
- the editor no longer has a full-document egui `Galley` render path
- no tests remain whose purpose is to preserve the removed full-document editor renderer

## Phase 4. Cache Expensive Text Layout

Goal:

- spend memory to avoid repeated layout work

Work:

- cache plain line/display-row layout records by document revision, line/range, font, wrap width, and theme
- keep volatile overlays such as cursor, active search result, and selection as cheap paint/update layers where possible
- warm nearby viewport rows in the `interactive` worker lane
- evict cache entries by revision, viewport distance, and memory budget

Exit criteria:

- repeated scrolling over the same region has high layout-cache hit rates
- selection and cursor movement do not rebuild unrelated layout

## Phase 5. Parallel Search Across Buffers And Chunks

Goal:

- use many cores for search without expensive UI-thread dispatch

Work:

- build search requests from snapshot handles and chunk descriptors
- split large buffers into coarse chunks aligned to line or piece boundaries
- search independent buffers/chunks in parallel
- stream partial result groups when it improves first-result latency
- preserve deterministic display order
- cancel or reject stale generations cheaply
- avoid flattening each target into a temporary full string

Exit criteria:

- all-tabs and active-workspace searches scale with available cores up to memory bandwidth or regex limits
- first useful result arrives before full completion on large scopes
- UI-thread dispatch cost stays small relative to worker scan cost

## Phase 6. Parallelize File Open, Restore, Save, And Persistence

Goal:

- make IO-heavy workflows exploit SSD throughput and background CPU without blocking interaction

Open:

- read files on bounded IO workers
- decode in parallel where independent
- build piece trees and initial metadata off-thread
- install ready documents on the UI thread in deterministic order
- prioritize active/opened-first documents

Restore:

- parse session state quickly
- prepare active tab first
- prepare cold tabs in background batches
- keep memory budget aware of how many tabs are warmed

Save and session persistence:

- snapshot on UI thread
- flatten only at write boundary
- write on IO workers
- coalesce repeated session persistence
- keep explicit synchronous flush for close or crash-critical boundaries

Exit criteria:

- large opens show progressive readiness
- session persistence does not stall normal interaction
- many-file restore benefits from parallel preparation

## Phase 7. Split Immediate And Deferred Metadata

Goal:

- prevent large edits from triggering full-document foreground recomputation

Immediate metadata:

- length
- dirty state
- revision
- local line updates needed for cursor and display correctness

Deferred metadata:

- artifact-heavy inspection
- expensive previews
- compliance or encoding-adjacent scans
- global search/index warmup
- cold display-row metadata

Rules:

- deferred work reads snapshots
- results are revision-checked
- repeated requests coalesce
- active viewport metadata outranks cold document metadata

Exit criteria:

- paste and large edit responsiveness is not dominated by whole-document follow-up scans

## Phase 8. Capacity-Aware Backpressure

Goal:

- keep high parallelism from becoming high contention

Work:

- add bounded queues per worker lane
- add task coalescing keys
- add cancellation/generation checks before expensive stages
- track worker pool saturation
- expose debug counters in measurement reports
- prevent unbounded pending results from accumulating

Exit criteria:

- many-core machines get useful parallelism
- smaller machines stay responsive
- stale work does not consume most CPU after rapid user input

## Priority Order

1. Add capacity measurements for flattening, layout bytes, worker utilization, and memory.
2. Harden piece-tree correctness and span iteration.
3. Make snapshots cheap, shared, and worker-safe everywhere read-heavy.
4. Delete the editor's full-document galley renderer and old renderer tests.
5. Build the viewport-first path until it is the only editor path, accepting interim breakage.
6. Add layout/display-row caching with memory budgets.
7. Rebuild search around snapshot chunks and parallel fanout.
8. Move open, restore, save, and persistence to bounded snapshot workers.
9. Split metadata into immediate foreground updates and deferred snapshot scans.
10. Add lane-specific backpressure and cache eviction policy.

This order intentionally keeps architecture ahead of parallelism. The app should first delete the old source of unnecessary work, then use all available cores for the work that remains.

## Success Criteria

The plan is working when:

- the full-document editor renderer is deleted, not hidden
- obsolete tests that defend the old renderer are gone
- UI frame stalls decrease on large files and large sessions
- editor layout work scales with visible rows, not full file size
- there is one editor rendering path, and it is viewport-first and snapshot-backed
- search uses multiple cores on large scopes without high dispatch overhead
- open and restore show progressive readiness instead of long blocking periods
- save and session persistence no longer block normal interaction
- whole-document string flattening becomes rare and visible in reports
- layout cache hit rate improves repeated navigation
- memory growth is explainable by useful caches/snapshots, not accidental duplication

## Design Guardrails

- Keep UI mutation single-threaded.
- Prefer snapshots over locks.
- Prefer coarse tasks over tiny task storms.
- Prefer bounded queues over unbounded throughput optimism.
- Prefer compact metadata over repeated full text.
- Prefer measuring bytes moved and text laid out, not just elapsed time.
- Keep one editor renderer. Simplicity should come from a clean viewport-first design, not from parallel implementations.
- Do not preserve old tests for removed behavior.
- Do not add fallbacks to keep interim stages green.
- Accept temporary non-working states during the migration.

## Bottom Line

Scratchpad should use modern PCs by combining incremental architecture with aggressive but bounded parallelism.

Zed shows the structural model: summarized text, snapshots, layered display transforms, and narrow invalidation. Refterm shows the hot-path model: compact records, cached text work, batched boundaries, and skepticism toward expensive layout APIs. The parallelism plan supplies the execution model: snapshot-backed workers, stale-result rejection, and bounded fanout.

Together, those ideas point to one strategy: make the UI thread small, make document reads structural, make layout viewport-sized, spend memory on reusable caches, and let workers consume cheap snapshots across all available cores.
