# Fredbuf-Inspired Buffer Performance Experiments

Date started: 2026-08-21

This log records isolated experiments inspired by `cdacamar/fredbuf`. Implementations are retained only when targeted Scratchpad Performance Lens measurements improve without a material regression in the relevant seven-promise guardrails. Raw run logs and copied artifacts are retained locally under `target/analysis/experiments/` and are intentionally not committed.

## Measurement protocol

- Run the relevant Criterion target at least three times before and after when practical.
- Regenerate the corresponding Scratchpad Performance Lens report using the local sibling checkout.
- Run buffer-domain tests, formatting, and Clippy for retained implementations.
- Check broad resource/promise guardrails for structural changes.
- Treat Criterion's comparison with an immediately preceding run as noise-sensitive; decisions use the absolute before/after ranges and resource measurements as well.

The initial full-lens baseline completed successfully. Promise health was: Large Files pass, Many Files pass, Search fail at the measured 1 GiB completion threshold, Many Tabs pass, Many Views pass, Large Text Mutation fail at the measured paste threshold, and Session Persistence Restore pass. The machine was substantially slower than the July reference run, so experiments are judged against same-session measurements rather than old Criterion history.

## Experiment 1 — Revision-shared append-only add storage

**Status: retained.**

### Change

Replaced the monolithic cloned add-buffer `String` with edit-boundary chunks backed by `Arc<String>`. Small consecutive edits share a copy-on-write tail capped at 256 KiB; large inserts receive a dedicated immutable chunk. Existing absolute `ByteSpan` semantics, compaction, provenance, and persisted history remain intact.

Added `snapshot_shared_edit_latency/16777216`, which edits a document containing 16 MiB of inserted text while a worker snapshot remains alive.

### Results

- Before: 26.91–29.40 ms.
- After: 0.187–0.208 ms across three runs.
- Improvement: approximately 99.3%, or roughly 140x.
- Ordinary 128 MiB paste insertion remained approximately 106–109 ms and Criterion classified the repeated comparison as no significant change/noise-threshold change.
- Buffer-domain tests: 75 passed.

### Promise impact

Improves snapshot-overlapped editing for Large Files, Search, Many Views, Large Text Mutation, and Session Persistence without a measured ordinary-paste regression.

## Experiment 2 — Persistent/path-copy internal nodes

**Status: reverted.**

### Change attempted

Stored packed internal nodes behind `Arc` and copied only touched nodes when a shared fragmented tree was edited. Packed leaves and existing tree semantics were retained.

Added `snapshot_shared_fragmented_edit_latency/20000`.

### Results

- Before: typically 1.23–1.56 ms.
- Prototype: typically 0.075–0.132 ms, about 92–95% faster.
- However, broad resource checks found regressions outside the targeted case:
  - 50,000-file peak live heap increased from about 156.9 MiB to 163.4 MiB.
  - Provenance long-session cumulative allocation increased from about 24.2 GiB to 33.6 GiB.
  - Ordinary paste Criterion samples became highly unstable, with materially slower samples, although the allocation probe itself was flat.

### Decision

Reverted. Per-node reference counting and allocations penalized Many Files and long edit sessions. A future persistent representation would need activation only for genuinely shared large/fragmented revisions, without adding an allocation to every small buffer.

## Experiment 3 — Bounded root checkpoints for large undo/redo

**Status: retained.**

### Change

History entries for operations of at least 1 MiB may retain before/after `Arc<PieceTreeLite>` checkpoints. Exact-generation undo/redo with no live anchors switches to the checkpoint; operation replay remains the fallback and the authoritative persisted/provenance-aware history model. Small edits, imported history, conflicts, and anchor-bearing documents retain the existing path.

Added `large_paste_undo_latency/16777216`.

### Results

- Before: 52.56–79.13 ms across three runs.
- After: 3.29–4.37 ms across three runs.
- Improvement: approximately 94% at the median range.
- Ordinary 128 MiB paste insertion remained approximately 102–111 ms across three post-change runs, matching the stable pre-change range.
- Buffer-domain tests: 75 passed.

### Promise impact

Directly improves Large Text Mutation and large-file undo/redo. Checkpoints are bounded by the existing history budget lifecycle and are omitted where anchors require operation replay.

## Experiment 4 — Balanced upper-level metric index

**Status: retained with a threshold.**

### Change

Prototyped a Fenwick-tree summary over top-level packed nodes. This provides logarithmic prefix lookup and updates when edits preserve the node count, while structural node-count changes rebuild the summary. The first version used it for every document; viewport measurements showed that this penalized the overwhelmingly common one-node case. The retained version keeps the original flat prefix representation below 64 nodes and activates the balanced index only for fragmented trees.

Added `fragmented_tree_edit_latency/20000`.

### Results

- Before: medians approximately 29–30 microseconds.
- Thresholded balanced index: medians approximately 16–25 microseconds under a noisy machine load, with the clean first comparison at 14.15 microseconds (about 50% faster).
- Ordinary 128 MiB paste insertion remained approximately 102–105 ms and Criterion reported no change.
- The unconditional prototype regressed viewport extraction by roughly 15–18%; it was not retained.
- With the 64-node threshold, repeated viewport comparisons reported no statistically significant change from the immediately preceding viewport baseline.
- Buffer-domain tests passed, including randomized edits and line lookup.

### Promise impact

Improves heavily fragmented Large Files, Search navigation, Many Views, and Large Text Mutation while preserving the small-tree path used by ordinary files and large contiguous pastes.

## Experiment 5 — Seekable forward/reverse character cursor

**Status: retained.**

### Change

Added `PieceTreeCharCursor`, which seeks once, retains the active backing piece, and walks UTF-8 safely in either direction without repeating tree lookup and `chars().nth(...)` prefix scans. Word-boundary scans now use the cursor. Existing span iterators remain the preferred bulk-text API.

Added `reverse_character_walk_latency/8192`.

### Results

- Before: 7.28–8.21 ms.
- After: 77.8–78.7 microseconds across three runs.
- Improvement: approximately 98.9%, or roughly 95x.
- Cursor and word-boundary tests cover forward/reverse UTF-8 movement and piece boundaries.

### Promise impact

Improves Unicode cursor/word navigation for Large Files and Many Views. The API is additive and does not change storage size or bulk traversal.

## Experiment 6 — Compact lazy line-start samples

**Status: retained after revising the eager prototype.**

### Change

The initial prototype indexed every 64th newline while each piece was built. It made dense line lookup dramatically faster but added a second text pass and regressed 128 MiB paste insertion by about 15–21%, so that version was rejected. The retained implementation stores a thread-safe lazy sample index in each dense-line piece. The first line query builds one `(character, byte)` sample per 64 newlines; subsequent line and offset queries scan at most the short remainder. Pieces with fewer than 64 newlines allocate no sample array.

Added `dense_line_lookup_latency/4194304`, resolving 1,024 distributed lines in a 4 MiB dense-line document.

### Results

- Before: 116.9–151.4 ms, with the clean baseline near 128.7 ms.
- Retained lazy index: approximately 82–108 microseconds.
- Improvement: over 99.9%, roughly three orders of magnitude on the dense-line workload.
- The eager prototype moved 128 MiB paste insertion from about 103 ms to 119–125 ms and was rejected.
- After making samples lazy, a stable paste rerun measured 105.6–109.3 ms, matching the pre-experiment range; the first rerun was affected by the same machine variance seen elsewhere.
- Viewport measurements remained noisy (roughly 14–15 ms at 4 MiB) and did not show a consistent retained regression.
- Buffer-domain tests passed.
- A first lazy implementation stored the cache in every reserved piece slot and increased the 50,000-file peak heap by about 15 MiB. It was replaced with a tree-level lazy side cache. After that correction, 50,000-file peak heap was 160.4 MiB versus the 156.9 MiB baseline (+2.2%), while cumulative allocation was within 0.3%; fragmented-session peak heap improved from 17.3 MiB to 15.1 MiB.

### Promise impact

Improves line navigation, scrolling, search previews, and many-view cursor mapping for Large Files, Search, and Many Views. Lazy construction avoids adding work to paste/file ingest until line queries actually need the index.

## Final validation notes

- `cargo test --lib`: 452 tests passed after all retained changes (later focused runs increased the buffer-domain count to 78).
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --all-targets` reached the Criterion bench executables and hit an existing egui `TexturesDelta` teardown panic in bench test mode; normal `cargo bench` and the full Performance Lens benchmark workflow completed.
- The final full lens run completed, but system load was not controlled: several unrelated long-running `pi`, app-daemon, Electron, and Scratchpad processes were active, load average was approximately 10, and CPU frequency was observed near 1.4 GHz. Broad wall-clock rows varied by 2–3x in both directions during the session. Consequently, acceptance decisions use repeated targeted comparisons plus allocator measurements; raw final artifacts remain under `target/analysis/experiments/final/`.
- After reducing retained-structure overhead and restoring the single-chunk compaction fast path, key resource comparisons versus the initial baseline were:
  - 100,000-edit provenance allocation: 24.81 GiB versus 24.80 GiB (flat).
  - 20,000-fragment mutation allocation: 132.7 MiB versus 134.6 MiB (improved); peak heap 15.1 MiB versus 17.3 MiB (improved).
  - 128 MiB paste allocation: 258.79 MiB versus 258.79 MiB (flat).
  - 50,000 files: cumulative allocation +0.27%, peak heap +2.2%.
  - 10,000-tab restore: cumulative allocation +0.28%, peak heap +1.0%.
