# Performance cycle 25: final audit and stopping point

The user requested completion of cycle 25 followed by a report covering all 25 paired Scratchpad/performance-lens cycles. No cycle 26 was started. The all-promises performance goal remains unmet and is paused at this requested stopping point.

The full [25-cycle report](../../scratchpad-performance-lens/docs/report-cycle25/report.md), [interactive graphs](../../scratchpad-performance-lens/docs/report-cycle25/graphs.html), [standalone graph export](../../scratchpad-performance-lens/docs/report-cycle25/trends.svg), and [complete audit tables](../../scratchpad-performance-lens/docs/report-cycle25/audit-tables.md) are committed in the companion lens repository. The report includes the complete cycle/commit ledger, discarded experiments, measurement-method corrections, remaining lens issues, and hypotheses that were not executed. Extracted data, source hashes and generators are committed alongside it.

The final full audit measured clean Scratchpad implementation commit `b964a137af24119adaefac693fc7faaa90013763` with lens implementation `5aa2d08`. Reporting commits follow those revisions and do not change measured application behavior.

All eight producers completed in **806.440 seconds**. The review reports **six failing promises, Many Views at risk, no passing promises, 11 reported budget misses, and six unmet scale checks**. Six promises have covered evidence; session persistence/restore is marked thin because aged-store forget is uncertain. Coverage does not mean success. The count of misses is a historical review aggregation, not a count of unique raw gates.

| Final observation | Budget | Result |
|---|---:|---:|
| First rendered search result, 10,000 targets | 120 ms | 26.813 ms median; all 12 workflow medians pass |
| Retained history, 100,000 edits | 180 ms | 93.560 ms |
| Retained production search results, 1,000 targets | 180 ms | 16.245 ms |
| Production search completion, 1 GiB | 180 ms | 1,445.451 ms median; fails |
| Paste publication, 128 MiB, resource probe | 150 ms | 1,404.355 ms; fails |
| First save, 10,000 tabs | 180 ms | 1,280.092–1,322.585 ms across three launches; fails |
| Full retained restore, 10,000 tabs | 180 ms | 284.833–311.192 ms across three launches; fails |
| Unchanged save, 10,000 tabs | 180 ms | 97.277–101.827 ms |
| Visible startup restore, 10,000 tabs | 180 ms | 17.557–19.024 ms |
| Cold-read observed p99 during saves | 4 ms | 3.679 / 4.068 / 4.114 ms; fails |
| Aged-store forget | 180 ms | 100.031–215.650 ms; uncertain |

The session group includes a noisy launch; passing observations do not establish a clean baseline. Relevant resource/capacity controls also remain noisy. Stable CPU controls cannot certify storage isolation. Headless rendering excludes GPU/display present. The report records final lens inconsistencies, including an empty top-level noise list despite noisy scenario evidence and wording that conflates uncertainty with missing measurements.

Cycle 25 reused bounded fragment-search workers and changed capacity measurement to the production matcher, with exact input/result/endpoint validation in the lens. The corrected 1 GiB baseline was 1,410.018 ms before reuse and 1,363.165 ms in the focused final run; the full audit was 1,445.451 ms. This is a modest targeted improvement, not proof of meeting 180 ms, and earlier serial-probe results use a different method.

Validation before the full audit: 726 library tests passed (7 ignored), strict Clippy passed, and release capacity/resource/workflow probes built. The paired lens passed 109 tests and strict Clippy. Final report changes are documentation and reporting artifacts only.

Local raw evidence remains at `target/analysis/co-development/promises-after-cycle-25/`; focused cycle evidence and rejected trials remain under `target/analysis/co-development/` and `target/co-development/`. The companion lens's committed extraction keeps report data usable after ignored target files are cleaned. No budgets, durability guarantees or cache/pending-byte bounds were relaxed. No commits were pushed.
