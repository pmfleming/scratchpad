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
