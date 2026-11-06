# Next five quality improvements

Baseline code: `b7f8acf`, measured by `refactor-complete`. Final measured code:
`1a5cd76`, measured by `next-five-final`. The same local RustQualityLens binary and
unchanged policy configuration were used. Source identities remained stable during
both runs. [Raw aggregate measurements](next-five-measurements.json) record the
source identities, binary BLAKE3, scopes, function coverage and gate results.

## Completed work and commits

| Improvement | Commit | Change and validation |
| --- | --- | --- |
| Checked layout decoding | `72173d5` | Four read queries become three; documents decode directly into their owners, removing the intermediate map and record clones. Shared scalar readers retain checked integer/boolean conversions. Nullable cursors require all four fields or none. All 94 database tests pass, including all 16 nullable cursor shapes and invalid values. |
| Editor input preflight | `946eb41` | Up to three event scans become one. Read requirements are collected under the input lock; storage pinning happens after releasing it. The existing test now covers bounded input, large selections/indentation, preparation and faults. All 83 editor tests pass. |
| Search command dispatch | `bb1a4af` | Shared focus handling and a common success return remove duplicate dispatch code. Existing search/replacement tests drive commands instead of bypassing them, covering settings, navigation, invalid indexes and focus effects. All 14 search-state tests pass; no test entry point was added. |
| Live History cache | `4eb5b5d` | Unchanged UTF-8 paths stay borrowed; repeated cached-ID clones and the cloned-ID hash set are removed. A subset/cardinality invariant skips retirement scanning when every cached document is still loaded. Retired keys move instead of cloning. All 28 History tests pass, including coalescing, renames, filtering, retirement/re-entry and history clearing. |
| Independent atomic staging | `7777e3e` | Each writer owns a unique temporary file in the target directory. Automatic cleanup replaces repeated failure cleanup, and three obsolete helpers disappear. Two scenarios cover replacement/write/creation errors and overlapping writers. |

Final review found that committed IME text used the insertion path while preflight
excluded it. Follow-up `1a5cd76` applies the same bounded-read rule to IME commits;
preedit stays separate. The editor matrix covers both and all 83 tests pass again.

The atomic regression **failed against the old implementation** with `NotFound`:
nested writes overlapped the same staging file. It passes with unique stages,
verifies complete intermediate/final contents, and leaves an unrelated old-style
`.write` file untouched. The overlap is deterministic and has no scheduler timing
assumption. Error cases preserve the old target and leave no owned staging files.

## Measured results

| Measure | Before | After |
| --- | ---: | ---: |
| Selected-module cognitive complexity | 174 | 154 |
| Selected-module cyclomatic complexity | 399 | 378 |
| Selected-module halstead effort | 1,729,530 | 1,363,391 |
| All-function cognitive complexity | 7,918 | 7,911 |
| All-function cyclomatic complexity | 13,477 | 13,468 |
| All-function Halstead effort | 54,825,538 | 54,797,925 |
| Source nonblank lines | 100,260 | 100,566 |
| Duplicated lines | 7,075 | 7,029 |
| Test catalog entries | 777 | 776 |
| Test-function SLOC | 18,407 | 18,671 |
| High-CRAP functions | 64 | 61 |
| Line coverage | 48,297/64,250 (75.17%) | 48,532/64,291 (75.49%) |
| Function coverage | 5,404/6,996 (77.24%) | 5,430/6,997 (77.6%) |
| Region coverage | 68,458/91,475 (74.84%) | 68,665/91,385 (75.14%) |

Selected-module rows include **all** function records in the same eight module
paths, including the new `database/rows.rs` and inline test functions. The scopes
are listed in the JSON; module records are not added to function totals. This
includes extracted helpers rather than only showing smaller entry points.

| Module group | Cognitive | Cyclomatic | Halstead effort |
| --- | ---: | ---: | ---: |
| layout_decoding | 32 → 27 | 168 → 146 | 699,948 → 514,635 |
| editor_preflight | 88 → 86 | 167 → 171 | 741,207 → 625,022 |
| search_dispatch | 3 → 2 | 22 → 20 | 22,074 → 21,458 |
| history_cache | 45 → 29 | 22 → 24 | 172,429 → 142,231 |
| atomic_writes | 6 → 10 | 20 → 17 | 93,872 → 60,046 |

Search dispatch coverage rises from **0% to 100%**, with CRAP **342 → 18**. Atomic
writing rises from **58.70% to 100%**. The new editor event collector and pin policy,
layout-view decoder, and History per-buffer updater all have 100% measured line
coverage. Layout reading's cyclomatic score falls from 54 to 19; its additional
helpers are included in the layout group above. Editor rendering falls from 25
to 15, but aggregate editor cyclomatic complexity rises by four after adding the
explicit policy and IME handling. History also adds two cyclomatic points while
reducing nesting, effort and idle cache work. The atomic group's cognitive increase
includes its expanded inline regression scenarios.

**Code size did not improve.** Nonblank source lines increase by 306, and test
function SLOC increases by 264, despite one fewer test entry point. Checked decoding and broader assertions account for most growth; no claim is made
that every requested metric improved. Coverage adds 235 covered executable lines
while its denominator grows by 41. Branch coverage is unavailable. No performance
benchmark was run, so the query/scan/allocation changes are structural findings,
not measured wall-time speedups.

## Validation and limits

The final all-feature root test suite, integration tests and doctests pass. The
warnings-denied, all-target/all-feature Clippy run also passes. Targeted logs are
under `target/quality-next-*`; full accepted reports are under
`target/quality-runs/refactor-complete` and `target/quality-runs/next-five-final`.
No dependency, quality threshold, waiver or required-review acceptance changed.

The strict gate remains **failed**: 14 partial artifacts, 3 required human reviews, and 32 threshold findings (previously 33). Test, practice, selected reliability and operational failure counts are all zero. Native Windows runtime validation remains pending under the existing authorization.

Graph means are reported without claiming a measured leverage improvement: leverage 10.5952 → 10.5347, locality 96.8318 → 96.8767. Both remain partial; unresolved dependency references are recorded in the JSON. Unsafe/FFI and escape-hatch counts are unchanged. The test catalog is an inventory, not a claim that platform-specific or standalone tests all executed.
