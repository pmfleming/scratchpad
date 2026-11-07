# Turso research and applicability — 2026-11-07

Scratchpad uses the embedded Rust **turso =0.8.3**, default features disabled,
with **turso_core 0.8.3**, standard WAL, `IoBackend::Syscall`, one serialized
writer actor and dedicated reader threads. Cargo's package VCS identity is
`607bff9fd0e547a25a97b3bf06e83bcc5d99d53c`. The app and contract-tool locks
must continue to agree. It does not use libSQL, hosted HTTP queries, replicas
or cloud sync. [The projects explicitly distinguish libSQL and Turso](https://github.com/tursodatabase/libsql/blob/main/README.md).

Research combines the actual downloaded 0.8.3 crate source, pinned documentation,
current release notes, issue reports and the historical database/memory experiments.
Main-branch documentation alone cannot establish a feature's availability in 0.8.3.

| Area | Evidence and applicability |
|---|---|
| Async architecture | The engine exposes asynchronous I/O; Scratchpad drives it with futures-lite on dedicated threads. An async function does not make foreground calls nonblocking automatically. Keep work on the actor/path lane. [Engine README](https://github.com/tursodatabase/turso). |
| Concurrency | Standard WAL still serializes writers. `BEGIN CONCURRENT` requires MVCC and commit-conflict retries. The published throughput gains concern concurrent transactions on different keys, not this single actor. A wholesale mode change would require new recovery/overflow validation. [Manual](https://raw.githubusercontent.com/tursodatabase/turso/v0.8.3/docs/manual.md), [0.8 performance release](https://turso.tech/blog/turso-0.8.0), [concurrency guidance](https://turso.tech/blog/concurrent-writes-in-practice). |
| Durability | Bodies currently commit NORMAL, metadata/manifests FULL. In 0.8.3 `Connection::set_sync_mode_for_database` changes connection state and always bumps prepare-context generation; `Statement::_step` detects the mismatch and reparses/retranslates. A candidate is permanently configured connections on the SAME Database and actor, committing body transactions before a manifest starts; cycle 1 tested but did not retain it without convincing native latency evidence. Keep FK enforcement, rollback and FULL metadata writes. [Pinned source](https://github.com/tursodatabase/turso/blob/v0.8.3/core/connection.rs), [statement source](https://github.com/tursodatabase/turso/blob/v0.8.3/core/statement.rs). |
| Prepared statements | The Rust `prepare_cached` obtains a cached program but builds execution state. App VM reuse saves more work only while connection context remains stable. Fully exhaust result cursors and drop failed VMs before retry. Bound caches. [Rust API source](https://github.com/tursodatabase/turso/blob/v0.8.3/bindings/rust/src/connection.rs). |
| Batching | Keep 32-row parameter batches and 1 MiB body batches. Larger body transactions were worse in the historical experiments and lengthen actor scheduling gaps. No SQL batching can remove the FULL boundary itself. |
| I/O backend | Linux io_uring is available. **Correction to the historical ideas file:** the actual pinned `UringIO::open_file` ignores `_direct` and calls ordinary `File::options().open`, without O_DIRECT. Do not explain its historical lack of gain using a direct-I/O assumption. Async submission still does not guarantee lower latency for this actor workload; keep syscall until a controlled workload proves a benefit. [Pinned backend source](https://github.com/tursodatabase/turso/blob/v0.8.3/core/io/io_uring.rs). |
| Page/cache size | Cache size is connection-specific; increasing database page caches is distinct from retaining verified text. The historical 256 MiB writer/64 MiB reader experiments did not show a reliable gain, while larger pages increased import time. Keep existing 4 KiB default and respect existing file page sizes. |
| SQLite-only tuning | `mmap_size`, `optimize`, `journal_size_limit`, `defer_foreign_keys` are not supported in the pinned compatibility table. Do not send speculative PRAGMAs and assume success. [Pinned compatibility table](https://raw.githubusercontent.com/tursodatabase/turso/v0.8.3/COMPAT.md). |
| Checkpoints | The 0.8.3 source has a fixed automatic checkpoint threshold; newer prerelease support does not exist in this pin. Historical larger-threshold tests produced longer fsync stalls, not better imports. Preserve explicit checkpoints and reader snapshot lifetime tests. [Releases](https://github.com/tursodatabase/turso/releases). |
| WITHOUT ROWID | Pinned support is experimental and effectively insert-only, rejecting operations/FKs used by this schema. Not applicable. [Compatibility](https://raw.githubusercontent.com/tursodatabase/turso/v0.8.3/COMPAT.md). |
| BLOB streaming | Core internals have blob support, but the pinned Rust public binding does not expose the needed incremental API. External body pack files would alter atomicity/recovery/GC and violate the current same-database design. Keep relational bodies. [Rust binding](https://github.com/tursodatabase/turso/tree/v0.8.3/bindings/rust/src). |
| Optimizer/indexes | ANALYZE and stat1 are available. Current restore deliberately uses typed joins and has query-plan tests; don't assume SQLite's planner behavior. Removing indexes/integer-key migration is a separate schema experiment, not a configuration tweak. |
| FTS/vector/CDC/sync | They add semantics and maintained state irrelevant to exact regex/plain-text editor search. FTS token search cannot replace the existing exact-range contract. Hosted/replication features are not needed for local UI latency. |

## Fresh upstream issue review

[Issue 11546](https://github.com/tursodatabase/turso/issues/11546) reports a
statement retaining Busy state after an attached-database lock is released.
Scratchpad doesn't use ATTACH, so this is not evidence of the same bug here;
it supports retaining explicit error/VM cleanup and testing retries.
[Issue 11547](https://github.com/tursodatabase/turso/issues/11547) reports changes/
total_changes discrepancies in DDL, OR FAIL and virtual-table cases. These
are not the app's normal statements. Inventory invalidation must nevertheless
remain scoped to the manifest connection; a body commit on another connection
must not acknowledge unrelated manifest changes. Tests must verify this directly.
Neither issue is assumed fixed in 0.8.3 or grounds for an untested upgrade.

The release listing currently exposes 0.8.3 and 0.9.0-pre.2. A prerelease is
not selected merely for a newer feature; the pin and existing WAL contracts remain.

## RAM, publication and recovery are different endpoints

Retaining verified, already committed text can accelerate repeated searches
without accepting more unsaved edits. Raising pending-body admission headroom
can reduce paste preparation waits but changes the amount of NORMAL-uncommitted
text allowed in RAM. Neither cache occupancy nor body flush establishes FULL
session recoverability: the history, layout and text references need a FULL
manifest commit. Process-exit/reopen tests check recovery; they cannot simulate
power loss or prove hardware fsync behavior. Report publication, NORMAL flush,
FULL manifest completion and exposure intervals individually.

The uncommitted historical `database-and-memory-ideas.txt` is a hypothesis source,
not a fresh benchmark: it reports fewer re-prepares with two connections, lower
paste latency with more pending RAM, and lower repeat-search latency with retained
text. This campaign will measure those changes in the production app with bounded
queues/caches, retaining every failure and outlier.
