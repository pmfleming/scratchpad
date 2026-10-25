# Transactional session storage migration

## Evidence and acceptance

Baseline: `5771513`, including the user's pre-existing working-tree editor test
change (untouched). Raw evidence lives in `target/turso-migration/baseline`.
Run `bash scripts/check-session-migration.sh PHASE` without editing inputs during
the run. It runs the local Rust Quality Lens hotspots, clones, locality, leverage
and reliability producers, then the full Scratchpad Performance Lens. Producer
success is NOT equivalent to every promise passing. No budgets are relaxed.

User changes excluded from all migration commits:
`docs/hyprland-dwindle-shortcut-delta.html`,
`src/app/ui/editor_content/native_editor/tests.rs`,
`tools/performance-lens/plan.html`.

### Phase 1 — engine contract and baseline

Pinned API: `turso = 0.8.0`, default features disabled, transitive versions locked.
`tools/turso-contract` is an isolated, repeatable executable, never pointed at
user data. Run `cargo run --locked --manifest-path tools/turso-contract/Cargo.toml`.
Linux validation passed: FULL synchronous commits, rollback, 16 MiB BLOB round
trip, subprocess exit without destructors before/after commit, WAL recovery,
explicit checkpoint and integrity check. An executor from futures-lite suffices;
no Tokio runtime or cloud credentials are needed. Checkpoint PRAGMAs return rows:
they must be queried/drained, not sent to `execute`.

Full baseline performance lens completed (exit 0). At 10,000 tabs:

| Operation | Criterion mean |
| --- | ---: |
| Steady-state session persist | 69.850 ms |
| Startup-visible restore | 16.162 ms |
| Full restore | 121.966 ms |

Initial 10,000-tab resource persistence is about 4.93 seconds. Fresh baseline
promise health: Large Files at risk; Many Files fail; Search fail; Many Tabs pass;
Many Views at risk; Large Text Mutation at risk; Session Persistence Restore at
risk (no session budget misses). Thus the README's historical seven-pass claim
is not a current acceptance result. Final comparison must preserve this fact.

Quality baseline: 4,626 hotspot records, 7.33% detected duplication (4,931 lines).
Semantic locality/leverage evidence includes syntax fallback; it is heuristic,
not a verified architecture gate. Phase 1 deliberately changes no application
code: production complexity, effort, cloning and LOC are unchanged. Subsequent
phases should remove the multi-file persistence machinery rather than introduce
unrelated metric-only rewrites.

Limitations: Windows execution and real power-loss testing are not available in
this Linux run. Abrupt process termination is not a power-loss simulator. The
engine's pre-1.0 status requires independent recovery exports/backups. Baseline
build preparation overlapped the isolated probe build; final timing comparisons
must be interpreted with normal host variance, not claimed as causal proof.

### Phase 2 — transactional worker and bounded content

Implemented behind test-only module wiring until the importer is ready: a bounded
single-owner database worker, cross-process ownership lock, restrictive Unix file
permissions, schema/application identity checks, FULL synchronous transactions,
256 KiB content chunks with BLAKE3 verification, per-tab incremental metadata,
revision-based content skipping, and stale-capture rejection. Cache updates occur
only after successful commit. Rollback keeps the previous complete session.

Validation: 490 library tests pass, including six new database contracts; strict
library/test Clippy passes. Raw lens evidence: `phase2-final`. Both local lenses
completed. Legacy snapshot-write orchestration was simplified: function cognitive
sum 22 -> 13, cyclomatic sum 47 -> 34, Halstead effort sum 79,747 -> 69,099, and
function SLOC 234 -> 202 in `session_store/ops.rs`. The new backend adds code until
legacy machinery is removed; this is not a claim of total-project LOC reduction.
Leverage for that module rose 10 -> 40, but locality fell 100 -> 91 with changed
semantic identity resolution; those heuristic outputs do not prove an improvement
in every architectural dimension. No quality thresholds were weakened.

Full performance lens: steady persist 68.043 ms, startup-visible 14.198 ms, full
restore 117.615 ms. These are still the legacy application backend; the database
is tested independently, not yet selected by production. Promise health is
unchanged from baseline (one pass, two fails, four at risk).

### Phase 3 — integration and safe import

The application now uses the database worker for all session reads and commits.
The legacy file writer/cleanup machinery is removed. Import normalizes v7 buffer
fields and legacy control-character flags, validates layout references and safe
snapshot IDs, commits the migration marker with the data, verifies a readback,
and leaves every source file untouched. Empty committed sessions do not reimport
old JSON. Failed/unsupported imports and corrupt database reads protect the store
against subsequent automatic overwrite. Cold-tab hydration now returns errors
instead of replacing failed content with an empty buffer.

Schema uses four tables: session state, independently updated tab metadata (with
nested buffer/history JSON), snapshot checksums, and bounded content chunks.
This avoids unnecessary joins for nested editor state while keeping large recovery
text out of metadata. Capture sequence numbers and one owner order immediate and
background saves; older captured requests cannot overwrite a newer commit.

All-features tests, formatting, and strict library/test Clippy pass. Both lenses
completed (`phase3`). In the session-store function selection excluding separate
test files, cognitive sum 147 -> 139, effort 998,120 -> 958,899, SLOC 2,077 -> 1,684
versus phase 2; cyclomatic sum increased 363 -> 370 with validation paths. History
and restored buffer fields are moved instead of cloned during reconstruction.

At 10,000 tabs: startup-visible restore ~0.175 ms and steady persistence ~31.62 ms
improve substantially. Initial resource persistence drops from ~4.93 s to ~0.62 s.
Full restore regresses to ~654.54 ms; the resource phase breakdown identifies
~90 ms database read versus ~591 ms serial reconstruction. This **does not pass**
the final non-regression requirement. Phase 5 must restore bounded parallel
reconstruction and measure again. Promise statuses remain unchanged from baseline.
Legacy profiling field names remain compatible with the lens: manifest read/parse
now includes coherent database content reads; manifest write includes commit.

### Phase 4 — parity and fault recovery

Added disk-full/rollback/retry, missing content, corrupt metadata before first load,
private-directory permissions, format/view/surface parity, clean-file history,
failed cold hydration, and application-level abrupt-exit recovery tests. The
ignored subprocess helper is actually invoked by its parent recovery test.
500 library tests and all integration/all-features tests pass; strict all-features
library/test Clippy and formatting pass. Both lenses completed (`phase4`).

Two correctness gaps found and fixed: replacing a document can reset its numeric
revision, so the database now compares weak source-tree identity plus generation
without retaining old text; and clean disk-backed buffers now restore their
exported history even when no recovery text was needed. Read/write paths reject
duplicate content identities instead of silently emptying a second buffer.

The disk-restore helper refactor reduced function SLOC 251 -> 227, cyclomatic sum
32 -> 31, effort 74,528 -> 71,492; cognitive sum stayed 13. Safety checks and tests
increase other totals; no claim is made that adding required checks reduces every
metric. Startup-visible restore 0.196 ms, steady persistence 33.779 ms, full restore
643.816 ms. The full-restore regression remains an explicit phase-5 blocker;
project-wide promise health remains the same as baseline.

### Phase 5 — bounded parallel reads, reconstruction and recovery export

Full restore now partitions content-ID ranges across at most eight read connections,
then reconstructs ordered batches on at most sixteen CPU workers. Small sessions
and startup-visible restore avoid that overhead. The exclusive owner processes
no writes during these reads, so all batches see the same committed state. A common
fallible scoped-batch helper joins every worker, preserves ordering, and propagates
spawn/panic failures rather than silently dropping tabs. Reconstruction failures
also protect the source against automatic overwrite.

Content chunks are locally ordered and verified by length, sequence and BLAKE3;
the full scan checks for orphan chunks outside partition ranges. Single-chunk
buffers transfer their owned bytes without another full-text copy. Bounded UTF-8
extraction coalesces fragmented document pieces, with each stored chunk <=256 KiB.
The writer uses an explicit 8 MiB page-cache setting and the portable syscall I/O
backend, not experimental MVCC or platform-specific asynchronous I/O. Healthy
shutdown drains the worker and explicitly checkpoints; checkpoint failures retain
the WAL and are diagnosed. Repeated DML uses prepared-statement caching rather than
recompiling SQL for every buffer. One retention-policy map replaces three cloned
identity sets. Failed writes invalidate all skip caches; autocommit-aware rollback
handling allows SQLITE_FULL retries, while a failed rollback protects reads and
writes until restart.

A correctness/performance cleanup drops obsolete recovery text only for hydrated,
clean, disk-backed buffers. Cold tabs retain their existing recovery content.
Clean files prefer available disk content over an obsolete optional backup; if the
file disappears the preserved cold backup remains recoverable. Tests cover both.
The export command below reads and verifies one committed logical snapshot, writes
private UTF-8 files and a v7 manifest last, and never overwrites a destination.

An isolated diagnostic is available as
`cargo run --release --example session_storage_profile`. It separates database read
and reconstruction timings and labels fresh/reused connections without pretending
to control the OS cache. Exploratory inline-content schema and oversized-cache
experiments did not improve the workload and were discarded; schema remains v1.
No benchmark workload or budget has been weakened. Target annotations now identify
the database implementation rather than the deleted legacy writer.

Final evidence: `target/turso-migration/phase5-final`. Both complete lenses ran;
505 library tests plus all integration/all-features tests pass, including the
subprocess helper invoked by its parent. Strict all-target/all-feature Clippy,
formatting, and `git diff --check` pass. Windows execution remains unverified:
this toolchain has no Windows standard-library target. Disk-full coverage uses
engine page-limit injection, not a physically exhausted filesystem.

| 10,000-tab operation | Initial baseline | Final |
| --- | ---: | ---: |
| Steady persist, Criterion mean | 69.850 ms | 31.546 ms |
| Startup-visible restore, Criterion mean | 16.162 ms | 0.220 ms |
| Full restore, Criterion mean | 121.966 ms | 112.887 ms |
| Initial persist, instrumented resource probe | 4,933.684 ms | 383.369 ms |

The three Criterion means and their confidence-interval upper bounds improve.
The instrumented full-restore sample is effectively flat: 134.593 -> 134.979 ms
(+0.3%, not a statistically established regression or improvement). These are
component measurements, not an end-to-end cold-start or cross-platform guarantee.
Repeat the archived comparison with:

```sh
python3 scripts/compare-session-baseline.py target/turso-migration/baseline target/turso-migration/phase5-final
# Explicitly fails today; producer exit 0 must not hide unmet project promises:
python3 scripts/compare-session-baseline.py target/turso-migration/baseline target/turso-migration/phase5-final --require-all-promises
```

**Acceptance limits:** session latency passes; all-project promise compliance does
not. Many Files and Search still fail, Many Tabs passes, and the other four remain
at risk, exactly as in the fresh baseline. The README now labels its older
seven-pass table as historical. No budgets or workloads were weakened.

Memory/allocation non-regression is also **not** achieved. At 10,000 tabs, initial
persistence's instrumented peak live heap rises from about 139 to 223 MiB, retained
growth from 0.83 to 20.2 MiB, and allocation traffic from 43.6 to 985 MiB. Atomic
transactions and engine/cache state trade memory for durability and substantially
lower latency; allocation traffic is not retained memory. Statement caching cut
initial persistence from 867 to 383 ms and allocation traffic from about 1,281 to
985 MiB versus the first phase-5 run. Further engine/allocation work remains.

Quality results are likewise mixed, not a claim that every aggregate improved.
Shared scoped batches, cached-DML handling, and one retention-policy map eliminate
repeated orchestration and identifier cloning. The persistence module's function
SLOC falls 188 -> 165 and effort 221,503 -> 178,635 versus the first phase-5 pass;
detected project duplication falls 7.54% -> 7.51%. Required fault checks, parallel
recovery, and export add code. In the same session-store function selection used
above (excluding separate test files), baseline -> final totals are cognitive
109 -> 172, cyclomatic 229 -> 436, effort 630,644 -> 1,168,009, and SLOC
1,619 -> 1,937. Project duplication is 7.33% -> 7.51%. Unweighted module locality
averages are 92.59 -> 90.40; leverage 32.50 -> 40.67. Module populations differ and
semantic identity includes fallback, so these are advisory observations, not a
verified architecture proof or a satisfied global complexity/LOC-reduction gate.

## Backup, recovery and downgrade

- The database and its WAL are one recovery unit. **Do not copy only `session.db`
  while Scratchpad is running**, nor open it with a different SQLite engine. Keep
  original legacy files until independent recovery exports have been verified.
- Build the optional offline helper with `cargo build --release --locked --bin
  session_backup`. Close Scratchpad, then run:
  ```sh
  target/release/session_backup /path/to/session-root /safe/existing-parent/new-backup
  ```
  The parent must exist and the destination must not. The helper takes the app's
  instance lock as well as the database ownership lock. It can import an old v7
  session first; source JSON/temp files remain untouched. The library export API
  can also operate through the already-owned worker without closing the app.
- A successful export contains `session.json` plus referenced `<id>.tmp` UTF-8
  files, not user documents, settings, or a raw database image. Clean saved files
  still depend on their original paths. Exported directories/files are 0700/0600
  on Unix; Windows uses inherited filesystem ACLs, which must be reviewed locally.
  Recovery text is not encrypted. An error can leave an incomplete destination:
  preserve it for inspection and retry to a different new directory.
- To recover, close the app and retain the **entire** failing session directory
  separately, including WAL and sidecars. Put a verified export in a fresh session
  directory. Startup imports it transactionally. Do not overwrite or delete the
  only damaged copy, and do not silently fall back to potentially stale legacy JSON.
- To downgrade to a v7 file-backed build, use a fresh export rather than the original
  pre-migration files. On Linux an export at `/safe/rollback-state/scratchpad` can
  be opened with `XDG_STATE_HOME=/safe/rollback-state old-scratchpad`; configuration
  and ordinary file paths are unchanged. Never run old and new writers against
  the same state directory.
- Foreign/newer schemas and corrupt or incomplete required recovery data fail
  closed. No automatic destructive repair, legacy deletion, VACUUM, or secure-erase
  claim is made. Checkpointing does not substitute for independent backups.
  To reclaim database high-water disk space, verify an export and import it into
  a fresh directory, retaining the original separately; never delete only its WAL.

## Intended sequence

1. Engine validation and baseline (this commit).
2. Database worker, bounded content chunks, transaction and incremental-write contract.
3. Safe, repeatable legacy import and application integration.
4. Behavioral parity and recovery fault tests.
5. Performance tuning, recovery export and operational cutover documentation.

Settings stay TOML; ordinary user document save paths and encodings are not
changed. Database failures must never silently replace an existing session with
an empty one. Legacy inputs remain untouched. No experimental MVCC, FTS, remote
sync, or mixed SQLite/Turso access is required.
