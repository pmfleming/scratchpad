# Cycle 1 — size-aware capture, 2026-11-07

Hypothesis: first-save content capture needlessly allocates and zero-fills a
256 KiB streaming scratch buffer for every small document. Known-length memory
inputs can use min(input length, 256 KiB), preserving CDC boundaries and the
streaming reader path. Adopt/capture now use that bounded constructor. Empty,
Unicode and boundary-length equivalence tests cover the change. The abrupt-exit
FULL recovery test now includes a multi-megabyte NUL/Unicode/CRLF body.

A second hypothesis used separate, permanently FULL/NORMAL connections on one
actor to avoid Turso re-prepares. This prototype passed recovery and mode tests
and reduced whole-process Callgrind instructions from 23,525,218,200 to
16,070,224,569 (including size-aware capture). Native first-save and paste times
had large stalls. Removing the second connection did not eliminate those stalls.
Nevertheless the connection topology change is **not retained** without convincing
latency evidence; `cycle1/two-connections.patch` preserves it for investigation.
Instruction figures belong to that rejected combined prototype, not the final
size-aware change. Valgrind reported its brk fallback warning in both runs;
instrumented wall time is not gate evidence.

Evidence root: `target/ux-performance-20261107/`. Baseline executables are from
`d92bb472958f32821a60e02629e8bcb633a35c64`; SHA-256, raw rows, stderr, host pressure
and interleaved order are preserved. The initial `cycle1/paired` comparison is
for the two-connection prototype. `cycle1/selected/paired` was contaminated by
correctness compilation and is explicitly exploratory, not the final timing
comparison. The final comparison and summary are recorded below.

The change does not alter foreground publication or durability policy. NORMAL
body commits, FULL manifest commits, 32 MiB pending backpressure, and process-exit
recovery retain their previous meanings. No promise is declared fixed from
allocation savings alone.

## Selected result

Three independent launches per arm, alternating order, release build after all
correctness work finished. Native median [min, max] milliseconds:

| Operation / size | Before | After |
|---|---:|---:|
| production_paste / 67108864 | 612.76 [323.65, 1085.96] | 1173.49 [698.33, 1590.51] |
| production_paste / 134217728 | 678.97 [672.74, 737.24] | 1015.04 [759.75, 1135.56] |
| session_persist_cost / 10000 | 2042.26 [1187.59, 2383.90] | 2070.42 [1877.40, 2295.30] |
| session_restore_cost / 10000 | 264.47 [260.05, 269.83] | 263.95 [262.24, 265.81] |
| session_steady_persist_cost / 10000 | 88.76 [87.96, 89.44] | 91.13 [90.11, 95.60] |
| startup_visible_restore_cost / 10000 | 15.90 [15.67, 16.21] | 15.78 [15.45, 16.18] |

10k-document first-save allocation volume: **4,344,251,508 → 1,763,481,604
bytes (−59.4%)**. This is allocation traffic, not retained RSS. First-save median
is effectively unchanged within wide disk variation. Paste publication medians
were worse (64 MiB 313 → 531 ms; 128 MiB 324 → 578 ms); keep these regressions
visible, do not declare a UX gain. The large input capture path is unchanged in
this selected implementation. A later paired cycle will reassess it.

731 library tests passed, 7 ignored; abrupt-exit/healthy-checkpoint recovery
2 passed, 1 helper ignored; formatting, strict Clippy for lib/bins/tests and Turso
lock identity checks passed. Final build log, source patch, binary hashes and
raw runs are in `cycle1/final-paired`. No full promise audit was run in this cycle;
the historical 1/7 pass result remains historical. Known first-save, restore,
paste and large-search target gaps remain unresolved.
