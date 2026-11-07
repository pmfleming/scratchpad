# Cycle 2 — bounded foreground headroom

Cycle 1 left paste waiting behind the 32 MiB body queue. The hypothesis here is
that background preparation can publish sooner if available system memory absorbs
more accepted text while Turso catches up. Relational storage and the serialized
writer remain. No storage mode, schema, FULL commit or numerical gate changes.

Policy v2 uses `min(failure budget, clamp(available / 32, 32 MiB, 256 MiB))`.
Linux memory PSI `some avg10 >= 1%` reduces it to the 32 MiB floor, still capped
by the failure budget. Producer threads refresh at most once per second; typing
and painting never sample system memory. A threshold decrease retains accepted
text and pauses further bulk work. One <=256 KiB chunk can overshoot the threshold.
Typing remains accepted; neither this queue nor the existing failure budget is a
hard bound on all live application memory. On platforms without PSI, available
memory and the absolute cap still apply. Diagnostic memory profiles limit the
budget input, not actual host RAM or OS page cache.

The lens change `1ccd7ff` preceded this cycle and freezes the v2 formula separately
from the unchanged v1 outcome endpoints, numerical gates and workload inventory.
It passed all 120 tests, formatting, all-target check and strict Clippy. This was
required policy integration, not a repair that manufactured a pass.

The production paste probe now records pending bytes at publication, observed
budget inputs, NORMAL flush time and a separately asserted FULL manifest barrier.
`publication_ns` and `workflow_ns` retain their previous endpoints. An explicit
FULL persist occurs **after** workflow_ns; outer process elapsed/allocation totals
therefore include extra diagnostic work and are not directly comparable to v1.
Only the unchanged publication gate and same-endpoint phases establish speedups.
The measured exposure interval belongs to this explicit probe barrier; normal
async autosave still has no promised maximum exposure interval. Total accepted
uncommitted and resident FULL-durable bytes remain unknown, not zero.

## Measurement and validation

Release binaries from cycle 1 and this cycle ran in alternating order, three
independent launches each, no concurrent builds. Capacity has three repeats per
workload per launch (n=9). Raw JSONL, SHA-256, host pressure, source patch and
order: `target/ux-performance-20261107/cycle2/{paired,capacity-paired}`.

| Paste size | Before median [range], ms | After median [range], ms |
|---|---:|---:|
| 8 MiB | 17.55 [15.76, 19.95] | 17.43 [13.00, 18.94] |
| 64 MiB | 141.39 [124.44, 595.34] | 67.69 [42.42, 73.21] |
| 128 MiB | 949.61 [317.20, 1789.16] | 102.48 [75.08, 133.33] |

All 27 candidate capacity publications stayed below the unchanged 150 ms gate;
this is a focused gate result, not a complete Text Mutation promise pass.
The production resource sweep independently measured publication medians of
59.7 and 120.2 ms at 64/128 MiB, with median maximum frame times 5.41/5.27 ms.
Pending peaks were 61/125 MiB, bounded below the observed 256 MiB threshold.
Median post-publication NORMAL flush took 1252/818 ms; median absolute FULL
barrier timestamps were 1359/952 ms from operation start. Do not subtract
independent medians to infer per-trial exposure; raw timestamps preserve each
interval. Foreground success comes with roughly a second of later DB work here.

10k first-save median was 1898 ms vs 1185 ms, with overlapping wide ranges
[1157,2374] vs [1164,2398]. Steady-save medians were 89.9 vs 92.0 ms. No durable
save speedup is claimed; large-search and restore targets remain unresolved.

733 library tests passed (7 ignored), focused admission/cache tests passed after
the queue-lock adjustment, recovery 2 passed (1 helper ignored), strict Clippy
and formatting passed. The new tests cover pressure and 4/16 GiB budget inputs,
failure caps, cancellation after lowering a limit without losing accepted text,
and monotonic publication/NORMAL/FULL diagnostic endpoints. Frozen lens gate
and outcome inventory tests pass against policy v2.
