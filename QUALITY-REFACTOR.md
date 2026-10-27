# RustQualityLens review and refactor

For the later review against `03f6691`, see
[Recovery quality review](QUALITY-RECOVERY-REFACTOR.md). The results below retain
their original baseline.

## Measurement basis

Baseline: `a3f4cd2`. Tool: local `../rust-quality-lens` at `d28f3d9`
(`rqlens 0.1.0`), frozen before measuring either revision. Rust 1.95.0,
LLVM 21.1.8, cargo-llvm-cov 0.8.5.

Raw reports, baseline source, configuration and the frozen executable are in
`target/quality-refactor-20261027/`. The scan includes `src`, `tests`, `crates`,
`benches`, `examples` and `scripts`. Results below describe authored Rust, not
generated dependencies or Markdown.

| Metric | Before | After |
| --- | ---: | ---: |
| Function cognitive complexity, summed | 5,203 | 5,134 |
| Function cyclomatic complexity, summed | 10,308 | 10,235 |
| Function Halstead effort, summed | 32,913,035 | 32,459,722 |
| Nonblank Rust source lines | 78,017 | 77,892 |
| Duplicated nonblank lines | 5,760 (7.38%) | 5,521 (7.09%) |
| Authored unsafe blocks | 14 | 8 |
| Glob imports | 8 | 6 |
| Registered tests in source inventory | 600 | 596 |
| LLVM project line coverage | 65.81% | 65.97% |
| LLVM project function coverage | 68.60% | 68.86% |
| Standalone Linux watcher line coverage | 85.61% | 95.96% |

These are syntax measurements, not runtime performance claims. Nested functions
contribute to the old watch-loop measurements. SQL is opaque to the Rust
complexity metrics. Macro expansion and the Cargo dependency fingerprint are
incomplete; dependency resolution is disabled in the isolated configuration.
Architecture scores therefore remain **advisory**, not complete semantic graphs.

LLVM's default filename filtering includes the lifecycle fixture subdirectory
and the new shared test-support file. Excluding both from both reports still
improves line coverage: **33,694/51,285 (65.70%) → 33,737/51,266 (65.81%)**.
Branch coverage was unavailable. The standalone watcher is measured separately,
not added to the root application's coverage denominator.

## Changes and rationale

### Linux watcher: remove hand-written syscall machinery

`crates/linux_file_watch/src/lib.rs` now uses Rustix's owned descriptor, typed
inotify flags and checked `Reader`, instead of raw FFI, manual event-layout
parsing and application-side unsafe casts. The reader still uses an uninitialized
byte buffer through its safe API; unsafe implementation details move to Rustix,
not out of the dependency tree.

A local `Watches` owner handles configuration, retries, dispatch and draining.
Unchanged watches survive reconfiguration. Missing-path discovery borrows paths
instead of cloning the entire watched set every polling interval. Channel/path
clones needed to transfer ownership remain. Spawn and terminal read errors are
reported rather than panicking or being silently discarded.

`linux_watch_loop`: cognitive **38 → 3**, cyclomatic **29 → 7**, Halstead effort
**437,215 → 16,153**. Tests cover actual writes, deletion/recreation, root
replacement, overflow, ignored events and deduplicated missing-path errors.
Thread-spawn failure and every kernel-error branch are not fault-injected.

### Persistence: one owner for row handling and history validation

`database/mod.rs` supplies narrow row-collection and digest-collection helpers.
History loading and fingerprint hydration share identity/order validation in
`database/history.rs`. Fingerprints still decode and release **one history row at
a time**; the refactor does not materialize a second full history to hash it.
Query failures remain store-level errors; malformed history rows retain their
buffer-scoped damage attribution. Commit-only cache acceptance is unchanged.

Save-point eligibility and creation use one `INSERT … SELECT … RETURNING`
statement. This removes the separate latest-point/digest queries and reliance on
`last_insert_rowid()`. The query deliberately owns the time/digest predicate;
its complexity has not magically disappeared because RustQualityLens cannot
score SQL. Tests check clock rollback, 299/300-second boundaries, unchanged
content, absent snapshots, retention, shared chunks and transactional rollback.

`checkpoint_version`: cognitive **10 → 1**, cyclomatic **25 → 7**, Halstead effort
**104,492 → 28,461**. Retention limits, schema **6** and recovery semantics remain
unchanged. No user session was migrated/reset and no original file was rewritten.

### Recovery ownership, redundant work and reuse

Version recovery belongs to `app_state/workspace/lifecycle.rs`, not menu rendering.
The menu no longer constructs documents or clones the buffer name and temporary
ID just to render the menu. Recovery still creates a separate untitled tab.
The unused close-tab description allocation/return, duplicate tab-creation wrapper
and unused `ClosedRow` clone implementation were removed.

The menu module's advisory locality score improves **82 → 85**; its observed
outbound dependencies fall **11 → 10**. Shared headless fixtures in
`app_state/test_support.rs` have **10 observed consumers**, replacing duplicated
store initialization and frame-driving code. Their observed-reuse leverage score
is 100. These are concrete reuse changes, not a claim that every architecture
score improved or that the whole application has no dead code.

### More meaningful coverage with fewer test registrations

- Consolidated overlapping last-tab and successful-save cases, retaining their
  settings, multi-tab, conflict-confirmation, dirty-state and persistence checks.
- Replaced direct hash comparisons with persisted-row round trips for five
  metadata/payload mutations, while retaining the large-payload cache and restart
  checks.
- Replaced the store-only reopen test with a real application recovery journey:
  close, restart, reopen, consume the archive, recover a version, reject an expired
  version, undo the reopened text and preserve an externally changed original file.
- Reused headless fixtures across lifecycle, file-controller, drag, accessibility,
  settings, search and background-I/O tests. Test-source LOC is approximately flat;
  the reduction in test volume is four registrations, not a large test-LOC deletion.
- Coverage exposed two save tests using a one-second wait that silently timed out.
  Shared test waits now assert completion with a five-second deadline. The final
  default-concurrency coverage run passed; production wait behavior is unchanged.

Production-file line coverage improves:

| File | Before | After |
| --- | ---: | ---: |
| `app_state/workspace/lifecycle.rs` | 32.73% | 47.24% |
| `session_store/retained.rs` | 76.09% | 100% |
| `session_store/database/history.rs` | 95.68% | 98.44% |
| `session_store/database/versions.rs` | 99.09% | 100% |

The test-quality heuristic reports three assertionless tests instead of two:
the watcher journey delegates its deadline assertion and event predicate to
`wait_for`. This is not an unvalidated smoke test. No additional tests were ignored
and no lint suppression was introduced. Reliability-finding count remains 784.

## Validation and reproduction

Passed:

```sh
cargo fmt --all --check
cargo fmt --manifest-path crates/linux_file_watch/Cargo.toml --check
cargo test --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path crates/linux_file_watch/Cargo.toml --locked
cargo clippy --manifest-path crates/linux_file_watch/Cargo.toml --all-targets --locked -- -D warnings
git diff --check
```

Root library: **527 passed, 2 ignored**; root binary/integration/doc suites also
passed. Watcher: **2 passed**. Its ignored standalone lockfile was locally resolved
to Rustix 1.1.4, matching the root lockfile; an earlier standalone 1.1.6 run also
passed. Existing Linux CI already runs the standalone watcher tests.

Reproduce measurements with the frozen configuration:

```sh
q=target/quality-refactor-20261027
for tool in hotspots clones escape-hatches reliability locality leverage test-quality; do
  "$q/rqlens" measure "$tool" --config "$q/rqlens.toml"
done
CARGO_LLVM_COV_TARGET_DIR="$PWD/target/llvm-cov-target" \
  "$q/rqlens" measure coverage --config "$q/rqlens.toml"
CARGO_LLVM_COV_TARGET_DIR="$PWD/target/llvm-cov-target" \
  cargo llvm-cov report --json > "$q/current-llvm.json"
CARGO_LLVM_COV_TARGET_DIR="$PWD/$q/watcher-cov" \
  cargo llvm-cov --manifest-path crates/linux_file_watch/Cargo.toml --locked \
  --json --output-path "$PWD/$q/current-watcher-llvm.json"
```

Inspect the coverage artifact's `measurement_confidence.complete`, not just the
producer's exit code: RustQualityLens can successfully emit an unavailable report
after a failing test run. Evidence paths are local build artifacts, not committed
fixtures. No packaged/native application was deployed or restarted.

## Follow-ups deliberately not folded into this refactor

The Windows watcher still needs native Windows validation before similar changes.
`write_prepared` and chunk writing remain persistence hotspots; their transactional
boundaries deserve a separate measured review. The shortcut definition match has
high cyclomatic complexity because it enumerates commands; replacing it solely to
lower a number would not establish a readability or safety improvement.
