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
