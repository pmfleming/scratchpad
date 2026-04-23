# Why Zed's Editor Is Fast

This report reviews Zed's source and internal performance docs to explain why the editor feels fast.

It is intentionally source-backed rather than benchmark-driven. The goal is not to claim that every fast path is fully proven from one file, but to identify the architectural choices that most directly explain Zed's responsiveness.

## Executive Summary

Zed's editor is fast because it is incremental almost everywhere.

It does not treat the document as "one big string that many features repeatedly rescan." Instead, it uses summarized tree structures for text storage and coordinate conversion, layered display transforms that propagate narrow invalidation regions instead of full redraw/recompute ranges, snapshot-based reads, and an engineering culture that explicitly watches foreground versus background work.

That combination matters more than any single optimization:

- the text model is built for localized edits and fast seeking
- the display model is built for localized invalidation
- the codebase has explicit support for parallel/background work where structure construction allows it
- the team profiles the editor as a first-class discipline

## Source Review

Primary sources reviewed:

- [crates/text/src/text.rs](https://github.com/zed-industries/zed/blob/main/crates/text/src/text.rs)
- [crates/sum_tree/src/sum_tree.rs](https://github.com/zed-industries/zed/blob/main/crates/sum_tree/src/sum_tree.rs)
- [crates/editor/src/display_map.rs](https://github.com/zed-industries/zed/blob/main/crates/editor/src/display_map.rs)
- [docs/src/performance.md](https://github.com/zed-industries/zed/blob/main/docs/src/performance.md)

## 1. The text buffer is built for incremental work, not whole-buffer churn

In `crates/text/src/text.rs`, Zed's `BufferSnapshot` is not just a plain `String`.

It stores:

- `visible_text: Rope`
- `deleted_text: Rope`
- `fragments: SumTree<Fragment>`
- `insertions: SumTree<InsertionFragment>`
- versioning, undo, anchors, and coordinate conversion support

That has several performance consequences:

- localized edits do not require rebuilding one giant contiguous text buffer
- undo/redo and operational history are represented structurally rather than as repeated full-text copies
- common editor coordinate conversions can be derived from summaries instead of rescanning text
- immutable snapshots make read-heavy operations easier to isolate from mutation

The file also contains a very revealing memory-conscious detail:

- `MAX_INSERTION_LEN` exists so large insertions can be split and represented with relative `u32` offsets instead of `usize`, explicitly to reduce memory usage

That is the kind of choice fast editors tend to make: not just "use a rope", but shape the rope-adjacent structures so metadata and edit history remain compact.

## 2. `SumTree` gives Zed fast seeking, slicing, and coordinate conversion

In `crates/sum_tree/src/sum_tree.rs`, Zed defines `SumTree` as a B+ tree-like structure where every node stores summaries for the subtree below it.

Those summaries can expose multiple dimensions at once. The code explicitly supports seeking by different dimensions and combining dimensions through `Dimensions<D1, D2, D3>`.

Why this matters for editor speed:

- the editor can seek by offsets, points, rows, counts, or other summarized coordinates without flattening the buffer
- large parts of the system can answer "where is this thing?" by walking summaries instead of scanning raw text
- slices and cursors are first-class operations on the tree
- transformed coordinate spaces can stay structural

This is one of the biggest reasons Zed scales well: the editor architecture keeps converting "linear scan work" into "tree walk with summaries."

That same file also shows explicit support for concurrency in structure construction:

- `SumTree::from_par_iter(...)`

This does not mean "everything is magically parallel," but it does show the data structure was designed to support parallel construction where that is profitable and safe.

## 3. The display pipeline is layered so edits only invalidate what changed

The strongest editor-specific evidence is in `crates/editor/src/display_map.rs`.

Zed's display model is not one monolithic "render text" pass. The module-level docs describe a stack of layers:

- `InlayMap`
- `FoldMap`
- `TabMap`
- `WrapMap`
- `BlockMap`
- top-level `DisplayMap`

Each layer has:

- a transform
- a transform summary
- coordinate conversion helpers
- iterators over rows/chunks
- a `sync` method that consumes lower-layer edits and returns transformed invalidation edits upward

The docs make an especially important point:

- using one invalidation region covering the whole range would be correct, but would cause unnecessary recalculation

That is a very strong signal about why Zed stays fast. The architecture is explicitly designed to avoid whole-range recomputation when a smaller invalidation region is enough.

In practice, that means:

- a text edit does not automatically force every display concern to recompute globally
- wrapping, inlays, folds, tab expansion, and block layout can be updated through narrower patches
- the display pipeline stays incremental across multiple coordinate systems

This is a much deeper advantage than "fast rendering." It means the editor avoids creating unnecessary work before rendering even begins.

## 4. Zed treats snapshots as a core abstraction

Across the reviewed files, snapshot types are everywhere:

- `BufferSnapshot`
- layer snapshots in the display map stack
- immutable read views used to derive transformed state

That matters because snapshots make it easier to:

- do read-heavy operations without disturbing live mutable editor state
- hand work across subsystem boundaries cleanly
- recalculate derived structures from a known revision
- apply bounded background work only when the source revision still matches

The source does not reduce this to one slogan, but the design strongly suggests that Zed's speed comes partly from giving subsystems stable, structural read views instead of repeatedly asking live mutable state to materialize a full text image.

## 5. The project explicitly monitors foreground and background work

Zed's internal performance guide in `docs/src/performance.md` is unusually direct:

- developers are told to inspect CPU time with flamecharts and tracing
- async/task profiling is part of the documented workflow
- the docs explicitly say to check whether anything blocks the foreground executor too long or takes too much clock time in the background

That matters because fast editors are rarely fast by accident.

Zed appears to combine:

- architecture that permits bounded work
- instrumentation that catches regressions when someone accidentally reintroduces unbounded work

This is probably one of the reasons the editor remains fast as features accumulate. The codebase is not only designed for incremental work; the team also has process and tooling to defend that design.

## 6. What Zed is probably not doing on the hot path

Based on the reviewed source, the editor's fast path is probably not dominated by:

- flattening the entire buffer into a fresh `String` for common cursor/display operations
- recomputing all display transforms from scratch on every edit
- forcing every feature to work in one coordinate system
- letting foreground/UI work silently absorb expensive background-worthy tasks

That is the key contrast with slower editors and slower editor paths: once a system falls back to whole-buffer materialization and whole-range invalidation too often, latency rises quickly even on machines with a lot of RAM.

## 7. The clearest lessons from Zed's design

If we translate Zed's design into portable lessons, the highest-value ones are:

1. Make summarized structural reads the default.
   Fast editors do not keep rediscovering document structure by rescanning the whole text.

2. Push coordinate conversion into first-class data structures.
   Offset-to-point, point-to-display-point, wrap-row mapping, and similar conversions should be structural operations, not ad hoc scans.

3. Keep display transforms layered and incremental.
   Folds, wraps, inlays, tabs, highlights, and blocks should consume bounded invalidations and produce bounded invalidations.

4. Prefer snapshots over shared mutable hot paths.
   Stable snapshots make background work and revision-safe recomputation practical.

5. Treat foreground blocking as a product bug.
   Zed's profiling docs strongly imply this mindset.

6. Build performance tooling into normal development.
   Fast systems stay fast when regressions are visible early.

## 8. Practical relevance for Scratchpad

For Scratchpad, the most relevant lessons are not "copy Zed wholesale."

They are:

- reduce the remaining contexts where document state is flattened into a full `String`
- move more editor-adjacent work onto summarized piece-tree or rope-native paths
- make metadata and display derivations revision-safe and incrementally invalidated
- expand bounded background preparation where snapshots make it safe
- keep profiling foreground latency, not just total throughput

The strongest source-backed takeaway is that Zed's speed is architectural. It comes from minimizing unnecessary work at the data-structure, display-transform, and execution-model levels all at once.

## Conclusion

Zed's editor is fast because its architecture consistently prefers:

- summarized trees over monolithic text
- snapshots over repeated whole-buffer materialization
- narrow invalidations over full recomputation
- explicit foreground/background separation over accidental blocking
- continuous profiling over intuition-only performance work

There is no single "magic" optimization in the reviewed code. The speed comes from the fact that the same incremental philosophy shows up in the text model, the display map, the concurrency hooks, and the development process.
