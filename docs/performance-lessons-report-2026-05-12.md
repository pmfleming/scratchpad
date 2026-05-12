# Performance Lessons Report

Date: 2026-05-12

This report summarizes what we learned during the recent seven-promises performance pass. The work focused most heavily on `Large Files`, `Large Text Mutation`, and cross-cutting concurrency/responsiveness, while checking the other promises for regressions after each change.

## Performance Refactor

The biggest measured gain landed in file-backed large-file startup: the 2 GB first visible paint probe improved from `7807.05 ms` to `3645.77 ms`, a `53.3%` reduction and `2.14x` speedup.

The cleanest isolated code-path gain was snapshot creation. Replacing an all-spans line-count scan with piece-tree metrics plus a tail-character check moved the direct snapshot profile from about `1801.00 ms` to `13.56 ms` for 128 snapshots of a 4 MiB buffer, a `132.8x` warm-run speedup.

The strongest structural memory win was bounding the provenance store. The store now caps at `16,384` entries and compaction rewrites retained add-buffer spans while dropping cold entries. The current probe suite does not measure RSS or retained provenance bytes directly, but the behavior is verified by tests and prevents unbounded long-session growth.

Several probe results are noisy, especially synthetic ceiling probes that do not isolate the changed path. We should treat the direction of repeated, targeted measurements as more trustworthy than a single ceiling number.

## What Changed

### Piece Tree Edit Costs

We removed several repeated whole-tree costs from normal edit paths:

- `PieceTreeRoot::recalculate` is no longer the default answer after every splice. Replaced node ranges update cached root metrics and prefixes from recalculated replacement nodes.
- Packing now computes leaf metrics once during leaf construction, then node/root recalculation sums existing child metrics where possible.
- `replace_leaf_span` now moves affected leaf vectors with `std::mem::take` instead of cloning every leaf around the splice.
- `rebalance_node_window` has a healthy-window fast path and avoids repacking balanced neighbors.

Likely effect: fewer `O(total pieces)` and clone-heavy operations during typing, paste, cut, undo, and redo on fragmented buffers. The current paste probes still show mixed timing because they are broad and allocation-sensitive, but the algorithmic shape is better.

### Anchor and View Lookup Locality

Leaf lookup by `LeafId` moved from node-by-node scanning to an index map rebuilt at structural boundaries.

Likely effect: many cursors, selections, search endpoints, scroll anchors, and many simultaneous views should scale better during edits. The current view probes improved overall, but they do not isolate anchor-heavy edit cases.

### Search Preview Rendering

Edited-buffer match preview rendering moved from per-match traversal to a batched pass over piece spans.

Likely effect: search result rendering for edited, fragmented buffers becomes `O(buffer + matches)` instead of `O(matches * leaves)`. The built-in search probes measure match throughput and target collection more than preview rendering, so this needs a dedicated edited-buffer preview probe.

### History Accounting

`history_byte_usage` is now maintained as a running total instead of resumming every history entry and span after each operation.

Measured/likely effect: less repeated history-budget work after edits. This matters most for long undo histories and large payloads.

### Snapshot Line Count

`DocumentSnapshot::from_shared` no longer walks every span to compute display line count. It uses piece-tree metrics and checks whether the buffer ends with `\n`.

Measured effect: direct snapshot profile improved `132.8x` on warm runs. This should benefit search, save, session persistence, restore, and metadata refresh paths that snapshot loaded documents.

Tradeoff: the snapshot path now follows the piece-tree newline metric rather than doing full inspection work. Detailed text-format metadata still comes from inspection paths.

### Provenance Store Bound

The provenance store is now bounded and more compact:

- keyed by `(buffer, start_byte)` plus stored length, instead of full `ByteSpan` as the map key;
- FIFO capped at `16,384` entries;
- rewritten during add-buffer compaction for retained visible/history spans;
- cold, unreferenced entries are dropped.

Positive tradeoff: memory is bounded for long edit sessions.

Cost/tradeoff: very old provenance metadata can age out. That is acceptable because provenance is advisory metadata, while undo/redo correctness remains in history payloads/spans.

### Large File Loading Experiment

We tried direct-to-`String` and chunk-fed piece building ideas for large UTF-8 loads. They were not retained.

What we learned: the existing full-buffer path is faster for the current probes. Direct `read_to_string` reduced one intermediate buffer in theory, but measured slower on 2 GB opens. Chunk-fed piece building was slower still because it lost the existing parallel piece construction path.

Tradeoff retained: keep the single existing load path until a peak-memory probe proves the memory win is worth any load-time cost.

## Final Promise / Measured Result Table

This table compares the first saved baseline from this pass against the current best probe outputs. All rows still reported `ok`.

| Promise | Measured Result | Likely / Unmeasured Improvement |
|---|---|---|
| Large Files | 1 GB file ceiling: `571.97 -> 515.96 ms`, **9.8% faster**. 2 GB first visible paint: `7807.05 -> 3645.77 ms`, **53.3% faster**. Layout ceiling: `1688.15 -> 1744.94 ms`, `3.4% slower`. | Peak memory during UTF-8 load is still not measured. We intentionally kept the faster full-buffer path after direct-to-`String` and chunk-fed experiments regressed load time. |
| Many Files | 50k file ceiling: `1710.66 -> 2101.59 ms`, `22.9% slower`. Resource tracking: `1684.65 -> 1733.07 ms`, `2.9% slower`. | Concurrency and snapshot improvements should help workflows that snapshot/search many loaded files, but the current many-file probe does not isolate those paths. |
| Search | 10k target search: `3.59 -> 3.34 ms`, **6.8% faster**. 10k target resource tracking: `3.52 -> 3.19 ms`, **9.4% faster**. 1 GB search ceiling: `405.54 -> 486.51 ms`, `20.0% slower`. | Edited-buffer preview rendering should improve substantially because previews now batch over spans. The probe suite does not yet time this separately from matching. |
| Many Tabs | 20k tabs: `3142.72 -> 3595.47 ms`, `14.4% slower`. 10k tab resource tracking: `1493.92 -> 1602.69 ms`, `7.3% slower`. | Snapshot line-count and history accounting should help tab-heavy persistence/search operations, but tab switching/reorder costs still need a focused follow-up. |
| Many Views | 1k views: `10.40 -> 9.03 ms`, **13.1% faster**. Resource tracking: `10.18 -> 9.73 ms`, **4.4% faster**. | Leaf ID indexing should reduce anchor lookup costs when many views share the same edited buffers. Anchor-heavy view probes are still missing. |
| Large Text Mutation | 128 MB paste allocation: `88.77 -> 82.29 ms`, **7.3% faster**. 512 MB paste ceiling: `361.32 -> 441.75 ms`, `22.3% slower`. | Edit paths now avoid several whole-tree recalculations/clones, history byte usage is cached, and provenance growth is capped. These are most likely to show up in fragmented long-session workloads, not one-shot paste ceilings. |
| Session Persistence Restore | Persist: `9074.23 -> 8896.73 ms`, **2.0% faster**. Restore: `354.24 -> 361.95 ms`, `2.2% slower`. Startup visible restore: `348.65 -> 360.09 ms`, `3.3% slower`. | Snapshot creation improved dramatically in isolation, so session/save/search paths that create many snapshots should benefit. Existing session probes appear dominated by other costs. |

## Where the Gains Were

The gains were strongest when we eliminated broad repeated work:

- snapshot line count changed from walking all spans to reading cached metrics;
- history byte accounting changed from repeated summation to a running total;
- piece-tree structural edits changed from whole-tree recalculation to replacement-range metric updates;
- search preview rendering changed from repeated per-match tree traversal to a single ordered pass.

The most important lesson: the project benefits more from removing repeated whole-structure walks than from adding special-case fast paths. This fits the user requirement to keep one path for small and large workloads.

## Where We Accepted Tradeoffs

We accepted bounded metadata retention in the provenance store. The positive is predictable memory in long sessions; the cost is that very old, cold provenance entries can be evicted. This does not remove undo payloads or visible text.

We rejected a potential memory optimization for loading because it slowed the measured large-file open path. The right next step is measurement, not a second implementation path.

We accepted a small semantic narrowing in snapshot line counting by relying on piece-tree newline metrics rather than full text inspection. Detailed line-ending and control-character metadata still belongs to the inspection pipeline.

We also accepted that some small-load timing shifts may regress by a few milliseconds if the single path is simpler and scales better. The larger risk is maintaining divergent small/big implementations.

## Measurement Gaps Found

The current suite is useful, but it does not yet prove several capacity claims:

- peak RSS / allocator high-water mark during very large UTF-8 load;
- edited-buffer search preview rendering with many matches and many pieces;
- provenance-store retained memory after hundreds of thousands of edits and history-budget eviction;
- anchor-heavy editing with many views, selections, search results, and scroll anchors;
- fragmented-buffer paste/cut/undo/redo after long sessions rather than one-shot paste into a relatively regular buffer;
- session persistence broken down into snapshot cost, serialization cost, file I/O, and restore reconstruction.

These gaps are tracked in `docs/unmeasured-capacity-issues.md` where applicable.

## Path To Achieving All Seven Promises

The path is plausible, but it requires turning the promises into continuously measured contracts.

1. Add missing probes for the unmeasured capacity issues above. This is the highest leverage next step because several structural wins are currently invisible to the dashboard.

2. Separate noisy ceiling probes from targeted path probes. Ceiling probes tell us whether the promise still passes; targeted probes tell us whether a specific change worked.

3. Continue removing whole-structure walks from shared hot paths. The next candidates should be session/tab persistence internals, tab set manipulation, and fragmented-buffer mutation.

4. Treat memory as a first-class metric. The provenance fix shows that some wins are not CPU wins; they are bounded-growth wins. Large-file loading needs the same treatment with peak RSS.

5. Keep the single-path rule. The experiments showed that separate "large file" paths can regress speed and add complexity. Prefer one scalable implementation, even if small workloads give up a few milliseconds.

6. Add regression gates around the seven promises. Every change should report the same promise table, plus explicit likely/unmeasured effects.

## Bottom Line

The project is closer to the promises than before, especially for large-file visible startup, snapshot-heavy workflows, many-view anchor locality, edited-buffer search preview rendering, and long-session memory control.

The remaining work is not mainly about finding one dramatic optimization. It is about making the current algorithmic improvements visible in the test suite, then continuing the same pattern: remove repeated global scans, bound retained metadata, and keep the implementation single-path and scalable.
