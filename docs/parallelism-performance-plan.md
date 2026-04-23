# Parallelism And Performance Plan

Date: 2026-04-22

## Decision

Scratchpad should treat performance as a two-track implementation effort:

1. finish the piece tree as a production document core
2. parallelize only the workloads that become cheap to dispatch once the document core stops flattening and cloning whole buffers

The old framing was directionally right about UI-thread stalls, search offloading, and synchronous I/O, but it still treated concurrency as the center of the plan.

That is too shallow for the current codebase.

Scratchpad now has a real `PieceTreeLite` implementation in `src/app/domain/buffer/piece_tree.rs`, plus edit, slice, and support modules. That changes the plan. The next bottleneck is no longer "there is no executor." The next bottleneck is "the app still pays too much flattening, copying, and snapshot cost before background work can help."

So the revised priority is:

- make the piece tree safe, cheap, and complete enough to serve as the primary read and write surface
- remove the remaining hot paths that still flatten the piece tree back into whole-buffer strings
- introduce worker-safe document snapshots
- then move search, metadata, persistence, restore, and file pipelines onto bounded background execution

## Executive Summary

Scratchpad is still mostly a single-threaded `eframe` application.

That is acceptable for UI mutation and rendering orchestration.

What is not acceptable anymore is that many expensive operations still cross the UI thread as whole-buffer `String` work even though the app now has a structured piece tree.

Today the codebase has:

- a real piece-tree-based `TextDocument`
- background search execution
- focused measurement tools and profile entrypoints
- capacity and resource probes that already expose large-file and session-scale costs

But it also still has:

- search request building that clones text and even clones the preview tree
- word and cursor helpers that extract large temporary strings
- session and file pipelines that still serialize too much work
- rendering paths that still assume contiguous text for large buffers
- metadata refresh that still does too much whole-document work directly after mutation

The result is that Scratchpad is partially parallelized but not yet architected to benefit from parallelism as much as it should.

The correct next step is not to scatter more background tasks across the app.

The correct next step is to complete the piece-tree implementation enough that:

- reads can be served as slices, iterators, and bounded windows
- snapshots can be shared across threads cheaply
- metadata can be updated incrementally
- search, preview, persistence, and viewport extraction no longer require whole-buffer flattening in ordinary operation

Only after that should broader bounded fanout become the default for search, restore, persistence, and file pipelines.

## What The Previous Plan Got Right

The earlier version identified several real problems correctly:

- too much work still happens on the UI thread
- search is the only clearly intentional background subsystem
- file open, save, restore, and persistence are too synchronous
- large-document rendering is a structural problem, not just a scheduling problem
- bounded concurrency and stale-work rejection will be necessary

Those points remain valid and should be preserved.

## What The Previous Plan Missed

The earlier version was incomplete in four important ways.

### 1. It treated concurrency as the main lever before storage architecture

That is backwards for the current codebase.

Search, preview, visible-range extraction, metadata refresh, and persistence will not become cheap enough just because the app adds a worker pool if the request-building path still clones large `String` values first.

### 2. It did not define a production target for the piece tree

The old plan mentioned copy-heavy paths and large-document work, but it did not define the piece tree as the specific document-core project that should solve those costs.

The repo now needs an explicit document-core plan, not just a concurrency plan with "large-document work" at the end.

### 3. It did not specify a worker-safe snapshot model

This is the biggest missing bridge between piece tree and parallelism.

Search, preview, save, and persistence all need a way to consume stable document state off the UI thread without:

- locking live editor state
- flattening the whole document first
- deep-cloning the underlying buffers per request

Without this, the app can only fake parallelism by moving the back half of the work off the UI thread.

### 4. It did not include the testing burden for a real text engine

A piece tree that sits under editor operations, search, persistence, and rendering needs stronger validation than a few scenario tests.

The plan must include:

- randomized edit-sequence validation
- Unicode and coordinate correctness tests
- metadata and line-lookup invariants
- snapshot correctness under background work

## Current State Review

## 1. App execution model

Scratchpad still runs in the usual `eframe` frame loop.

Relevant code:

- `src/main.rs`
- `src/app/app_state.rs`
- `src/app/app_state/frame.rs`

Implication:

- UI mutation stays single-threaded
- there is still no general shared task system
- any expensive preparation path can still hitch frames directly

That is fine as a UI model.

It is not fine as a data-preparation model.

## 2. The piece tree is now real enough to build on

Relevant code:

- `src/app/domain/buffer/piece_tree.rs`
- `src/app/domain/buffer/piece_tree/edit.rs`
- `src/app/domain/buffer/piece_tree/slice.rs`
- `src/app/domain/buffer/piece_tree/support.rs`
- `src/app/domain/buffer/document.rs`

What already exists:

- append-only original and add buffers
- balanced root, internal-node, and leaf structure
- subtree metadata for bytes, chars, newlines, and piece count
- character-based insert and delete
- piece-slice iteration for bounded extraction
- line and character navigation helpers
- direct integration into `TextDocument`

What is still missing or incomplete:

- broader snapshot coverage that avoids flattening whole search ranges inside worker-side scan paths
- richer incremental metadata for rendering and search-adjacent work
- strong randomized correctness coverage
- broader conversion of editor helpers away from flattening
- a full rendering path that consumes piece-tree windows directly

So the piece tree is no longer a probe, but it is also not yet the complete document core that the rest of the app can safely parallelize around.

## 3. Search is backgrounded, but dispatch is still too expensive

Relevant code:

- `src/app/app_state/search_state/runtime.rs`
- `src/app/app_state/search_state/worker.rs`
- `src/app/domain/buffer/state.rs`

What works:

- matching work is offloaded
- search targets already carry revisioned `DocumentSnapshot` handles instead of cloned preview trees
- bounded target fanout exists for wider current-scope and all-tabs search requests
- generation-based cancellation exists
- stale work is already rejected

What still costs too much:

- target preparation happens before worker dispatch
- worker-side scan still flattens each target into owned text before matching
- result delivery still waits for full-request completion rather than emitting partial groups as they land

This means search is backgrounded, but not yet cheap to launch and not yet able to exploit wide-scope parallelism cleanly.

That same pattern is the warning sign for the rest of the app:

- backgrounding work helps only after the piece-tree and snapshot path stop paying avoidable flattening costs up front

## 4. File, session, and restore flows are still too synchronous

Relevant code:

- `src/app/services/file_controller/open.rs`
- `src/app/services/file_controller/save.rs`
- `src/app/services/file_service.rs`
- `src/app/services/session_manager.rs`
- `src/app/services/session_store/mod.rs`
- `src/app/app_state/startup_state.rs`

The old diagnosis still holds:

- file open and decode do too much blocking work
- save and session persistence are too synchronous
- restore is too serialized
- repeated interactive events can trigger too much repeated work

The difference now is that these flows should be redesigned around stable document snapshots, not just moved into ad hoc worker threads.

The most important concurrency opportunity is therefore not "add more workers" in the abstract.

It is:

- move file read and decode off-thread
- move initial piece-tree construction and install-ready buffer preparation off-thread
- stage restore so active content is prepared first and cold content waits behind bounded background budgeting
- move normal session persistence off the UI path

## 5. Large-document rendering is still structurally limited

Relevant code:

- `src/app/ui/editor_content/text_edit.rs`
- `src/app/ui/editor_content/native_editor`
- `src/profile.rs`

The core constraint remains:

- a whole-document text path cannot be rescued by more threads alone

The document core and the rendering path now need to meet in the middle:

- the piece tree should provide visible-range extraction and coordinate services
- the renderer should stop demanding full-buffer contiguous text for large documents

Concrete current examples worth planning against:

- `src/app/ui/editor_content/text_edit.rs` still starts editable rendering from `buffer.document().extract_text()`
- `src/app/ui/editor_content/native_editor/mod.rs` still has a whole-document extraction path for active editing

## Primary Goal

The goal of this plan is not to maximize concurrency.

The goal is to reduce user-visible latency and memory churn by making document access incremental first, then parallel where appropriate.

In concrete terms, Scratchpad should end this effort with:

- a production-grade piece-tree document core
- cheap immutable document snapshots for background consumers
- bounded background execution for naturally independent work
- lower UI-thread stalls during search, open, save, restore, and persistence
- a large-document editor path that avoids whole-buffer layout and flattening

## Non-Goals

This plan should not:

- add an async runtime just for fashion
- move UI mutation off the main thread
- parallelize tiny operations that are cheaper inline
- hide whole-buffer copying behind worker threads and call that solved
- treat piece tree as an isolated storage experiment separate from actual app behavior

## Architecture Direction

## 1. Piece tree becomes the primary document engine

`TextDocument` should stop treating the piece tree as a storage detail and instead make it the canonical source for:

- edit mutation
- range extraction
- line lookup
- preview generation
- search snapshots
- viewport windows
- persistence snapshots

Flattening should only happen at explicit compatibility boundaries:

- save to disk
- clipboard operations that require owned text
- export
- legacy compatibility shims
- diagnostics or tests that intentionally request full text

## 2. Immutable snapshots become the bridge to parallelism

The app needs a `DocumentSnapshot` or equivalent immutable view with these properties:

- references a specific document revision
- shares backing buffers rather than copying them
- exposes range iterators, bounded extraction, and metadata queries
- is safe to send to worker threads
- carries enough revision identity for stale-result rejection

This is the central design requirement for the next phase.

Without it, every worker pipeline will continue to pay flattening or deep-clone cost before dispatch.

## 3. Background execution stays bounded and explicit

Once snapshots are cheap, a shared executor becomes high value.

That executor should:

- have bounded worker count
- expose clear task categories
- support stale-work cancellation or cheap rejection
- preserve deterministic ordering where user-visible order matters
- coalesce repeated requests for the same logical target

This should serve:

- search
- file read and decode
- piece-tree construction and install-ready buffer preparation
- restore
- save and session persistence
- deferred metadata recomputation
- future analysis and indexing tasks

## Implementation Plan

## Phase 0. Lock down the measurement and correctness baseline

Before broad architectural change, make the success criteria explicit.

Measurement priorities:

- search dispatch cost separate from search scan cost
- first-result latency separate from full-completion latency
- piece-tree snapshot creation cost
- viewport extraction latency for large buffers
- multi-file open latency
- restore latency for many tabs and buffers
- session persist latency and working-set growth
- main-thread frame stall time during search, open, save, and persistence

Correctness priorities:

- line and column coordinate invariants
- range extraction equivalence against a plain `String`
- randomized insert/delete/replace sequences
- Unicode-heavy edit and lookup cases
- snapshot isolation by revision

This phase is required because the document-core work and the concurrency work will otherwise hide regressions inside each other.

## Phase 1. Finish the piece-tree core as a production data structure

Goal:

- harden `PieceTreeLite` into a production-quality document core

Work:

- keep the current balanced root, internal-node, and leaf model
- retain append-only original and add buffers
- preserve subtree aggregates for bytes, chars, newlines, and piece count
- add any missing metadata needed for rendering and preview work
- explicitly document complexity targets for:
  - insert
  - delete
  - line lookup
  - char lookup
  - bounded range extraction
- audit the current balancing strategy for repeated local edits and long edit sessions
- avoid hidden full-tree rebuild paths outside explicit rebalance windows

Validation:

- deterministic edit-sequence tests
- randomized edit-sequence comparison against `String`
- invariants for subtree metadata and prefix tables
- Unicode correctness tests for character-based slicing and lookup

Exit criteria:

- the piece tree is trusted as the primary mutable text store
- correctness confidence comes from more than scenario tests

## Phase 2. Introduce revisioned immutable document snapshots

Goal:

- create the bridge that lets background work consume document state cheaply

Required properties:

- cheap clone
- shared backing storage
- fixed revision identity
- read-only piece traversal
- bounded extraction and preview APIs
- line and offset lookup APIs

Likely design direction:

- move backing buffers and tree storage behind shared ownership
- keep mutation on the live document side
- publish immutable snapshots by revision
- reject background results when the revision no longer matches

What this phase unlocks:

- search requests without full `String` cloning
- preview generation without cloned trees
- background save and persistence from stable snapshots
- restore and analysis tasks that do not touch live editor state

Exit criteria:

- a search request can carry a cheap snapshot handle instead of full copied text
- a save or persist request can consume a stable snapshot off the UI thread

## Phase 3. Convert read-heavy editor helpers to piece-tree-native access

Goal:

- stop wasting the new document core on avoidable temporary strings

Status update:

- `DocumentSnapshot` now exists and is already used across search request construction and preview generation
- hot word-boundary and cursor-motion helpers no longer rely on whole-prefix or whole-suffix extraction in the common path
- viewport-oriented extraction has dedicated benchmark and profile coverage
- the remaining work in this phase is consolidation: remove compatibility fallbacks and other residual flattening on still-hot read paths

Priority conversions:

- word-boundary logic
- cursor motion helpers
- preview generation
- visible-line extraction
- selection-scoped range reads
- metadata refresh that can be incremental

Design rules:

- prefer iterators and bounded local scans over full-prefix extraction
- avoid `extract_range(0..index)` patterns on hot cursor paths
- avoid reintroducing "clone the tree and flatten later" request building on newer snapshot-based flows
- keep the small-document fast path simple when it is actually cheaper

Exit criteria:

- common cursor, selection, preview, and visible-range helper paths stay on bounded piece-tree reads by default, with compatibility fallbacks limited to clearly isolated paths

## Phase 4. Rebuild search on top of snapshots and bounded fanout

Goal:

- make search both cheap to dispatch and scalable across buffers

Status update:

- search requests already carry revisioned snapshot handles instead of cloned whole-buffer payloads
- match previews are already built from snapshot-backed preview helpers
- bounded fanout exists for wide-scope target processing
- report-driven coverage now includes `search_dispatch_profile` alongside the existing current-scope and all-tabs search profiles
- the remaining work is no longer foundational wiring; it is targeted latency reduction in two separate slices: runtime dispatch preparation and worker-side scan/completion

Work:

- reduce runtime target collection and request assembly cost on the UI-thread side of dispatch
- reduce worker-side scan cost where `DocumentSnapshot` is still flattened into a temporary search string per target
- keep the single-buffer small-work path lightweight
- decide whether partial result streaming still earns its complexity after dispatch-path and scan-path reductions
- preserve generation-based cancellation, stale-result rejection, and bounded fanout while tightening variance on wide-scope search

Important rule:

- search parallelism should happen across independent buffers or chunks, not by scattering tiny tasks blindly

Exit criteria:

- dispatch cost is small enough that worker offload actually matters
- `ActiveWorkspaceTab` and `AllOpenTabs` search scale without major UI-thread preparation spikes
- first-result and completion latency both improve materially on large scopes, with dispatch-path and worker-scan regressions measurable independently

## Phase 5. Move file, save, restore, and persistence flows onto snapshot-based workers

Goal:

- remove recurring blocking I/O from interactive flows without weakening correctness

### 5.1 File open and decode

- keep user intent and tab placement decisions on the UI thread
- move read, decode, and artifact detection onto workers
- move initial piece-tree construction and any install-ready document preparation onto workers as well
- preserve deterministic tab ordering for multi-file open
- allow bounded parallel open for independent files

### 5.2 Save

- create a stable snapshot
- flatten only on the save boundary
- write on a background worker
- allow explicit synchronous flush where correctness boundaries require it

### 5.3 Session persistence

- mark session dirty immediately
- coalesce repeated persistence requests
- persist from stable snapshots on a background writer
- reserve synchronous flush for close, crash-critical, or explicit user-save boundaries

### 5.4 Startup restore

- parse restore state and load buffers off the UI thread
- install ready buffers incrementally or in coarse batches
- keep the startup surface coherent while work completes progressively

### 5.5 Deferred metadata recomputation

- keep mutation and correctness-critical metadata on the foreground path
- publish revisioned snapshots for heavier artifact and compliance scans
- recompute deferred metadata on workers
- apply results only if the revision still matches

Exit criteria:

- open, save, restore, and normal persistence no longer cause avoidable frame stalls
- large edits do not require every whole-document metadata pass to finish synchronously

## Phase 6. Build the large-document editor path on piece-tree windows

Goal:

- make the storage work matter in real UI interaction

Work:

- expose viewport-oriented line and span windows from the piece tree
- support overscan extraction for smooth scrolling
- map cursor and selection operations onto piece-tree coordinates directly
- stop requiring whole-document contiguous text for large buffers
- preserve a simpler legacy path for small documents if it remains cheaper

Important point:

This is not an optional later polish pass.

Without it, the app will continue paying full-layout costs that no amount of background work can hide.

Exit criteria:

- large-file view and edit operations work from visible-range extraction rather than whole-buffer flattening

## Phase 7. Tighten backpressure, ownership, and failure rules

Goal:

- keep the more concurrent system debuggable and correct

Required rules:

- every background result carries a revision or generation token
- stale results are rejected cheaply
- queues stay bounded
- repeated requests coalesce
- task ownership is explicit
- ordering is deterministic where the UI expects it
- worker tasks never mutate live UI state directly

This phase should be implemented incrementally alongside Phases 4 through 6, not postponed until the end.

## Priority Order

If the repo wants the best return with the least wasted effort, the order should be:

1. measure dispatch, snapshot, and I/O costs explicitly
2. finish and harden the piece-tree core
3. finish converting hot read paths to piece-tree-native access so the remaining flattening is explicit and rare
4. consolidate the landed snapshot model so the remaining hot search paths stop flattening unnecessarily
5. split search follow-up into dispatch-path and worker-scan reductions, each with dedicated profile-driven validation
6. move file open, piece-tree build, restore, and persistence to background snapshot consumers
7. separate immediate metadata from deferred metadata recomputation
8. build the large-document viewport path

This order matters.

Adding a shared executor before cheap snapshots and incremental reads would mostly move copying around instead of removing it.

## Validation Plan

Every phase should be validated against:

- the existing measurement workflow in `docs/measurement-tools.md`
- focused profile entrypoints in `src/profile.rs`
- large-file open, scroll, paste, and split probes
- search first-result and full-completion metrics
- tab-count and session-scale resource probes
- randomized correctness checks against plain-string behavior

The performance report should explicitly track:

- UI-thread dispatch cost
- worker completion cost
- amount of text flattened per workflow
- amount of cloning per workflow
- first useful result latency
- peak working set and page-fault growth

## Expected Outcome

If this plan is executed well, Scratchpad should end up with:

- lower frame hitching during search, open, save, restore, and persistence
- much cheaper search dispatch
- better first-result latency for wide-scope search
- better scaling across many buffers and large files
- less whole-buffer copying and flattening
- a document engine whose structure actually matches the app's performance goals

Most importantly, the app should stop treating "parallelism" and "large-text architecture" as separate topics.

The piece tree is the prerequisite that makes bounded parallelism worthwhile.

## What To Fix First

Based on the latest full overview in `target/analysis/`, the immediate order of work should be:

1. Reduce dispatch-path cost independently from worker scan cost.
   The new dispatch baselines and `search_dispatch_profile` now make runtime target collection and request assembly measurable on their own. Use `search_current_dispatch_aggregate_size` and `search_all_dispatch_aggregate_size` to shrink the preparation path before worker-side scanning begins.

2. Reduce worker-side scan and completion cost independently from dispatch.
   The top over-budget rows are still dominated by current-scope and all-tabs completion scenarios, especially `search_current_completion_aggregate_size/256`. Use `search_current_app_state_profile` and `search_all_tabs_profile` to drive improvements in the worker scan path after dispatch overhead is separated out.

3. Keep the landed snapshot and fanout foundations, but remove the remaining flattening inside hot search paths.
   The foundational snapshot, preview, fanout, and coverage work is already in place. The next leverage point is reducing the temporary string work that still happens after target collection and before or during matching.

4. Convert the remaining hot editor read paths away from whole-prefix and whole-suffix extraction.
   Word-boundary and nearby cursor helpers still allocate temporary strings in ways that blunt the benefit of the piece tree. This is smaller than the search snapshot problem, but it is on the same architectural path and should happen early.

5. Move normal session persistence onto snapshot-based background work.
   The resource profile shows session persist cost growing to roughly 1.7 seconds at 1000 tabs. That is too expensive to keep coupled tightly to interactive flows once the snapshot model exists.

6. Move more of file open and restore beyond raw I/O into background document preparation.
   The current background lane already helps with read and decode, but large-file open is still too expensive because installable document state is prepared too late and too synchronously.

7. Separate immediate metadata updates from deferred whole-document metadata recomputation.
   Large paste and other large-document edits still pay too much post-mutation work on the foreground path. Revision-safe deferred metadata work is now one of the clearest concurrency opportunities.

8. Build the large-document viewport path after search and snapshots land.
   The capacity output still says file-size and paste ceilings are memory-bound, with file-size usability dropping after 32 MB and paste usability dropping after 8 MB. That means the app still needs visible-range rendering and incremental document access, not just more worker threads.

## Recommendation

The next implementation step should be:

1. harden the current piece-tree core with stronger invariants and randomized validation
2. remove the highest-value remaining flattening on active editor and hot helper paths
3. reduce dispatch-path overhead using `search_dispatch_profile` plus the dispatch aggregate-size benchmarks
4. reduce worker scan and completion overhead using `search_current_app_state_profile` and `search_all_tabs_profile`
5. move session persistence and more of open and restore preparation onto bounded snapshot-based background work
6. split metadata work into immediate and deferred revision-safe stages

After those three steps, the next round of search work can be split cleanly between runtime preparation cost and worker-side scan cost instead of treating them as one blended bottleneck.
