# Open Overview Capacity Review

Date: 2026-04-24

## Question

After reading "Why Zed's Editor Is Fast" and reviewing the latest open-overview artifacts, why is Scratchpad's measured capacity still not improving in a meaningful way?

## Executive Summary

The short answer is that Scratchpad has adopted some Zed-like ingredients, but not yet in the specific paths that the open-overview capacity probe measures.

Zed stays fast because its architecture is incremental across the whole stack:

- summarized structural reads instead of repeated whole-buffer rescans
- layered display transforms with narrow invalidation rather than broad recomputation
- snapshot-oriented background work that can safely stay off the foreground path
- explicit attention to foreground blocking as a product bug

Scratchpad now has part of that story. It has a piece-tree-backed document model and more snapshot-friendly background work than before. But the latest overview still shows that the measured capacity ceilings are dominated by synchronous whole-buffer construction, full metadata refresh, and eager tab materialization.

That is why capacity is not moving much:

1. the probe still measures direct foreground-style work such as `BufferState::new(...)`, `insert_direct(...)`, and `refresh_text_metadata()`
2. those paths still do large full-text or full-document passes
3. recent improvements are more visible in background open, restore, and persistence flows than in the probe's direct in-memory stress paths
4. the remaining hot paths are still much closer to "whole-buffer work" than to Zed's "incremental everywhere" model

So the main problem is not that the recent work was worthless. The main problem is that the architecture is still only partially incremental, and the latest capacity probe is measuring the parts that remain least incremental.

## What The Latest Open-Overview Run Shows

From [target/analysis/capacity_report.json](../target/analysis/capacity_report.json):

- file size ceiling: last successful at 32.0 MB, first failure at 128.0 MB
- paste size ceiling: last successful at 64.0 MB, first failure at 256.0 MB
- tab count ceiling: last successful at 512 tabs, first failure at 4096 tabs
- split count ceiling: no failure through 32 splits

Those are latency ceilings, not out-of-memory ceilings. The probe marks a scenario as failed when it crosses a usability budget, not when the process crashes.

That distinction matters. The latest run does not say "Scratchpad runs out of RAM at 128 MB." It says "Scratchpad is already too slow to feel interactive by that point."

The companion resource profiles in [target/analysis/resource_profiles.json](../target/analysis/resource_profiles.json) reinforce that interpretation:

- file-backed 128 MB open: 352.5 ms, about 167.8 MB allocated
- 64 MB paste allocation profile: 142.5 ms, about 135.6 MB allocated, 3430 allocations, 162 reallocations
- 4096-tab resource profile: 816.1 ms, about 215.1 MB allocated, about 211.8 MB peak live

These numbers are not "machine capacity exhausted" numbers. They are "too much synchronous work for an interactive editor" numbers.

## What Zed's Review Changes About The Interpretation

The Zed review in [docs/zed-editor-performance-review.md](zed-editor-performance-review.md) sharpens the interpretation.

The most important lesson from that review is not just "use a rope" or "use a piece tree." Zed is fast because the same incremental design appears repeatedly:

- text storage is structural and summary-driven
- coordinate conversion is structural
- display transforms propagate narrow invalidations
- snapshots are first-class
- profiling guards the foreground path

Against that standard, Scratchpad looks transitional rather than complete.

Scratchpad now has a piece-tree document model, but the latest overview shows that the surrounding system still often behaves like a full-document editor around that core.

In other words:

- the text store is more incremental than before
- the editor workflow around it is still not incremental enough

That is the main reason capacity is not improving.

## Root Cause 1: The Capacity Probe Still Measures Direct Synchronous Core Work

The clearest evidence is in [src/bin/capacity_probe.rs](../src/bin/capacity_probe.rs).

The probe does not mainly measure the file-open and restore paths that were recently pushed into background work. Instead it directly exercises:

- `BufferState::new(...)` for the file-size sweep
- `BufferState::new(...)` plus `insert_direct(...)` plus `refresh_text_metadata()` for the paste sweep
- repeated `WorkspaceTab::new(...)` and tab combining for the tab sweep

That means the benchmark is still centered on:

- synchronous buffer construction
- synchronous metadata recomputation
- eager tab object growth

So if recent work mostly improved background I/O orchestration, restore flows, or deferred compliance checks, the probe was never well-positioned to show a large gain.

This is the single biggest reason the latest open-overview run does not show the improvement one might expect from recent architectural cleanup.

## Root Cause 2: Buffer Construction Is Still Full-Content Work

Scratchpad's text document is piece-tree-backed in [src/app/domain/buffer/document.rs](../src/app/domain/buffer/document.rs), which is a real improvement over a monolithic editable `String` hot path.

But buffer construction still begins from full content and full inspection.

In [src/app/domain/buffer/state.rs](../src/app/domain/buffer/state.rs):

- `BufferState::new(...)` derives `TextFormatMetadata` from the full input text
- `BufferState::with_format(...)` calls `buffer_text_metadata(&content, &mut format)`
- that means line endings, artifact state, and related metadata are computed before the buffer is built

In [src/app/services/file_service.rs](../src/app/services/file_service.rs):

- `read_file(...)` decodes the entire file into a `String` with `read_to_string(...)`
- format and text metadata are then derived from the full decoded content

So Scratchpad now has a better internal mutation structure, but large-file open and initial buffer construction are still fundamentally whole-content operations.

That is not how Zed's architecture wins. Zed wins because the surrounding layers also avoid rediscovering the whole document repeatedly.

## Root Cause 3: Large Paste Still Pays For Full Metadata Rescans

The latest run says paste capacity is better than some earlier regression notes suggested, but it is still not structurally solved.

The key path is visible in two places:

- the probe in [src/bin/capacity_probe.rs](../src/bin/capacity_probe.rs)
- buffer metadata refresh in [src/app/domain/buffer/state.rs](../src/app/domain/buffer/state.rs)

The paste sweep does this directly:

1. create a base buffer
2. insert a large string into the piece tree with `insert_direct(...)`
3. immediately call `refresh_text_metadata()`

`refresh_text_metadata()` then calls `buffer_text_metadata_from_piece_tree(...)`, implemented in [src/app/domain/buffer/analysis.rs](../src/app/domain/buffer/analysis.rs).

That analysis still iterates text spans across the document and runs `TextInspection::inspect_spans(...)` to recompute line counts, line-ending state, and artifact summary.

So the insertion itself is no longer the old "shift one giant string tail" problem, but the edit still triggers a document-wide metadata pass.

This is a direct mismatch with the Zed lesson. Zed's advantage is not only localized mutation. It is also localized invalidation and localized recomputation. Scratchpad has improved the mutation primitive more than it has improved the recomputation story around it.

## Root Cause 4: Tab Capacity Is Still Dominated By Eager Live State

The latest tab ceiling still fails at 4096 tabs, and the resource profile shows that this is mostly an object-growth and synchronous-work problem, not a machine-RAM limit.

Again the probe makes that plain. The tab-count sweep in [src/bin/capacity_probe.rs](../src/bin/capacity_probe.rs) repeatedly builds full `WorkspaceTab` values and then performs split and combine operations across them.

That is not a lightweight descriptor model. It is a live object graph model.

Zed's architecture review points toward the opposite direction:

- summarized state
- narrow invalidation
- avoid broad recomputation

Scratchpad's tab stress path still looks much more eager than summarized. The app still pays for constructing and manipulating many live buffers, views, and pane structures rather than cheap inactive-tab descriptors.

That is why the latest overview still treats tab count as a practical capacity ceiling even though the absolute allocation numbers are modest relative to system RAM.

## Root Cause 5: Background Work Improved, But The Execution Model Is Still Not Zed-Like End To End

Recent work did move some operations into background processing, and that is directionally correct.

But the latest code still shows two limits.

First, [src/app/services/background_io.rs](../src/app/services/background_io.rs) runs a single FIFO worker thread for:

- path loads
- session restore
- session persistence
- encoding-compliance refresh

That is useful for moving work off the UI thread, but it is still one serial lane.

Second, [src/app/services/session_manager.rs](../src/app/services/session_manager.rs) still captures the session synchronously before queueing persistence.

So the system has more background work than before, but it still does not match the Zed pattern of revision-safe snapshots and bounded parallel work strongly enough to change the measured capacity story.

This matters because the open-overview capacity probe is harsh on any remaining foreground or single-lane whole-state work.

## Root Cause 6: Search Still Shows A Smaller Version Of The Same Architectural Gap

The speed-efficiency rollup in [target/analysis/speed_efficiency_report.json](../target/analysis/speed_efficiency_report.json) reports:

- 12 over-budget latency scenarios
- 3 near-failure capacity ceilings

The top triage items are search scenarios, not capacity scenarios. That is relevant because they show the same pattern: the system still spends too much time doing scale-sensitive work that looks like repeated scanning.

In [src/app/app_state/search_state/worker.rs](../src/app/app_state/search_state/worker.rs), fragmented plain-text search avoids flattening the whole buffer, which is good. But it still processes the search as overlapping extracted windows.

That is better than unconditional full flattening, but it is still not the kind of summary-driven, structural query path the Zed review points toward.

This does not directly explain the capacity ceilings by itself. It does show that the same deeper issue remains across the codebase: Scratchpad has reduced some worst-case flattening, but it has not yet turned enough of its surrounding editor work into true structural incrementalism.

## Why Capacity Is Not Improving, In One Sentence

Scratchpad has improved some infrastructure around the editor, but the latest overview still measures the parts where the editor does too much synchronous whole-buffer construction, whole-document metadata refresh, and eager live-state growth, so the reported ceilings remain roughly where they were.

## What Would Have To Change To Move The Overview Numbers

The Zed review suggests the direction clearly.

If the goal is specifically to move the open-overview capacity ceilings, the highest-value improvements are the ones that make the measured paths more incremental rather than merely more asynchronous.

## 1. Make metadata refresh incremental

The current paste path still pays for a document-wide metadata pass after mutation. That is probably the most direct reason large paste remains close to the latency budget.

The important shift is:

- from full `refresh_text_metadata()` rescans
- toward edit-bounded line-ending, artifact, and line-count updates

## 2. Make large-file open staged instead of fully decoded plus fully inspected up front

The current open path still decodes the full file into a `String` and derives metadata from the whole content before the buffer is ready.

The important shift is:

- first paint from a bounded initial representation
- deferred or staged secondary inspection

## 3. Virtualize inactive tab state

The tab probe is still paying for large amounts of live state.

The important shift is:

- from full `WorkspaceTab` materialization for every inactive tab
- toward lighter inactive descriptors or snapshots with promoted live state only on activation

## 4. Split background work into purpose-specific lanes only after the synchronous hot paths are reduced

The single background worker is a real limitation, but it is probably not the main reason the current capacity probe stays flat.

It matters more after the dominant synchronous rescans and eager state construction have been reduced.

## 5. Keep using Zed as an architectural standard, not just a data-structure checklist

The most useful lesson from the Zed review is that performance comes from consistency.

Scratchpad already has some of the right primitives:

- piece tree
- snapshots
- some background processing

But the latest overview shows those primitives are not yet the default shape of the measured workflows.

Capacity will improve once the editor's surrounding operations become incremental in the same way the document core is becoming incremental.

## Conclusion

The latest open-overview run does not show major capacity improvement because Scratchpad is currently in the middle of the architectural transition, not at the end of it.

The codebase is no longer purely monolithic in the old sense, but it is also not yet Zed-like in the places that matter most to the current probe.

The latest artifacts point to a consistent conclusion:

- direct buffer construction is still whole-content work
- large paste still triggers full metadata recomputation
- tab scaling still uses eager live-state growth
- background work exists, but does not yet rewrite the synchronous hot paths that dominate the probe

So the correct reading of the latest run is not "nothing got better." It is:

the recent work improved the architecture around the editor, but the measured capacity ceilings will not move much until Scratchpad applies the same incremental design more deeply inside open, paste, metadata, and inactive-tab paths.