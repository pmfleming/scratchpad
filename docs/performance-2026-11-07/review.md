# Three-cycle UX performance campaign

Starting revision: `d92bb472958f32821a60e02629e8bcb633a35c64` (clean worktree).
Recovery branch: `recovery/performance-20261107-d92bb47`. Lens starting revision:
`214208cac906bc677ca7efae0de5143d3c771f7b`; its pre-existing untracked
`docs/report-cycle25/database-and-memory-ideas.txt` is preserved. Nothing is pushed.

## Historical boundary and decision

**Choose (a), continue from the starting HEAD.** The last 25 commits are
`ce0c0d1..d92bb47` inclusive, not the whole failed experiment. The paired
25-cycle campaign starts at setup commit `ad488e7` (parent `3ec35dd`), with
cycle 1 at `dec2962`. Earlier probe repairs begin at `b8f8993`. The underlying
live-text Turso migration starts at `a00d0b6` and reaches stage 7 at `86ad24d`;
it is a separate boundary, not a candidate for removing relational storage.
These boundaries come from the actual log and the lens cycle-25 report's
per-cycle app/lens commit table, not an assumed HEAD-minus-25 rollback point.

| Option | Assessment |
|---|---|
| (a) Continue | Retains correctness-tested incremental improvements and the current measurement contracts. The remaining failures have specific causes that can be addressed independently. Selected. |
| (b) Revert to campaign start, reapply useful work | Would reapply virtually every production change and every contract correction. Adds conflict/reconstruction risk without removing the principal bottleneck, which predates the campaign. |
| (c) Revert to intermediate point, reapply useful work | No diff establishes a harmful shipped midpoint change worth losing later gains. Rejected drafts (16 KiB pages, scanning SQL plans, pin removal) were already excluded from commits. |

The experiment failed to pass all promises; it did not fail to improve the app.
Historical timings below are evidence for retaining code, **not current results**.
The reviewed patch archive is `target/ux-performance-20261107/history.patch`.

## Review of all 25 commits

| Commit | Relevant implementation / measurement finding | Decision |
|---|---|---|
| `ce0c0d1` | Sends selected file text before inactive shells; preserves final ordering and missing-file fallback; replaces repeated document-ID list scans with a set. Tests assert first delivery has actual text. | Keep foreground benefit. |
| `aa48a99` | Moves fixture creation outside operation timers and keeps objects alive for allocation sampling. Changes comparison scope, not app speed. | Keep; do not compare against setup-inclusive numbers. |
| `1ec8417` | One-second cached memory sample removes per-buffer sysinfo work; ASCII fast scan retains scalar control handling. | Keep; explicit budget refresh still resamples. |
| `efc5933` | Captures at most 64 small documents / 1 MiB before draining; caches bounded statement VMs, exhausts cursors, discards failed VMs. | Keep; synchronous toggles undermine VM reuse and are cycle 1's target. |
| `a253250` | FIFO body batches capped at 1 MiB with a 4 ms scheduling target; deterministic document write order. | Keep; duration is not a hard I/O deadline. |
| `9a9f405` | Arc<String> retains original allocation; wider printable-ASCII scans; adds missing 10k-piece workload. | Keep; workload addition is a measurement correction. |
| `80f319a` | Reader statement reuse and reset after failure; foreground reader-thread physical-I/O counters. | Keep; process-wide I/O cannot prove foreground cold reads. |
| `17ad6ec` | Independent vectorizable ordinary UTF-8 passes with fallback for CR/control/artifact bytes and equivalence tests. | Keep; not all Unicode takes this fast path. |
| `50af326` | Separates paste publication/render from later NORMAL flush and preparation phases. | Keep; earlier endpoint is not a durable speedup. |
| `536a959` | Inspects normalized replacement input and original spans instead of rereading sealed new bodies; Unicode/CRLF/undo coverage. | Keep. |
| `96c45fb` | Lazy line-aligned search iterator, eight-window lookahead, cancellation and exact-match tests. | Keep bounded planning; serial boundary lookup remains a hypothesis. |
| `6b7d513` | Typed json_each CROSS JOIN forces indexed history lookups; aged fixture uses real incremental undo edits. | Keep query-plan tests; 12–54 s scanning drafts were rejected. |
| `00fc1da` | Batched indexed text-manifest reads, shared corrupt-row validation, moves decoded histories instead of cloning. | Keep; bodies remain lazy. |
| `450b9e8` | Indexed global/owner LRU replaces scans of all cached chunks on touches; randomized reference-model and pinned-Arc tests. | Keep. |
| `68c7d08` | Shared fixture declarations and union of repeated scenario selections. | Keep; fewer preparations, same launches/workloads. |
| `0c8a6d2` | memchr newline scans in metadata and verification, actual page/cache configuration; existing-page reopen tests. | Keep; 16 KiB page experiment was not shipped. Old write-byte interpretation is not reliable amplification evidence. |
| `5d469a9` | Mutation preflight omits owned-tail pins that force copy-on-write; extraction still pins owned bodies. Probe asserts actual retained history. | Keep; historical 100k edits 1315 → 92 ms. |
| `ac731f4` | Geometric partial-result prefix publication bounds copying; single-use production fixture retains complete results. | Keep; historical corrected 1k-target baseline 1151 → 16 ms. |
| `e58e6e1` | Preparation/write/commit and restore subphase intervals; parent/child consistency tests. | Keep attribution, no optimization claim. |
| `569ba43` | Batches only proven-new pathless documents/history; 32-row bounded parameter blocks and power-of-two tail shapes; existing rows retain incremental path. | Keep; historical history phase 1359 → 299 ms. |
| `a709316` | Batches proven-new layouts in same FULL transaction; rollback/reopen/reorder tests; restore retained through sampling; explicit no-op transaction flag. | Keep; removed teardown is a method boundary, not faster reconstruction. |
| `b964a13` | One bounded scoped worker cohort per target, joins failures/panics; capacity uses actual production matcher and checks exact completion count. | Keep modest gain; 1 GiB remained ~1363 ms against 180 ms. |
| `77e7aab` | Final report points to paired lens evidence, failures, noise and frozen gates. | Keep historical record. |
| `0727bec` | Reports actual cache limits beside memory-derived app defaults without mutating them. | Keep; direct-store probes use 16/4 MiB, real app can use 512/64 MiB. |
| `d92bb47` | Versioned outcomes and independent probe endpoint declarations; distinguishes publication, NORMAL flush, FULL commit; unknown exposure stays null. | Keep contracts and frozen inventories. |

## Measurement rules for this campaign

Use Scratchpad's own `nix develop` environment (Rust/Cargo 1.95.0). Preserve
baseline executables and SHA-256 hashes, raw JSONL, logs and lens sidecars in
`target/ux-performance-20261107/`. Run before/after binaries interleaved without
concurrent builds for focused comparisons; use complete declared sweeps and
independent launches. Final full promise audit is separate from focused results.
Headless UI observations cover input, publication, CPU layout and tessellation,
not compositor presentation. No new latency threshold or reduced workload is a pass.

Three substantive cycles are planned: capture allocation and fixed-mode writer investigation; bounded
memory headroom for foreground bulk publication; committed-text scan residency.
Each ends in an application commit with correctness checks and measurements.
Any genuine lens defect is fixed/tested/committed between application cycles.
These three cycles do not claim to complete every item of the separate seven-
operation residency research plan; missing diagnostics remain explicitly missing.
