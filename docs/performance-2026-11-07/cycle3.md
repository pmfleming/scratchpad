# Cycle 3 — bounded committed-text residency

The application now retains verified immutable bodies in a separate scan cache:
available RAM / 8, capped at 2 GiB aggregate and 1 GiB per document. The viewport
cache retains its own independent budget. A just-committed body can enter the scan
cache without another database read; subsequent scans retain verified loads there.
This changes residency, not NORMAL/FULL durability or pending admission policy v2.
The actor samples memory at most once per second, including idle periods; Linux
PSI pressure evicts scan ownership and lowers new pending admission. Returned Arcs
remain valid through eviction. Those short-lived pins can outlive cache ownership.

Direct stores default to no scan retention; the actual application enables the
memory policy when applying its history budget. The search-capacity fixture now
explicitly enables that same app policy and records its name. It keeps every
original workload, repeat, production matcher, exact-result check and threshold.
The declared endpoint remains `all_ranges_returned`, with uncontrolled store
residency. No gate is converted into a resident-only gate.

The new `residency_probe` implements the existing search-file-size diagnostic
contract for both ASCII and Unicode/CRLF. SHA-256 identifies the actual input and
exact character ranges. It uses the production matcher/cancellation generation,
records store-read deltas and retained hits, and excludes setup/warm-up from the
native timer. Owned-contiguous is a primitive reference; owned-piece-tree uses the
production worker policy. Store-resident requires zero timed store reads and
positive retained hits; insufficient memory or pressure produces unavailable.
Store-warm explicitly primes a scan with both app caches disabled and proves
subsequent store reads. Cold search reports unavailable because this probe does
not yet establish physical page-cache eviction/read proof. No cold timing is
invented. The six other ladder operations remain unimplemented and are not run
by this search-only selection. Diagnostics never establish promise passes.

The 4/16 GiB profiles only cap memory-policy input; they are not VM/cgroup memory
limits, and host OS-cache capacity stays real. A 1 GiB document cannot fit the
512 MiB scan budget under the 4 GiB profile. The probe records that fact rather
than quietly increasing the budget. Normal app retention is evictable and does
not change crash exposure. A NORMAL-committed cached body alone is not evidence
of a FULL-recoverable session manifest.

New correctness coverage checks independent viewport/scan eviction, pinned-Arc
survival, zero store reads on a retained repeat, fresh store reads after eviction,
exact Unicode/CRLF results across diagnostic rungs, and pre-start cancellation.
The probe initially exposed an invalid small test fixture: files below 16 MiB
use the owned path. The test now uses the real streaming threshold; zero reads
without retained hits was correctly rejected, not labelled a resident success.

## Audit-discovered diagnostic correction

The first full audit is preserved in `target/ux-performance-20261107/final/`
and `final-audit.log`. Seven producers completed; resources correctly reported
partial coverage because the new explicit-barrier assertion failed during the
long writer-stress case. Autosave had already committed the identical snapshot:
`persist_profiled` returned a successful no-op, not a failed durable write.

Cycle 3 corrects that cycle-2 instrumentation assumption. A fresh transaction
records its FULL completion timestamp. An unchanged successful barrier reports
`full_barrier_previous_snapshot_reused`, records barrier completion separately,
and leaves the original FULL commit timestamp/exposure unknown. A regression
test performs two real persists and checks that the second cannot invent a
commit time. Neither publication nor existing gate timers change. The lens
correctly rejected the failed producer; no lens fix or weakened check was needed.
The corrected final audit uses a new output directory; old evidence is retained.

## Paired search and residency measurements

Cycle-2 and cycle-3 release binaries ran in alternating order, three independent
launches each with three repeats per size (n=9). First-use samples remain in the
distribution. The source patch, executable hashes and raw trials are archived in
`target/ux-performance-20261107/cycle3/paired/`. This comparison predates only the
barrier-reporting correction above; its scan-cache implementation is the final one.

| Search size | Before median [range], ms | After median [range], ms |
|---|---:|---:|
| 1 MiB | 0.602 [0.334, 1.323] | 0.748 [0.372, 1.422] |
| 64 MiB | 83.615 [77.072, 126.384] | 19.687 [18.487, 68.351] |
| 256 MiB | 336.687 [316.988, 477.679] | 77.967 [71.682, 270.981] |
| 1 GiB | 1384.492 [1321.343, 1971.141] | 297.026 [269.184, 1059.172] |

The 1 GiB median falls 78.5%, but still misses the unchanged 180 ms gate. Retention
helps repeats; first-use CPU/setup costs remain visible. This is not evidence that
every 1 GiB search is interactive or that a cold database read became faster.

The separate diagnostic ladder completed 270 requests across 18 groups in 827.6 s:
210 successful and 60 unavailable (54 physical-cold requests and six 1 GiB
resident requests under the 4 GiB policy profile). All 48 successful resident
trials had zero timed store reads and positive retained hits. All 54 warm trials
performed store reads. SHA-256 result identities and exact counts matched.
Host-profile medians below are milliseconds, n=3 per cell:

| Size / fixture | Contiguous reference | Owned piece tree | Store resident | Store warm |
|---|---:|---:|---:|---:|
| 64 MiB ASCII | 3.07 | 6.66 | 13.35 | 211.21 |
| 64 MiB Unicode/CRLF | 3.36 | 69.79 | 20.38 | 220.75 |
| 256 MiB ASCII | 10.02 | 25.70 | 52.60 | 797.39 |
| 256 MiB Unicode/CRLF | 12.18 | 270.49 | 75.45 | 902.64 |
| 1 GiB ASCII | 38.61 | 100.64 | 136.81 | 3280.93 |
| 1 GiB Unicode/CRLF | 48.01 | 1046.72 | 254.44 | 3534.74 |

These are distinct diagnostic contracts, not interchangeable implementations of
the capacity gate. The ladder is incomplete: 18 groups lack physical-cold proof;
the six other operation families were not implemented. The original diagnostic
artifact uses lens `1ccd7ff`; it remains at `residency/lens/` with its original
provenance and is not restamped or imported into the later full audit. A useful
next hypothesis is Unicode/search-window/character-offset CPU work: resident
search still takes 254 ms without reading the store, versus the 48 ms primitive.

## Lens repair and final complete audit

The second full audit, `final-complete/lens/`, completed all producers but correctly
refused to combine contexts that its then-current identity considered different.
The only context difference was BAT0 status changing from Charging to Not charging
while external AC remained online. This was an overly strict lens identity rule.
Separate lens commit `0f859ff` normalizes charging completion only with positively
identified online external power; discharge, unknown status, unplugging, CPU policy
and other environment differences remain significant. Raw power status remains in
transient conditions. Identity revision 2 invalidates old contexts. All 121 lens
tests, formatting, all-target check and strict Clippy passed. No gate changed.
See the lens's `docs/power-identity-review.md` for the reproduction and source.

A fresh run using that lens completed all eight producers in 883.2 s at
`final-verified/lens/`. All seven promises are covered; all seven required source
artifacts load with valid provenance. Optional flamegraphs are absent. Result:
**Many Views passes; Large Files, Many Files, Search, Many Tabs, Large Text
Mutation and Session Persistence Restore fail.** Ten unique numerical gates fail.
Some resource launches have noisy controls; aged forget and tab-count resource
evidence straddle their budgets. These limitations are retained, not rerun away.

The fresh capacity sweep reports 128 MiB paste publication median 99.886 ms and
1 GiB search 312.341 ms. The distinct production paste probe publishes 64/128 MiB
in 129.767/248.133 ms: the latter fails the 150 ms gate. Corresponding post-publication
NORMAL flushes take 592.385/1424.267 ms, and fresh FULL barriers complete at
750.683/1706.750 ms from start. The same-trial publication-to-barrier intervals
are 620.916/1458.617 ms, upper bounds to the explicit observed barrier rather than
guaranteed first-durable times or normal-autosave exposure limits. Maximum frame
times are 5.787/5.261 ms. The long writer-stress run successfully reuses an existing
FULL snapshot and leaves its original commit time unknown, as intended.

10k first FULL persistence is 1450.250 ms, complete restore 329.384 ms, steady
persistence 163.038 ms and visible-only restore 18.319 ms (medians, n=3). The first
two fail 180 ms; foreground visibility and full reconstruction remain separate.
See `results.md` and `cycle3-summary.json` for the remaining gates and provenance.

## Correctness validation

735 library tests passed (7 ignored), followed by the three targeted workflow
tests including the newly added real no-op barrier regression. Release integration
tests passed: backup 4, recovery 2 (1 helper ignored), session 4. All-target strict
Clippy, formatting, check and the Turso lock consistency check passed after fixes.
An initial small residency fixture and a Clippy style error are retained in their
original logs; the final suite/build supersede them. Process-exit recovery tests
are not power-loss/hardware-fsync proof. No storage engine or durability mode was
changed in this cycle.
