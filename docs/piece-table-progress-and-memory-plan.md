# Piece Table Progress And In-Memory Performance Plan

Date: 2026-04-23

## Purpose

This document captures the current state of Scratchpad's piece-table and piece-tree work, identifies the remaining performance bottlenecks, and lays out a no-code implementation plan for selectively moving more data into memory where it is likely to produce the best performance return.

This plan is not only about search.

It is explicitly about four scale cases:

- loading and restoring very large documents
- manipulating very large active documents
- keeping one workspace tab with many distinct documents efficient
- keeping a workspace with many open tabs and buffers efficient

## Executive Summary

Scratchpad is no longer at the "flat piece-table probe" stage.

The current codebase already has a real integrated `PieceTreeLite` document core with:

- append-only original and add buffers
- root, internal-node, and leaf structure
- cached byte, char, newline, and piece metrics
- character-based insert and delete
- bounded extraction and preview helpers
- revisioned shared snapshots for worker-facing operations

That means the core storage direction is now credible.

The biggest remaining performance problem is not whether the project has a piece table at all.

The biggest remaining problem is that the app still lacks a complete scaling strategy across active-document cost, inactive-document memory pressure, and workspace-wide load and restore behavior.

Some important app paths still fall back to contiguous whole-text `String` extraction, especially in editor rendering and fragmented-search fallback paths.

But the broader issue is larger than that:

- large file load still builds full decoded content eagerly
- startup restore still restores the whole saved workspace
- one tab may hold many distinct buffers through `extra_buffers`
- many open tabs multiply the memory cost of full document state, undo history, metadata, and any future caches

The next performance phase should therefore focus on:

1. eliminating unnecessary whole-buffer flattening on interactive paths
2. adding selective in-memory caches where fragmentation makes flattening unavoidable
3. keeping persistence-boundary flattening as an acceptable exception
4. adding explicit memory-tier rules for active, warm, and cold buffers

The key policy point is that "large file" does not automatically mean "do not use memory."

A 1 GB text file is enormous as a document, but on a machine with ample RAM it may still be entirely reasonable to spend another 1 GB or more on acceleration for the actively edited document if that materially improves latency.

The real constraint is not raw file size by itself.

The real constraint is total workspace memory pressure relative to available system memory.

## Current Progress

## What is already implemented

The integrated document core now includes:

- `PieceTreeLite` in `src/app/domain/buffer/piece_tree.rs`
- tree mutation support in `src/app/domain/buffer/piece_tree/edit.rs`
- range and span iteration support in `src/app/domain/buffer/piece_tree/slice.rs`
- `TextDocument` ownership through `Arc<PieceTreeLite>` in `src/app/domain/buffer/document.rs`
- immutable revisioned `DocumentSnapshot` in `src/app/domain/buffer/snapshot.rs`

Capabilities already present:

- character-aware length tracking
- line lookup and character-position mapping
- bounded range extraction
- preview extraction for search matches
- borrowed contiguous-text fast path when a requested region lives in one span
- background search dispatch using snapshots rather than cloned preview trees
- piece-tree-backed metadata refresh and visible-line window extraction

## What has been validated

Current validation already covers meaningful correctness ground:

- repeated inserts that split into multiple balanced nodes
- repeated removals that merge nodes back down
- randomized edit-sequence comparison against a `String` model
- preview generation correctness after fragmentation
- visible-line-window extraction through the piece tree
- search snapshot behavior for borrowed contiguous text and fragmented fallback

As of 2026-04-23, the relevant focused test slices passed:

- `cargo test piece_tree --lib`
- `cargo test search --lib`

## What this means

The project has moved beyond the old Phase 0b "descriptor vector only" piece-table prototype.

In practice, Scratchpad is now on the indexed hybrid or piece-tree path described in earlier planning docs, and that work is already integrated into the live buffer, snapshot, search, and viewport machinery.

## Workloads This Plan Must Cover

The next-stage plan needs to optimize for more than one benchmark shape.

## 1. Large-document load

Examples:

- opening a single very large file from disk
- reopening a large file with another encoding
- startup restore that includes one or more large saved buffers

Relevant current behavior:

- file load and session restore already run through a background I/O lane
- path loads and restore requests are processed off the UI thread
- file decode still eagerly produces full text content before the buffer becomes live

## 2. Large-document manipulation

Examples:

- editing a very large active file
- scrolling through a very large active file
- repeated cursor motion, preview extraction, undo, and visible-window updates on fragmented text

Relevant current behavior:

- piece-tree-backed visible-line extraction already exists
- the active native editor still has a full-text compatibility path
- undo is still string-backed rather than descriptor-backed

## 3. One tab containing many documents

Examples:

- a workspace tab with several file groups
- split views that reference different buffers inside one tab
- keeping metadata and view state responsive even when the tab owns many distinct files

Relevant current behavior:

- `WorkspaceTab` can hold many distinct buffers through `extra_buffers`
- one tab therefore may already represent a multi-document workspace with real memory cost
- tab-level session capture currently snapshots every buffer in the tab

## 4. Many open tabs and many total buffers

Examples:

- opening dozens or hundreds of tabs
- restoring a prior session with many buffers
- leaving many clean documents open for long periods

Relevant current behavior:

- session restore restores saved buffers into full `BufferState` values
- session persistence captures a snapshot for every buffer and writes full text for each saved buffer
- there is not yet a clear active-versus-inactive memory policy for open buffers

## Memory Principle

Scratchpad should optimize against memory pressure, not against file size in isolation.

That means the plan should prefer the following rule set:

- for the active document, spend memory aggressively when there is clear latency benefit and system memory headroom exists
- for warm nearby documents, keep enough state to switch quickly without paying full rebuild cost
- for cold inactive documents, become conservative and drop derived caches first
- when the workspace as a whole becomes large, optimize for total residency rather than maximum speed on every open buffer at once

In other words:

- a single active 1 GB file may justify expensive acceleration structures
- 100 open files that together consume tens of gigabytes may not

The policy therefore needs dynamic budgeting rather than a blanket "large files must stay memory-thin" rule.

## Remaining Bottlenecks

## 0. The plan previously underweighted workspace-scale memory pressure

This is the main correction to the earlier draft.

The prior version focused too heavily on search and interactive flattening, and not enough on:

- eager hydration of all restored buffers
- cumulative memory cost of many open buffers
- the difference between an active buffer and a cold background buffer
- multi-buffer tabs and whole-workspace restore behavior

## 1. Full-text editor rendering still exists on the hot path

The active native editor render path still begins by extracting the entire document text before layout.

That is the most important remaining interactive bottleneck because it keeps large-document editing tied to full-buffer flattening and full-buffer layout cost.

## 2. Fragmented search still falls back to owned text

`DocumentSnapshot::search_text_cow` already borrows contiguous text cheaply when possible.

When the requested range is fragmented across pieces, it still builds an owned `String`.

That is much better than the old model, but it still means search on edited or fragmented documents can pay a full flattening cost before matching work begins.

## 3. Undo still stores deleted and inserted text as `String`

Undo and redo behavior are better structured than earlier probe code, but they do not yet realize the full piece-table advantage.

The current document operation history still stores inserted and deleted text payloads rather than piece descriptors or slice references.

## 4. Some repeated local scans still rely on character-by-character traversal

Cursor movement, word-boundary logic, preview generation, and line navigation are all much better aligned with the piece tree than before.

Even so, some of those helpers still repeatedly call `char_at`, `line_info`, or local scans in ways that may become noticeable at scale, especially after fragmentation.

## 5. Persistence still flattens, but that is acceptable

Session persistence and file save still extract full text.

That is not the main performance concern because these are persistence boundaries rather than frame-sensitive interaction paths.

Those paths should stay lower priority unless measurements say otherwise.

However, persistence volume still matters at workspace scale because a large saved session with many buffers can force a lot of eager work during save and later during restore.

So save-path flattening is acceptable, but eager restore of every saved buffer is not automatically acceptable.

## 6. Buffer residency policy is still too simple

Today the system largely assumes that an open buffer remains a fully hydrated live document.

That is manageable for small workspaces.

It becomes expensive when the user keeps:

- many large files open
- many buffers inside one tab
- many tabs across a long session

The next performance phase needs a real residency model rather than a one-size-fits-all "everything open stays fully live" approach.

## Options For Moving More Data Into Memory

The question is not whether to move more data into memory in the abstract.

The question is which additional in-memory representations are worth their memory cost.

## Option A. Lazy per-revision contiguous shadow-text cache

Shape:

- keep the piece tree as the source of truth
- lazily build a contiguous `String` for a revision only when a hot path needs it
- retain it behind revision identity so repeated calls reuse the same owned text

Benefits:

- directly reduces repeated flattening for fragmented search
- helps compatibility paths that still require full text
- likely gives the fastest practical improvement for large edited documents

Costs:

- can temporarily double memory for cached revisions
- must be invalidated or naturally replaced by revision changes

Assessment:

This is the strongest next option if the goal is to reduce real app latency for active documents without redesigning every consumer at once.

It should be lazy, revision-scoped, and used only where full contiguous text is actually needed.

For a single large active file on a machine with headroom, this option should be considered acceptable even when the cache is large.

## Option B. Eager always-live full-text mirror

Shape:

- keep a full contiguous `String` mirror alongside the piece tree at all times

Benefits:

- simplest read path for any whole-text consumer
- avoids rebuild cost entirely

Costs:

- permanently duplicates document memory
- weakens the point of the piece-table design for large files
- increases mutation bookkeeping complexity

Assessment:

This is too expensive as the default policy.

It only makes sense if the project decides that near-term compatibility speed matters more than memory behavior, which does not match the evidence collected so far.

## Option C. Leaf-local line-start indexes

Shape:

- add per-leaf cached line-start byte or char offsets
- preserve subtree line counts at internal nodes as today

Benefits:

- reduces repeated rescanning inside leaves for line lookup and preview work
- improves visible-window extraction and cursor movement
- memory overhead is modest and proportional to local newline density

Costs:

- more metadata maintenance on split, merge, insert, and delete

Assessment:

This is a very good second-wave optimization after the larger flattening issue is addressed.

It is cheaper than a full-text mirror and directly targets line-oriented editor behavior.

## Option D. Leaf-local word-boundary hints

Shape:

- add lightweight metadata for whitespace, punctuation, or word-run boundaries inside leaves

Benefits:

- may reduce repeated `char_at` calls for word jumps and selection expansion

Costs:

- adds more metadata complexity
- win is likely smaller than viewport or flattening improvements

Assessment:

This should be treated as a later, measurement-driven optimization rather than an immediate priority.

## Option E. Search-specific secondary index

Shape:

- build an auxiliary per-revision structure to accelerate search
- examples could include chunk tables, normalized lowercase caches, or future token indexes

Benefits:

- can improve repeated searches across the same revision

Costs:

- memory overhead can grow quickly
- adds invalidation complexity
- may not help enough unless search is still dominant after flattening costs are reduced

Assessment:

This is worth considering only after the piece-tree and snapshot path stop paying repeated flattening costs.

Otherwise the project risks optimizing the wrong layer.

## Option F. Progressive buffer hydration

Shape:

- restore or open a workspace in stages
- hydrate the active tab first
- hydrate visible or recently requested buffers next
- defer cold buffers until first activation or an explicit background budget allows them

Benefits:

- makes startup and large workspace restore feel faster
- avoids paying full load cost for buffers the user may never look at in the current run
- reduces peak memory pressure during startup

Costs:

- needs placeholder buffer states and clear loading transitions
- complicates restore sequencing and some commands

Assessment:

This is one of the most important non-search optimizations for many-tabs and many-buffers scenarios.

It is likely more valuable than deeper search work for real workspace-scale responsiveness.

## Option G. Active, warm, and cold buffer residency tiers

Shape:

- active buffers stay fully hydrated with hot caches allowed
- warm buffers stay piece-tree-backed but drop expensive derived caches first
- cold clean buffers may be demoted to lightweight metadata-plus-path state and rehydrated on demand

Benefits:

- puts a real cap on workspace memory growth
- allows aggressive optimization of the active document without paying that cost for every open document
- aligns memory policy with user behavior instead of raw tab count

Costs:

- requires explicit lifecycle transitions
- needs careful behavior for dirty buffers, undo history, and conflict-on-disk cases

Assessment:

This is the key architectural answer for very large numbers of tabs and multi-buffer workspaces.

Without it, every new cache risks improving one active document while making total workspace memory much worse.

This tiering model also creates room to be intentionally memory-hungry for the active document while staying disciplined at workspace scale.

That is likely the right trade for Scratchpad.

## Option H. Restore and open budgeting

Shape:

- cap how many files or buffers are fully opened concurrently
- batch restore work
- prioritize active and likely-visible content

Benefits:

- reduces startup spikes
- prevents a large session restore from overwhelming CPU, disk, and memory all at once

Costs:

- startup completion becomes staged instead of all-at-once

Assessment:

This is a strong companion to progressive hydration and should be treated as part of the same workspace-scale design.

## Recommended Direction

The best near-term performance strategy is:

1. keep the piece tree as the canonical storage model
2. avoid eager duplication of all document text
3. add selective lazy in-memory caches only for active paths that still require contiguous text
4. add an explicit residency and hydration strategy for inactive buffers and large restored workspaces

That should be read as a dynamic policy, not a memory-avoidance policy.

When one active document is huge and system memory is available, Scratchpad should be willing to spend memory for speed.

When the user has many open buffers or tabs, Scratchpad should start optimizing for total residency instead.

That leads to the following priority order.

## Recommended Priority Order

### Priority 1. Progressive hydration and residency policy

Before adding more caches, the app needs rules for which buffers stay fully live.

The first priority should be:

- hydrate the active tab and active buffers first
- delay or budget hydration for cold restored buffers
- define active, warm, and cold buffer tiers
- drop expensive derived caches before considering document eviction

This is the most important correction for many-open-tab and many-buffer scenarios.

### Priority 2. Lazy shadow-text cache for fragmented active revisions

Use a lazy contiguous-text cache only when:

- search needs full contiguous text and `borrow_range` cannot return a shared slice
- the active editor compatibility path still requires whole-document layout

Do not make this cache unconditional for every revision.

Do not keep these caches alive for cold inactive buffers unless measurement proves they are worth it.

But for the active document, especially a very large one, the threshold should be generous.

If the user is actively working in a 1 GB file and the machine has headroom, the system should prefer faster repeated operations over strict memory minimalism.

## Priority 3. Make visible-window rendering the default for large unwrapped buffers

The existing piece-tree-backed visible-line window path is already in the codebase.

The next step should be to expand the conditions under which that path becomes the normal large-buffer editor route instead of the exception.

This is likely a bigger real-world win than adding more metadata everywhere.

## Priority 4. Add leaf-local line-start caches

Once full-text flattening is less dominant, line-local indexing is the most promising lightweight in-memory enhancement.

It targets:

- line lookup
- preview extraction
- viewport slicing
- vertical cursor movement

## Priority 5. Rework undo toward piece-descriptor history

This does not need to happen before the rendering and search wins.

But it is still the most direct way to realize the original piece-table advantage in long edit histories.

## Priority 6. Consider smaller hot-helper caches only if profiles still point there

Word-boundary hints or search-specific caches should come only after measurement confirms they matter.

## Concrete Plan

## Phase 1. Re-measure by workload class

Goal:

- establish whether current cost is dominated by active-document work, startup restore, multi-buffer residency, or workspace-wide tab count

Work:

- use the existing profile entry points for document snapshot, search dispatch, and viewport extraction
- add measurement slices for:
  - opening one very large file
  - restoring many buffers
  - one tab with many buffers
  - many tabs with mixed active and inactive buffers
- compare contiguous documents against fragmented edited documents
- record both elapsed time and memory deltas

Exit criteria:

- there is a current baseline that distinguishes:
  - load cost
  - active editing cost
  - restore cost
  - inactive residency cost

## Phase 2. Define buffer residency and progressive hydration

Goal:

- prevent many-buffer and many-tab workspaces from forcing every open document to stay fully hydrated and equally expensive

Work:

- define active, warm, and cold buffer tiers
- define which caches survive in each tier
- hydrate active content first during restore and open
- defer cold-buffer hydration until activation or background budget permits it
- preserve correct behavior for dirty buffers and disk-freshness tracking

Exit criteria:

- large restored workspaces do not eagerly pay the same full cost for every buffer at startup
- inactive clean buffers have a cheaper steady-state memory story than active buffers

## Phase 3. Add lazy contiguous shadow-text caching for active fragmented revisions

Goal:

- eliminate repeated flattening cost for fragmented active revisions without eagerly duplicating every document

Work:

- define a revision-scoped cache boundary
- use it for fragmented-search fallback
- use it for any remaining active-editor full-text compatibility path
- keep the piece tree as the canonical mutable state

Exit criteria:

- repeated work over the same active fragmented revision no longer rebuilds full text each time
- cold buffers are not forced to keep these caches alive

## Phase 4. Push large-buffer editing harder onto visible-window rendering

Goal:

- remove whole-document extraction from the main interactive editor path for large unwrapped documents

Work:

- expand the existing visible-line-window path
- keep small-document and wrapped-text fallback behavior where necessary
- verify cursor, selection, scrolling, and search highlights stay correct

Exit criteria:

- large unwrapped buffers no longer rely on full-document text extraction during normal editing

## Phase 5. Add leaf-local line indexes

Goal:

- reduce repeated rescans inside leaves for line-oriented operations

Work:

- store line-start offsets or equivalent leaf-local line metadata
- update them incrementally during split, merge, insert, and delete
- use them in line lookup, preview extraction, and viewport slicing

Exit criteria:

- line and preview operations show lower scan cost on fragmented large documents

## Phase 6. Revisit undo representation

Goal:

- realize more of the long-session efficiency that motivated piece-table storage in the first place

Work:

- replace string-heavy undo payloads with piece descriptors or slice-based edit records
- preserve selection restore semantics
- validate reverse application thoroughly

Exit criteria:

- undo-heavy workloads improve in both memory behavior and elapsed time

## Phase 7. Revisit open and restore parallelism

Goal:

- improve load and restore throughput once hydration policy and buffer tiers exist

Work:

- revisit the bounded background I/O lane
- determine whether path loads or session restore should gain more controlled fanout
- keep prioritization tied to active and visible content rather than raw bulk concurrency

Exit criteria:

- large open and restore workflows improve without causing larger memory spikes or regressing interaction quality

## What Not To Do Yet

The following ideas should stay out of the first next phase unless new measurements force a change:

- do not keep an always-live full contiguous mirror for every document
- do not add heavy search indexes before flattening costs are under control
- do not prioritize save-path flattening over active editing, load, restore, and residency behavior
- do not add large amounts of metadata everywhere before proving the line-oriented paths still dominate after rendering and search improvements
- do not optimize only the active document while ignoring inactive-buffer memory growth across a large workspace

This does not mean "avoid spending memory on large active files."

It means "avoid spending the same expensive memory on every open file by default."

## Success Criteria

The next stage should be considered successful if it achieves most of the following:

- startup restore can prioritize the active workspace rather than eagerly hydrating every saved buffer equally
- many open tabs and many multi-buffer workspaces have a better steady-state memory story
- large edited documents no longer pay whole-document extraction during normal unwrapped editing
- repeated work against fragmented active revisions reuses contiguous text when it must exist
- line and preview operations avoid repeated deep rescans inside leaves
- memory growth stays selective and revision-scoped rather than permanently duplicating all open buffers
- undo remains correct and is positioned for later piece-descriptor storage

## Recommendation

Scratchpad should continue the piece-tree direction as the main document-core path.

The immediate next move is not to store everything twice in memory.

But it may be entirely correct to store the active document twice, or maintain other large acceleration structures for it, when system memory headroom makes that a good trade.

The immediate next move is to:

1. keep the piece tree canonical
2. add a real residency model for active, warm, and cold buffers
3. restore and open workspaces progressively instead of treating every saved buffer as equally urgent
4. add a lazy contiguous shadow-text cache only for active fragmented revisions that truly need it
5. push the visible-window editor path further into the default large-buffer route
6. add leaf-local line indexing only after measuring the post-cache bottlenecks

That path is the best balance between performance, memory discipline, and implementation risk.
