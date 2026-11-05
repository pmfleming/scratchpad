# Live text storage: stage 0

Date: 2026-11-05. Application baseline: `087e045`. Implementation plan:
`docs/turso-live-text-storage-plan.md`, reviewed against that same commit.

## Decision: no-go for stage 1

Do not enable multiprocess WAL in the application or begin the dependent storage
rewrite yet. The optional frame-index overflow contract fails reproducibly on
Turso 0.8.3 in Linux multiprocess mode. The equivalent standard-WAL control passes.
This meets the plan's explicit instruction to stop and reassess when Linux
multiprocess WAL fails its stage 0 checks. MVCC remains disabled, and no
performance budget has been raised.

This commit implements measurement prerequisites and preserves the failed gate.
Stages 1–7, including all application storage, schema, and degraded-operation
changes, remain unimplemented. A no-go is not a completed performance acceptance
or permission to release the new storage design.

## Comparison with the current code and recent Git history

The working tree was clean on entry. HEAD exactly matched the plan's review base;
there were no later commits to reconcile. Source inspection confirmed the two
temporary backings, 256 KiB chunks, per-original 8 MiB cache, process-wide add
cache, mutex-held I/O, infallible text reads, payload-bearing history export on
every save, and queued `writes_blocked()` calls described by the plan. The app
still opens standard WAL with syscall I/O and FULL/8 MiB/foreign-key pragmas.

The recent commits impose correctness constraints on the later rewrite:

| Recent changes | Behavior to preserve |
| --- | --- |
| `c753615`, `091dcc7` | Reject malformed captures while omitting loading previews from durable ownership. |
| `07d8ef7`, `5a74037` | Disk opens and tab/pane closes remain possible when session storage is unavailable. |
| `ad07c29`, `0af183e` | Retry superseded saves, report failed disk checks, and preserve retained file identity after temporary failures. |
| `cae31c0`, `fb6e39f`, `2671b13` | Saved text and encoding reopens do not create false external-change observations. |
| `09dc756`, `d49000c`, `1ec45fe` | Pane promotion, Timeline hydration, and closing cold split panes preserve document lifecycle and ownership. |

One measurement assumption was stale: `resource_probe --large-dirty-save` expected
1,024 undo entries and 16 MiB of history. Initial contents now add one replayable
34 MiB Load entry. The original assertion failed with 1,025 entries. Fixture
revision 2 measures all 50 MiB of retained history, keeps the same 50 MiB document
and nine one-character saves, and checks 1,034 undo entries after reopen. No
history was removed to make the old measurement pass.

## Contract and baseline changes

The contract now matches application I/O/pragmas and explicitly propagates the
WAL mode to crash children. It adds a bounded dedicated reader, real concurrent
reads/writes, pinned snapshots, writer Busy checks, checkpoint assertions,
application-cache read timings, synthetic reader panic containment, a page-limit
DatabaseFull check, and NORMAL/FULL process-exit recovery with live slots.

The busy TRUNCATE result is asserted as `(1, NULL, NULL)`, rather than attempting
to decode its NULL frame counts as integers. After the reader ends its snapshot,
the normal-size tests require a successful TRUNCATE and a zero-length WAL.

The focused `--live-text-baseline` reuses existing file, history, paste, large-text
scanner search, and many-tab session cases. It adds shuffled 4,096-character viewport
windows to measure application-cache misses on the old file backing. The full
resource suite retains the unrelated many-file/search-workspace cases. Its first
trial under build load hit the existing two-second search-workspace timeout;
that deadline was not relaxed. The focused search case scans an owned string;
it does not establish file-backed search latency.

Linux CI now runs both regular WAL contracts, and Windows CI runs the standard
contract natively. The optional overflow test deliberately stays outside routine
CI and returns a failure exit status when the engine violates its contract.

## Overflow reproducer

Use the two `--wal-overflow` commands in [README.md](README.md). The test pins an
old reader snapshot and commits four 256 KiB rows per transaction. It attempts
4,352 rows, taking the WAL past 262,144 frames. The data and database paths are
disposable, and no application session is opened.

The multiprocess run reaches chunk 4,049 with a 1,080,717,232-byte WAL, then
`BEGIN IMMEDIATE` returns `Busy("database is locked")`. The same point reproduced
on repeated runs. The threshold is consistent with the pinned engine's shared
frame-index capacity; that correlation does not establish the precise engine
root cause. The standard-WAL run passes with the same pinned-reader workload.

The diagnostic preserves the original failing transaction context and reports
the effect of releasing the snapshot and reopening connections separately. The
writer retry after ending the snapshot still returns Busy, while reporting
autocommit true. Dropping both connections and database handles and reopening in
the same process also returns Busy. No successful recovery at this overflow
boundary is claimed. The gate remains a failure even if a later recovery
succeeds. Do not discard this
result as ordinary contention: the only other connection is a reader, and no
second writer exists during the bulk loop.

## Evidence and limits

The [captured evidence](evidence/stage-0-2026-11-05/results.json) includes raw
contract logs, 36 successful resource events, all 34 Criterion cases from the
three requested suites, environment information, and source/binary hashes. The
dirty-save workload completed all nine saves and its exact-text/history reopen
assertions. Both regular Linux contracts and the standard-WAL overflow control
passed; the multiprocess overflow contract exited with status 1.

Validation also passed for release builds, formatting of the application and
standalone probe, Clippy with warnings denied for both affected executables,
`cargo xtask check-turso-locks`, and `actionlint`. These changes affect profiling
and contract executables plus CI, so the full application regression suite was
not rerun locally. There are no production application code changes.

Timing runs share a host with
concurrent Rust/Nix builds. They establish reproducible workloads and observed
results, not an uncontended budget confirmation. OS pages remain warm, so the
read metrics do not measure cold physical storage. Process peaks retain their
existing lifetime-high-water scope, including earlier cases.

The page ceiling is not real ENOSPC/fsync injection. The panic is injected at the
reader boundary, not an induced engine bug. Abrupt process exit is not power
loss: these results do not justify NORMAL chunk commits in production. Windows
runtime results await CI; Linux standard WAL does not substitute for them.

After the failed gate, the following remain deferred: uncontended budget
confirmation, typing during a save, file-backed search latency, several-GiB database read latency, real disk
failure and unsupported-filesystem tests, CDC versus fixed chunks, 100,000 typed
history entries versus packed spans, and typed layout/document comparisons at
10,000 tabs. There is no claim that every stage 0 acceptance check passed.

Before proceeding, resolve or explicitly redesign around the multiprocess
overflow failure, rerun its control/recovery tests, obtain native Windows
results, and complete the remaining stage 0 measurements. In particular, any
reader snapshot lifetime bound or WAL-mode policy change needs to be reflected
in the plan and validated rather than assumed to cure the failure.
