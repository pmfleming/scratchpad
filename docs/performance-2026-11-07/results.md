# Scratchpad performance campaign — 2026-11-07 to 2026-11-08

Exactly three application cycles were completed, retaining relational Turso.
The selected starting point was **(a), continue from `d92bb472958f32821a60e02629e8bcb633a35c64`**.
The recovery branch `recovery/performance-20261107-d92bb47` preserves that revision.
There were no unrelated application working changes. No history was destroyed and
nothing was pushed. The lens's pre-existing untracked
`docs/report-cycle25/database-and-memory-ideas.txt` was preserved unchanged.

## Baseline decision and engine research

[The review](review.md) covers the last 25 commits and relevant diffs, anchored at
the actual starting HEAD. That window is `ce0c0d1..d92bb47` inclusive. It is not
the experimental boundary: the paired 25-cycle campaign setup was `ad488e7`,
parent `3ec35dd`, with first implementation `dec2962`. The preceding live-text
Turso migration was a separate history segment. Reverting to either boundary or
an intermediate revision would require reapplying most useful code and corrected
probes; several failed assumptions concern bottlenecks that predate that campaign.
Continuing preserves useful bounded queues, reader isolation, restore and query
work, while keeping rejected configurations out of production.

[Turso research](turso-research.md) covers pinned source, compatibility, release
notes and current issues. Scratchpad uses embedded `turso =0.8.3` / `turso_core
0.8.3`, syscall I/O and standard WAL, not libSQL or hosted Turso. No version upgrade
was made. MVCC, io_uring, page-cache changes, prepared VM reuse, batching and
background work were evaluated in that context. SQLite-only PRAGMAs and unavailable
public BLOB APIs were excluded. Cycle 1 rejected a two-connection prototype despite
fewer instructions because native latency did not establish a reliable win.

## Three cycles

| Cycle / commit | Retained change | Comparable result |
|---|---|---|
| 1 / `9de9af0` | Size CDC scratch storage to known input; stronger FULL recovery fixture | 10k first-save allocation traffic 4.344 → 1.763 GB, −59.4%; no durable-latency win |
| 2 / `058d25e` | Memory-aware bounded pending admission, versioned policy v2, separate publication/NORMAL/FULL observations | Paired 128 MiB publication median 949.61 → 102.48 ms; all 27 candidate capacity publications under 150 ms |
| 3 / commit containing this report | Separate bounded committed-text scan cache, pressure eviction, exact-result residency probe and barrier-reporting correction | Paired 1 GiB search median 1384.49 → 297.03 ms, −78.5%; still above 180 ms |

These are adjacent-cycle comparisons, not one before/after full-suite experiment.
Paste/search use three alternating launches with three repeats per workload (n=9);
allocation uses three launches. Ranges and raw provenance are in the cycle reports
and JSON summaries. Allocation traffic is not RSS. First-use samples remain in
search results. Criterion's automatic historical-cache change percentages are not
used as paired evidence. The rejected cycle-1 connection prototype's instruction
improvement is not attributed to the selected implementation.

The full final audit does **not** establish consistent paste success: its separate
production probe takes 248.133 ms at 128 MiB, versus the 150 ms publication gate,
while the capacity sweep median is 99.886 ms. Both results stand. No baseline full
audit was rerun at `d92bb47`; focused original-binary baseline measurements and
adjacent-cycle comparisons are current evidence. The earlier one-pass/six-failure
audit is historical context, not a fresh baseline.

## Final promise health and unresolved work

`final-verified/lens/measure_run.json` reports all eight producers completed in
883.2 s. `performance_review.json` loads all seven required source artifacts and
covers all seven promises. The run-duration field `promise_evidence: false` means
lens wall time itself is not promise evidence; it does not mean the producers were
unusable. Missing optional flamegraphs and residency evidence are not substituted
with artifacts from other tool identities.

| Promise | Final verdict | Remaining pressure |
|---|---|---|
| Large Files | Fail | 1 GiB background indexing 8667.742 ms; UTF-8 load resource gate |
| Many Files | Fail | 50k first-visible 541.668 ms; lazy-open and file-resource tracking |
| Search | Fail | 1 GiB all-ranges result 312.341 ms against 180 ms |
| Many Tabs | Fail | Shared file/session gates; tab-count resource evidence uncertain |
| Many Views | Pass | No current gate miss |
| Large Text Mutation | Fail | 128 MiB production publication 248.133 ms against 150 ms |
| Session Persistence Restore | Fail | 10k first FULL persist 1450.250 ms and full restore 329.384 ms against 180 ms; store import |

The exact ten failing gate IDs are retained in `cycle3-summary.json`: three
capacity gates and resource gates for UTF-8 load, many-file lazy-open, many-file
tracking, production paste, first persistence, full restore and store import.
Tab-count tracking and aged-store forget are uncertain, not promoted to passes.
Resource controls flagged noise in some UTF-8 load, dirty-save, edited-preview,
aged-store and concurrent-read launches; the source-wide control was stable.
Noisy measurements cannot establish population-tail or cross-machine guarantees.
This was a live workstation, not a controlled idle machine. No reruns were used
to select a more favorable verdict.

Other useful foreground results: 1 GiB first-visible file window median 8.666 ms;
10k visible session restore 18.319 ms, separately from full reconstruction;
10k search-target sweep 18.869 ms. CPU layout/tessellation endpoints do not measure
display scanout. All twelve search-workflow endpoint checks pass in the final run.

## Memory, durability and recovery trade-offs

Policy v2 permits pending bulk bodies up to
`min(failure_budget, clamp(available_RAM / 32, 32 MiB, 256 MiB))`, with one chunk
of at most 256 KiB overshoot. Linux PSI pressure reduces admission to the floor.
Accepted work is retained and further bulk work waits; typing remains accepted.
This bounds bulk admission, not all live process memory or total accepted text.
The scan cache owns committed verified bodies up to available RAM / 8, capped at
2 GiB total and 1 GiB per document. Pressure evicts cache ownership; active Arc
pins can survive eviction. Viewport retention remains separately bounded.

Fast RAM publication is explicitly allowed before Turso catches up. In the final
production probes, 64/128 MiB paste publications take 129.767/248.133 ms; subsequent
NORMAL flushes take 592.385/1424.267 ms. Fresh FULL manifest barriers complete at
750.683/1706.750 ms from start. These separately observed barriers are not an
upper-bound promise for normal autosave, and cannot identify an earlier FULL
commit. The long writer-stress no-op reuses an already FULL-committed snapshot;
its original commit time remains null rather than being invented.

RAM residency, NORMAL body commit, FULL session recovery and power-loss durability
are distinct. Total accepted-not-FULL-durable and resident-FULL-durable bytes remain
unknown. Retained committed text does not itself increase accepted unsaved work;
pending admission does increase possible RAM-only exposure. Abrupt-process-exit
tests validate recovery after FULL persistence, not hardware power-loss behavior.
There is no new promised maximum autosave exposure interval.

## Lens changes and diagnostic limits

Separate lens commit `1ccd7ff`, between application cycles 1 and 2, adds exact
validation of pending policy v2 while preserving v1 endpoints and numerical gates;
120 tests passed. Separate lens fix `0f859ff`, discovered during cycle-3 validation
and committed before the final app commit, corrects false provenance invalidation
on battery charge completion with external AC still online; 121 tests passed.
Raw statuses, disconnection/discharge identity and CPU policies remain recorded.
Both changes passed formatting, check and strict Clippy. No next application cycle
started before either necessary lens change was committed.

The search-only residency ladder has 210 successful and 60 unavailable trials.
All 48 successful store-resident trials prove zero timed DB reads and positive
retained hits. Host 1 GiB resident ASCII/Unicode medians are 136.81/254.44 ms, while
warm database trials take 3280.93/3534.74 ms. These are diagnostic contracts, not
substitutes for the search capacity gate. Physical-cold proof is absent; six other
operation families remain unimplemented. The 4 GiB profile cannot retain 1 GiB
inside its 512 MiB scan budget. Profiles cap policy inputs, not actual host RAM.
The next defensible hypotheses are Unicode/search-window CPU costs, foreground
paste preparation variance, and relational first-save/full-restore work; they
were not implemented as an unrequested fourth cycle.

## Reproduction and validation

Use Scratchpad's own `nix develop`. Final command:

```sh
nix develop --command /home/laufan/Projects/scratchpad-performance-lens/target/release/splens measure promises --config target/ux-performance-20261107/final-verified-config.toml
```

Use a new output directory for any rerun. Paired driver/configs, release binaries,
binary hashes, raw JSONL, environment samples, source patches and full audit logs
are in `target/ux-performance-20261107/`. `evidence.sha256` authenticates the selected
local artifacts; these large raw artifacts are preserved locally rather than
committed. The compact summaries and reports are committed. The selected final
measurement source is parent `058d25e` plus `cycle3/final/source.patch`; the final
code commit contains that implementation and reports added after measurement.
The separate residency run retains its original lens `1ccd7ff` provenance.

Do not use `cycle1/paired` (rejected two-connection candidate) or
`cycle1/selected/paired` (overlapping test compilation) as final cycle-1 latency
evidence; use `cycle1/final-paired`. The first full audit's probe failure and the
second full audit's stale contexts remain under `final/` and `final-complete/`.
Only `final-verified/` supplies the final aggregate above.

Correctness: cycle 1 passed 731 library tests; cycle 2 passed 733; cycle 3 passed
735, plus three targeted workflow tests after the barrier regression was added
(seven existing ignored library tests each cycle). Cycle 3 also passed release
backup (4), recovery (2, one helper ignored) and session (4) integration tests,
all-target strict Clippy, formatting, check and Turso lock consistency. Logs retain
initial failures and subsequent successful checks; no test or budget was removed
to manufacture a pass.
