# Pinned Turso engine contract

This executable creates disposable databases under the system temporary directory.
It never needs an application session or a running window. The application and
probe pin Turso 0.8.3; `cargo xtask check-turso-locks` checks their engine identities.

Run the regular contract in standard WAL mode (also the default with no arguments):

```sh
cargo run --release --locked --manifest-path tools/turso-contract/Cargo.toml -- --wal-mode standard
```

On Linux, also run:

```sh
cargo run --release --locked --manifest-path tools/turso-contract/Cargo.toml -- --wal-mode multiprocess
```

The regular contract runs in Linux and native Windows CI, with multiprocess WAL
tested on Linux only. It asserts correctness, without timing gates tied to CI
hardware. Set an external process timeout when running against a suspect engine;
the reader response timeout cannot interrupt a stuck engine call or destructor.

## Live-text checks

Both modes use `IoBackend::Syscall` and the application's connection pragmas:
`synchronous=FULL`, `cache_size=-8192`, and `foreign_keys=ON`. Dedicated reader
connections override their page cache to `-512` KiB. Crash subprocesses inherit
the selected WAL mode explicitly.

The executable checks:

- Commit/rollback, a 16 MiB BLOB, strict tables, foreign keys, backup integrity,
  abrupt process exit, recovery, and checkpointing.
- No dirty reads across a thread/connection boundary, snapshot isolation, and an
  immediately contending writer returning `Busy` with no busy timeout.
- Four-chunk FULL transactions while an old reader snapshot keeps WAL frames
  alive, and TRUNCATE reporting Busy until that snapshot is released. Turso 0.8.3
  returns `(1, NULL, NULL)` for this busy checkpoint result.
- Shuffled 256 KiB reads through a bounded reader hop and direct reads, followed
  by reader requests that overlap another thread's writes and auto-checkpoints.
- An injected panic contained by the reader boundary and exact text readable on
  a replacement connection. This tests containment, not an engine panic trigger.
- `DatabaseFull` under `max_page_count`, rollback, and preserved committed text.
  This is not a physical ENOSPC or failed-fsync simulation.
- A NORMAL chunk transaction followed by a FULL manifest transaction, then exit
  with active reader and writer slots. Reopen must keep both committed rows,
  omit the uncommitted row, pass integrity checks, and truncate the WAL. This
  verifies process-exit recovery only; it does not prove power-loss durability or
  authorize NORMAL chunk commits in the application.

`METRIC` lines report microseconds and nearest-rank p99; `CHECK` lines report
asserted behavior. Read timings include result allocation and byte comparison.
The OS page cache remains warm: these are application-cache/page-cache tests,
not physical cold-storage latency measurements. Record competing build/load
activity before using the numbers to choose performance budgets.

## WAL frame-index overflow gate

This optional test inserts 4,352 chunks (1,088 MiB), keeping one old reader
snapshot open, to exceed the shared frame index's 262,144-frame capacity. Allow
several GiB of free temporary disk space. It runs outside routine CI:

```sh
cargo run --release --locked --manifest-path tools/turso-contract/Cargo.toml -- --wal-mode standard --wal-overflow
cargo run --release --locked --manifest-path tools/turso-contract/Cargo.toml -- --wal-mode multiprocess --wal-overflow
```

The multiprocess test currently fails on the pinned engine when a new writer
transaction returns Busy at the capacity boundary. It prints the chunk and WAL
size, then diagnoses whether releasing the old snapshot restores writes and
TRUNCATE. It still exits unsuccessfully, preserving the failed go/no-go gate.
Do not turn this into a passing test by accepting Busy, raising a budget, or
enabling MVCC. See the stage 0 report and captured evidence before enabling this
mode in the application.

The [stage 0 report](STAGE-0.md) records the code/history comparison, no-go
decision, validation, and remaining work. Its
[evidence directory](evidence/stage-0-2026-11-05) preserves the raw runs.

## Application baseline

The existing production-dependent resource probe has a focused live-text suite:

```sh
cargo build --release --locked --bin resource_probe --benches
SCRATCHPAD_FILE_BACKED_OPEN_MAX_BYTES=268435456 target/release/resource_probe --live-text-baseline > baseline.jsonl
```

The size cap applies only to large-file fixture cases. Remove it for the existing
up-to-2-GiB workload. This suite retains the existing file, edit-history, paste,
large-text scanner search, and 100/1,000/10,000-tab session measurements, plus random
file-backed viewport windows. It excludes unrelated many-file and search-workspace
cases; the default resource suite still runs those cases.
The existing search case scans an owned string, so a file-backed search measurement
is still needed before accepting the storage change.

The dirty-save fixture is revision 2: its 34 MiB Initial contents entry plus 16 MiB
of paste history total 50 MiB of retained history, with 1,025 initial entries.
It performs nine one-character saves, reports actual payload sizes, then verifies
exact text and 1,034 entries after reopen. This corrects the old fixture's stale
1,024-entry/16-MiB assumptions; compare against revision 2 in future runs.

Also run the unchanged `promise_latency`, `frame_budget`, and `search_speed`
Criterion suites and preserve their output/baseline estimates. Measurement
orchestration and budget policy remain in Scratchpad Performance Lens.
