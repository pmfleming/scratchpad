# Latest Capacity Report Analysis

Date: 2026-04-23

## Question

Why does Scratchpad become unusable at what look like very small sizes, such as 128 MB file-size sweeps and 256 MB paste-size sweeps, even on a machine with 64 GB of RAM?

## Executive Summary

The latest capacity report is mostly showing interactive latency ceilings, not true out-of-memory limits.

The app is not failing because 128 MB or 256 MB is close to exhausting a 64 GB machine. It is failing because the measured operations already take far longer than the report's usability budgets:

- file size ceiling budget: 160 ms
- paste size ceiling budget: 150 ms
- tab count ceiling budget: 140 ms

On the latest run:

- file size sweep is last OK at 32.0 MB and first fails at 128.0 MB
- paste size sweep is last OK at 64.0 MB and first fails at 256.0 MB
- tab count sweep is last OK at 512 tabs and first fails at 4096 tabs
- split count sweep does not fail through 32 splits

The root cause is that the editor still does too much whole-buffer work per operation:

1. large-file open still decodes the whole file into a `String`, inspects the whole text, and then builds the piece tree from the full text
2. large paste still triggers whole-document metadata refresh after mutation
3. high tab counts still materialize full live tab, buffer, view, and pane state instead of lightweight inactive tabs

The piece tree is helping, but it has not yet changed the fact that open, paste, and tab-scale workflows still do large, synchronous, full-document work.

## What The Capacity Report Actually Means

The capacity report at `target/analysis/capacity_report.json` marks a scenario as failed when it exceeds a latency threshold, not only when it crashes.

That is defined in `scripts/capacity_report.py`:

- `file_size_ceiling`: 160 ms
- `paste_size_ceiling`: 150 ms
- `tab_count_ceiling`: 140 ms
- `split_count_ceiling`: 120 ms

So "first failure at 128 MB" means "the operation took too long to stay interactive", not "the machine ran out of RAM at 128 MB".

This matters because the report is answering a product question:

- can the app stay responsive?

It is not answering the narrower system question:

- can Windows still allocate memory for the process?

On a 64 GB machine, those are very different thresholds.

## Evidence From The Resource Profiles

The latest `target/analysis/resource_profiles.json` makes the problem clearer.

### File-backed large-file open

At 128.0 MB:

- elapsed: 603.6 ms
- allocated bytes: 167.8 MB
- peak live bytes: 167.8 MB

This is nowhere near exhausting 64 GB of RAM. The problem is that the open path is already about 4x the 160 ms usability budget.

### Large paste allocation profile

At 64.0 MB:

- elapsed: 162.1 ms
- allocated bytes: 135.6 MB
- peak live bytes: 135.4 MB
- allocation count: 3430
- reallocation count: 162

Again, the issue is not total machine memory. The issue is synchronous mutation plus rescanning work that already exceeds the interactive budget even though the absolute allocation size is moderate.

### Tab count resource tracking

At 4096 tabs:

- elapsed: 816.5 ms
- allocated bytes: 214.8 MB
- peak live bytes: 211.5 MB

This is also far below physical RAM capacity. The bottleneck is the amount of live object graph and per-tab work the app performs, not total machine memory.

## Why These Operations Are Still Expensive

## 1. Large-file open is still whole-file and multi-pass

`src/app/services/file_service.rs` reads the whole file into a `String` with `decoder.read_to_string(&mut content)`.

After that:

- encoding and artifact inspection are derived from the full text
- `BufferState::with_format` computes text metadata from the full content
- `TextDocument::with_preferred_line_ending` builds a piece tree from that full `String`

So the current open path is still:

1. decode full file
2. inspect full file
3. build editor structure from full file

The piece tree improves edit structure, but it does not yet make file open incremental or demand-driven.

## 2. Large paste avoids tail-shifting the whole file, but still pays for whole-document refresh

The current document is piece-tree-backed in `src/app/domain/buffer/document.rs`, which is an improvement over the older single-`String` model.

That helps because the insertion itself no longer depends on physically shifting the tail of one giant `String`.

But the current edit path still does expensive follow-up work:

- `BufferState::refresh_text_metadata()` runs after large edits
- `buffer_text_metadata_from_piece_tree()` walks spans across the whole document
- `TextInspection::inspect_spans()` recomputes line counts and artifact state by iterating the full text

So large paste is still effectively:

1. append inserted text into the piece-tree add buffer
2. rebuild piece metadata for the edit
3. rescan the entire document for line endings and artifact summary

That is why the app can fail on responsiveness well before it is anywhere near exhausting RAM.

## 3. Tab scaling is dominated by eager live state, not by 64 GB versus 256 MB

`WorkspaceTab` owns:

- an active `BufferState`
- extra buffers
- view state
- a pane tree

`TabManager` stores all tabs in memory as live `WorkspaceTab` values.

So even with small per-buffer payloads, the app still pays for:

- buffer construction
- line and artifact metadata
- view and pane structures
- combine and split operations across many tabs

The resource profile uses only 48 KB per tab buffer and still reaches 816.5 ms at 4096 tabs. That strongly suggests the ceiling is driven by eager state construction and aggregate per-tab work, not a shortage of machine RAM.

## What The Flamegraphs Add

The flamegraphs are useful, but only in a limited way here.

### Large paste flamegraph

The `large_file_paste_profile` flamegraph mostly points at app-side text and metadata work, especially:

- piece-tree piece construction
- newline counting
- `TextInspection` character inspection

That supports the conclusion that paste cost is not just the insert itself. A meaningful amount of time is going into rebuilding and rescanning text metadata.

### Tab operations flamegraph

The `tab_operations_profile` flamegraph shows work around:

- `BufferState::new`
- `buffer_text_metadata`
- pane cloning and tab structure work

That fits the resource profile result: the app is doing too much eager construction and manipulation per tab-scale workflow.

### Split flamegraph

The `large_file_split_profile` flamegraph supports the report's split result: split work is not the primary capacity problem right now.

## Important Measurement Caveats

The latest report is directionally correct, but two details should be interpreted carefully.

## 1. `peak_working_set_bytes` is contaminated across scenarios

The report generator runs all scenarios inside one long-lived probe process and samples Windows `PeakWorkingSetSize`.

That means later scenarios can inherit earlier lifetime peaks. In `scripts/capacity_report.py`, the scenario row takes the maximum sampled `peak_working_set_bytes` seen within that scenario's event window, but Windows peak working set is process-lifetime high watermark, not "peak for only this step".

This explains why later rows show the same 25.4 GB peak working set. That number should not be read as "this individual scenario genuinely needed 25 GB resident memory right now".

## 2. per-sample working set is captured after the step completes

The probe prints an event after a step finishes, and the Python wrapper samples the process only after reading that event.

That means:

- transient peak residency inside a step is often missed
- per-step `working_set_bytes` can look surprisingly small
- allocation counters and elapsed time are more trustworthy than the post-step working set snapshot

For this reason, the resource profiles' `allocated_bytes` and `peak_live_bytes` are better evidence than the raw sampled working-set fields.

## 3. the capacity probes include fixture creation cost

`src/bin/capacity_probe.rs` measures more than just "editor core cost":

- file-size sweep generates the synthetic text inside the timed closure
- paste-size sweep creates the base buffer and inserted text inside the timed closure

That overstates pure editor cost somewhat.

However, it does not change the main conclusion, because the file-backed resource profile still shows 603.6 ms for a 128 MB real open path.

## Why A 64 GB Machine Still Does Not Save This

Having much more physical RAM helps only when the bottleneck is raw capacity.

Here, the current bottleneck is mostly synchronous work per operation:

- decode full content
- inspect full content
- build full editor representation
- rescan full content after edits
- keep many tabs fully live

Those costs scale with document size and tab count even when the system still has plenty of free RAM.

More RAM can delay paging and reduce worst-case collapse, but it does not turn a 600 ms synchronous open or an 800 ms tab-scale operation into an interactive one.

## Priority Improvement Areas

## 1. Large-file mode and staged open

Highest near-term value.

The app should detect large files early and avoid the full-feature path on initial open. The main changes should be:

- staged or chunked decoding where possible
- defer artifact inspection for large files
- defer non-essential encoding-compliance checks
- reduce large-file startup work before first paint

This is the fastest way to move the 128 MB file ceiling without a deep rewrite.

## 2. Incremental text metadata refresh

Highest structural win for paste responsiveness.

`refresh_text_metadata()` currently rescans the full document after edits. That should move toward:

- incremental line-count updates
- incremental artifact tracking
- deferred background refresh for non-critical metadata

This is the clearest fix for why large paste crosses the latency budget so early.

## 3. Inactive tab virtualization

Highest win for high-tab ceilings.

Inactive tabs should not all remain fully materialized as live buffers, pane trees, and view graphs. The app should be able to keep:

- lightweight descriptors for inactive tabs
- lazily restored editor state
- downgraded buffer presence for tabs not currently visible

The 4096-tab result is telling us that the current eager model does not scale.

## 4. Keep using the piece tree, but push its benefits into the rest of the pipeline

The move away from a single editable `String` was the right direction, but the surrounding pipeline is still largely whole-document.

The next wins come from making open, metadata refresh, snapshotting, and persistence exploit the piece tree instead of flattening or rescanning full text too often.

## 5. Fix the reporting so future reports are easier to trust

This is lower product priority than the runtime work, but it should still happen.

The reporting should separate:

- per-step peak residency
- process lifetime peak residency
- transient allocation peak

Without that separation, the working-set fields are easy to misread.

## External Reference Points

Two useful comparison points are:

- Microsoft Edit: [microsoft/edit](https://github.com/microsoft/edit)
- Zed: [zed-industries/zed](https://github.com/zed-industries/zed)

These are useful as examples, but not as one-to-one architectural proof.

From their public repo descriptions:

- Microsoft Edit presents itself as "a simple editor for simple needs"
- Zed presents itself as a "high-performance" editor and uses the tagline "Code at the speed of thought"

That matters because they illustrate two different ways fast editors stay fast:

- scope discipline: keeping the product surface intentionally smaller and avoiding expensive always-on behavior
- performance-first architecture: treating responsiveness as a primary product feature rather than a later optimization pass

For Scratchpad, both lessons are relevant:

- the Microsoft Edit example supports the case for a large-file mode with reduced feature work on first open
- the Zed example supports the case for making responsiveness a first-class architectural goal for buffer, tab, and rendering pipelines

I would use those projects as directional references for product philosophy and prioritization, not as evidence that Scratchpad should copy any specific internal design without separate investigation.

## Conclusion

The latest capacity report does not show a machine-memory problem. It shows an operation-shape problem.

Scratchpad is becoming unusable at 128 MB and 256 MB because those workflows still do too much synchronous, whole-buffer work for an interactive editor. The current piece tree has improved the edit core, but open, metadata refresh, and tab materialization still behave more like a whole-document editor than a large-scale one.

If the goal is materially better capacity on modern machines, the best priorities are:

1. staged large-file open
2. incremental metadata refresh after edits
3. inactive tab virtualization
4. tighter measurement of transient memory peaks
