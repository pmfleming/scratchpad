# Stage 7: live text completion and validation

The live editor has no temporary text backing. Schema 13 adds verified final-LF
metadata so history can rebuild whole pieces without reading their bodies.
History replay first compares logical references; partial-boundary reads remain
bounded and fallible. A streaming comparison handles explicitly imported recovery
payloads whose logical addresses differ. ASCII range capture uses direct byte offsets, avoiding a repeated scan in tiny
near-end history edits. Range lookup begins at the indexed leaf,
and retained backing intervals are merged before reachability checks.

Large replacements retain deleted spans and bounded labels rather than copying
the deleted payload. Imported replacements append shared chunk aliases to stable
add addresses and keep previous versions reachable. Aliases share pending state
and one accepted body; owner-specific caching and read faults remain separate.
Pending collection keeps bodies while any alias still owns them.

Large paste, selection delete, clipboard extraction, indentation, undo/redo and
History navigation prepare on the background lane. Results publish atomically
against a captured document/history token; additional accepted input stays ordered.
Clipboard extraction owns the one required clipboard string. Admission is atomic
across concurrent producers, including an operation that encounters a failure
midway. Failure refuses an unpublished candidate without changing the live text.
New raw bulk events are checked before entering deferred input. Save As and file
save wait for accepted input; Forget does not invalidate a running preparation.

UI persistence requests and tab/final-view closes use the session background lane.
A close records the same final state and activity before removing a view. Newer
edits or changed layouts cancel removal; a second close supersedes the earlier
archive safely. Unavailable closes retain accepted RAM state. Cancelled previews
cancel their import without manufacturing lifecycle activity.

Restore transports typed metadata and references, with no empty whole-text map.
Every committed document requires a text manifest, even when clean and disk-backed.
Production restore never reloads disk as a missing-manifest fallback. Newly opened
batch files are adopted before cold-shell publication and carry their references
until committed. Logical snapshot identities survive anchor-only COW but change
on edits, including divergent clones. This prevents cursor movement from making
commit acknowledgements or close intents stale.

Reset retains hydrated cold references across a failed backup and writer retirement,
so retry can preserve cold text without calling the stopped writer. Reset backs up
all known sidecars, including a pre-existing experimental `-tshm` file, without
enabling that WAL mode. An intentional reader closure interrupts a scan without
permanently faulting its document; normal reader failures still fault it. Import cancellation
is checked throughout streaming; incomplete UTF-8 and cancelled candidates never
publish partial documents. Large non-UTF-8 decoding still stages in RAM as scoped,
then moves its decoded body into the store. Small owned originals remain supported.

## Linux evidence

Commands, raw measurements and compact comparisons are retained in `evidence/stage-7/`.
Resource timings are local observations. Stage 0's shared-host baseline was
provisional; ratios do not establish statistical significance. Headless UI timings
include production input, logic, layout and tessellation, excluding GPU presentation
and native OS input delivery. Application cache misses use a warm OS page cache.

The sustained-I/O workload seeds 3 GiB of non-repeating ASCII, then writes another
3 GiB with a different generator seed while pasting 64 MiB, scrolling and preserving
queued typing. A 200 ms writer pause is injected as a separate stressor. The initial
exploratory run reused the seed and exercised deduplication; it is excluded from the
sustained-write acceptance result. The corrected run confirms database growth past
6 GiB. Batch timing includes actual hashing/commit/checkpoint duration; 50 ms remains
a scheduling target, with indivisible I/O overruns retained. No other budget changed.

Per-process I/O is OS-accounted write volume, including WAL/checkpoint activity;
it is not physical device traffic. RSS high-water is process-lifetime. Cache and
tail figures are endpoint measurements; pending has an explicit high-water counter.
Pinned viewport bodies can overlap cache and are not separately summed. The OS page
cache cannot be attributed to this process and is not reported as private RSS.

Native Windows runtime validation remains pending by the user's explicit direction.
The strict RustQualityLens gate is separate from compilation/tests. Its outstanding
partial evidence, required human reviews and threshold findings remain visible;
no policy or performance threshold was raised to make a gate pass.

Validation passed: 700 library tests with seven existing ignores, the complete
all-feature integration/documentation suite, and all-target/all-feature Clippy with
warnings denied. The separately invoked 10,000-tab metadata measurement passed:
83 ms steady save, 113 ms shell restore, 2 ms ID reservation and 455 ms first-tab
close (stage 2: 87/121/4/715 ms). Its first save is 1,749 ms because the new text
manifests are included; this is distinct from the typed-only stage 2 fixture.

The 43-case resource sweep reports no failed scenarios. Every matched duration is
below its recorded stage 0 duration; additional 512 MiB–2 GiB cases have no inferred
baseline. At 10,000 nonempty tabs, save/restore are approximately 4.8 s/0.85 s versus
25.9 s/8.7 s in the provisional baseline. First paint remains about 2.8–3.2 ms. The
100,000 tiny-history case initially exposed a 9.5 s ASCII offset scan; its direct
byte-offset fix reduces it to 335 ms (recorded baseline 696 ms). No limit changed.
Criterion promise/frame/search benchmarks completed; their compact comparison uses
the committed stage 0 estimates, not Criterion's previous local cached sample.

The standalone standard-WAL contract passed transactions, concurrent readers,
Busy, checkpoints, injected panic containment, disk-full preservation and abrupt
exit/reopen recovery. Its read-hop p99 during commits is 1.46 ms. Crash probes are
process-exit tests and must not be described as power-loss tests. Native Windows
runtime tests remain pending.

The final sustained-write sample records 19 measured body-batch overruns above
50 ms (maximum 89.8 ms); seeding adds 70 overruns, maximum 127.4 ms. Across 7,453
production headless frames, p99 is 0.591 ms and maximum 11.67 ms; queued typing
survives. The 3 GiB seed plus a distinct 3 GiB producer grows the database to
6,523,023,360 bytes. Pending peaks at 33,554,436 bytes (32 MiB + four bytes), below
32 MiB + one 256 KiB chunk. Endpoint cache is 8 MiB, tail seven bytes; configured
text-cache limits are 16 MiB aggregate / 4 MiB per owner. The writer has an 8 MiB
engine page-cache budget and each reader 512 KiB; actual occupancy is not exposed.
Process RSS high-water is 215,605,248 bytes. Process-accounted writes are
29,117,722,624 bytes over seeding and the measured workload. WAL peaks at a sampled
27,488,672 bytes (5 ms polling) and ends at 2,509,112 bytes before shutdown; this
sampled peak is not a continuously observed device-level maximum.

Final 64 MiB / 2 GiB streaming imports take 175 ms / 5.61 s. Verified random-read
p99 is 0.598 / 0.372 ms. One-edit manifests hash zero new sealed-body bytes; their
sample times are 4.49 / 56.9 ms. The latter FULL commit is retained as an I/O
outlier, not removed or used to relax a target. UI persistence is asynchronous.
Repeated exploratory samples and the final sampled run are kept separately.

Earlier repeated seeding samples reached 700 ms; the final run does not establish
a worst-case commit bound. All overruns remain in the retained artifacts.

Final review follow-up: small editor actions pin their complete bounded read range;
indentation includes the line prefix and outdent lookahead. A long line prefix or
more than 256 line edits prepares off-thread. Undo/redo also prepares when an entry
contains more than 256 logical spans, even when its byte payload is tiny. Added
cases exercise Tab deep in an 80 KiB line, indentation across 300 short lines, and
undo of hundreds of tiny replacements. The final suite passes 702 library tests,
seven existing ignores, all integration/doctests and warnings-denied Clippy.

The first frozen stage 7 quality run (commit b58faee) passed measurement,
verification, audit/feature tools and source-hash stability. Its strict gate failed
with 14 partial-evidence artifacts, three unmet human reviews and 37 threshold
findings; no actual test failure or reliability finding was reported. See
quality-before-indentation.json. The follow-up receives a new frozen-source run;
no rule or threshold is relaxed.

The final frozen run covers fe9ada9 with identical before/after source hashes and
an unchanged quality tool. Measurement and verification pass. Strict policy still
fails: 14 partial-evidence artifacts and three required human reviews remain;
36 module threshold findings are also retained. No actual test failure or
reliability finding is reported. The complete compact result is
`evidence/stage-7/quality-summary.json`. This is not a passing release gate.

Standalone `cargo hack check --locked --all-targets --feature-powerset` passed all
four feature combinations. Standalone `cargo audit --json` exited zero with no
vulnerability entries, plus four unmaintained-package warnings, one unsoundness
warning and three yanked versions. The unsoundness warning is event-listener 5.4.1,
RUSTSEC-2026-0221 (non-Send StackSlot tags). The dependency lock is unchanged from
stage 6. These warnings were not suppressed; see audit.json and audit-summary.json.
Tool-version discovery in the frozen wrapper is distinct from these actual scans.

Implementation and Linux measurements are committed. Native Windows runtime,
independent human reviews, partial quality evidence, reported thresholds and the
dependency warnings remain release acceptance items. No policy, engine pin or
performance target was changed to hide a finding.
