# Capacity Analysis Report

Date: 2026-04-19

This report summarizes why Scratchpad currently has limited capacity for very large files, very large paste operations, and high tab counts, based on the latest overview artifacts and a read-only review of the current implementation.

## Executive Summary

The current capacity limits are primarily a memory-scaling problem, not a pure CPU problem.

The latest capacity artifact shows:

- File size ceiling: responsive through 32.0 MB, first unacceptable latency at 128.0 MB
- Paste size ceiling: responsive through 8.0 MB, first unacceptable latency at 64.0 MB
- Tab count ceiling: responsive through 512 tabs, first unacceptable latency at 4096 tabs
- Split count ceiling: no failure reached through 32 splits

The important detail is that these are mostly latency ceilings, not immediate crash ceilings. The app often still completes the work, but it does so slowly enough to become unusable.

The dominant reason is architectural: the editor stores document text as a single in-memory `String`, performs whole-document scans for metadata, and eagerly materializes full `BufferState` values for open buffers and tabs. That model is simple and correct for normal files, but it scales poorly when the workload becomes large.

## Evidence From The Current Overview

The current `target/analysis/capacity_report.json` identifies 3 of 4 capacity scenarios as memory-bound.

### File size ceiling

- Last OK: 32.0 MB
- First failure: 128.0 MB
- Failure mode: `unusable_latency`
- Suspected limiting resource: `memory`
- Page-fault growth: 435,078

Interpretation: loading and representing larger files causes enough memory pressure and paging that the app remains functional but no longer responds quickly.

### Paste size ceiling

- Last OK: 8.0 MB
- First failure: 64.0 MB
- Failure mode: `unusable_latency`
- Suspected limiting resource: `memory`
- Page-fault growth: 649,552

Interpretation: large mid-buffer insertion becomes expensive both because the text storage must be reshaped and because metadata is refreshed across the resulting large document.

### Tab count ceiling

- Last OK: 512 tabs
- First failure: 4096 tabs
- Failure mode: `unusable_latency`
- Suspected limiting resource: `memory`
- Page-fault growth: 8,405,716

Interpretation: opening many tabs causes cumulative object growth and enough memory churn that latency becomes unacceptable well before an outright crash.

### Split count ceiling

- Last measured: 32 splits
- Failure mode: `not_reached`
- Suspected limiting resource: `cpu`

Interpretation: split layout work is not the main capacity bottleneck right now. Large-file and high-tab scenarios should be prioritized first.

## Why Capacity Is Limited

## 1. The editor uses a single `String` as the document backing store

`TextDocument` stores the full document as one `String` and edits it in place.

Relevant implementation points:

- `src/app/domain/buffer/document.rs`
- `src/app/domain/buffer/state.rs`

Why this matters:

- Opening a large file means the full text must exist in memory at once.
- Mid-document insertion into a `String` is expensive because the tail of the string must be moved.
- Large edits become more expensive as the document grows.
- The model provides no chunking, paging, or partial materialization.

This is the single biggest structural reason the app struggles with very large files and very large paste operations.

## 2. Metadata refresh is whole-document, not incremental

After text changes, `BufferState::refresh_text_metadata()` rescans the entire document text to recompute line counts, line endings, and artifact flags.

Relevant implementation points:

- `src/app/domain/buffer/state.rs`
- `src/app/domain/buffer/analysis.rs`

Why this matters:

- Large paste operations pay for the text mutation and then pay again for a full pass over the updated text.
- Large-file open also pays for full-text inspection during buffer construction.
- The current inspection logic walks the text character by character.

This explains why paste size and file size both hit latency ceilings quickly even though the operations still eventually complete.

## 3. File open is synchronous, full-file, and multi-pass

`FileService::read_file()` reads and decodes the entire file into a `String`. It also performs multiple passes to determine encoding, format metadata, and artifact summary before the buffer is ready.

Relevant implementation points:

- `src/app/services/file_service.rs`
- `src/app/services/file_controller/support.rs`
- `src/app/domain/buffer/state.rs`

Observed implications:

- Large files are fully decoded before the editor can present them.
- File content is inspected more than once.
- The capacity probe is in-memory only, so it understates the end-to-end cost of real file open.
- Real large-file open is likely worse than the in-memory file-size sweep because real file open also includes disk I/O and decoding.

So the file-size ceiling from the overview should be treated as optimistic for real open-file UX.

## 4. Each tab eagerly owns full tab state and buffer state

`TabManager` stores a `Vec<WorkspaceTab>`, and each `WorkspaceTab` owns its active `BufferState`, any extra buffers, its views, and its pane tree.

Relevant implementation points:

- `src/app/domain/tab_manager.rs`
- `src/app/domain/tab.rs`
- `src/app/domain/tab/layout.rs`

Why this matters:

- High tab counts grow memory roughly with the number of materialized tabs and buffers.
- The current model does not virtualize inactive tabs or downgrade them into lightweight snapshots.
- Combining tabs and creating split layouts adds more state on top of the buffer content itself.

The capacity probe uses only 48 KB per tab buffer and still reaches unacceptable latency at 4096 tabs. In real user workflows, where tabs often hold much larger text buffers, the practical ceiling can be lower.

## 5. Page faults, not handle growth, are the dominant system symptom

Across the failing scenarios, the capacity report repeatedly points to memory pressure and page-fault growth rather than handle exhaustion.

That matters because it narrows the problem:

- This does not currently look like a leaking-handles problem.
- It does not primarily look like an I/O descriptor limit.
- It looks like large in-memory working sets and repeated whole-buffer work causing paging and latency collapse.

## Scenario-by-Scenario Explanation

## Very large files

Why it degrades:

- The full file is decoded into memory.
- Format and artifact inspection scan the content.
- Buffer construction scans the content again for line and artifact metadata.
- The editor keeps the full document resident as one `String`.

Result:

- 32 MB is still tolerable.
- By 128 MB, latency is already beyond the threshold.
- At larger sizes, the app may still finish, but only after seconds rather than an interactive delay.

## Very large paste operations

Why it degrades:

- The inserted text is first created as a large `String`.
- The destination document is another large `String`.
- Mid-buffer insertion requires shifting existing text.
- Metadata is then recomputed over the whole updated document.

Result:

- Small and medium pastes are fine.
- Very large pastes cross the threshold quickly because the operation is effectively compound: allocate, insert, shift, then rescan.

## Large numbers of tabs

Why it degrades:

- Each tab is a live object graph, not a lightweight descriptor.
- Each tab owns buffer state and view/layout state.
- The tab-count sweep also performs split and combine operations, which exercises more than tab-strip rendering.

Result:

- The app remains usable up to hundreds of tabs.
- Around thousands of tabs, the live in-memory model becomes too expensive for responsive interaction.

## What This Means For Product Behavior

The app is currently optimized for ordinary text-editor workloads, not extreme-capacity workloads.

That means the present design is suitable for:

- normal source files
- moderate paste sizes
- modest tab counts
- multi-pane editing at human-scale layouts

It is not yet suitable for:

- very large log files or generated files
- huge paste/import operations into already large buffers
- very large open-tab working sets

## Recommended Solutions

## Priority 1: Replace the single-`String` document model for large buffers

Best fix:

- Move to a rope, piece table, or similarly chunked text representation.

Why:

- This addresses the root cause for both large-file and large-paste scalability.
- It reduces the cost of mid-buffer edits.
- It enables future partial loading and incremental analysis strategies.

Expected impact:

- Highest long-term improvement for file size and paste ceilings.
- Significant implementation cost.

## Priority 2: Make text metadata incremental or lazy

Best fix:

- Stop rescanning the whole document after every large edit.
- Update line counts and artifact flags incrementally where possible.
- Defer expensive artifact scans until idle time or explicit inspection.

Why:

- This is the most direct improvement for paste latency.
- It also reduces the open-file tax after buffer construction.

Expected impact:

- Medium to high benefit.
- Lower risk than a full document-model replacement.

## Priority 3: Add a large-file mode

Best fix:

- Detect large files up front and open them with reduced features.
- Examples: read-only mode, delayed syntax-like inspection, delayed artifact scanning, reduced undo depth, or limited session persistence.

Why:

- This creates a practical near-term escape hatch even before a deeper buffer redesign.
- It makes failure predictable instead of surprising.

Expected impact:

- High user-visible improvement for large-file open.
- Does not fully solve editing scalability by itself.

## Priority 4: Reduce memory per inactive tab

Best fix:

- Keep only active or recently used tabs fully materialized.
- Convert inactive tabs into lightweight snapshots or descriptors.
- Reload full buffer content lazily on activation when acceptable.

Why:

- The tab ceiling is fundamentally about cumulative live state.
- Virtualizing only the tab-strip UI is not enough; the data model must also become cheaper.

Expected impact:

- High benefit for very large tab counts.
- Requires careful UX and session-restore decisions.

## Priority 5: Avoid repeated full-file passes during open

Best fix:

- Collapse inspection steps so format detection, artifact detection, and line metadata do not each require separate whole-text passes.
- Consider streaming or staged decode for very large files.

Why:

- Real open-file latency is currently paying for multiple synchronous passes.
- This is a concrete optimization opportunity even without changing the editor core immediately.

Expected impact:

- Moderate benefit for large-file open.
- Limited benefit for paste and tab-count ceilings.

## Priority 6: Add user-facing safeguards

Best fix:

- Warn before opening unusually large files.
- Warn before very large paste operations.
- Offer alternatives such as read-only open, partial open, or cancel.

Why:

- This does not solve the underlying scalability issue, but it prevents surprising lockups.

Expected impact:

- Good short-term UX protection.
- Low engineering risk.

## What Not To Prioritize First

- Split layout optimization should not be first; the current report did not hit a split ceiling.
- Another CPU flamegraph should not be the first response for file-size, paste-size, or tab-count ceilings; the current evidence points more strongly to memory scaling and page-fault pressure.
- Tab-strip rendering polish alone is unlikely to materially change the tab ceiling because the data model already becomes expensive before the UI can save it.

## Suggested Next Measurement Work

To sharpen the diagnosis further, the next profiling pass should focus on memory behavior rather than only CPU stacks.

Recommended follow-up measurements:

- Allocation profiling for large-file open
- Allocation profiling for large paste into a large buffer
- Working-set and page-fault tracking while scaling tab count
- Real file-backed large-file tests, not only synthetic in-memory probes
- Session persist and restore cost with hundreds or thousands of tabs

## Bottom Line

Scratchpad's current capacity limits come from a straightforward but non-scalable editor core:

- full documents stored as single `String` values
- whole-document rescans for metadata
- eager, fully materialized tab and buffer state

That architecture is adequate for normal editing, but it creates predictable latency collapse as file size, paste size, and tab count grow.

If the goal is materially better capacity, the most important change is to move away from whole-document `String` editing plus whole-document rescans. Everything else is a secondary optimization or a temporary guardrail.