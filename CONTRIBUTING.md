# Contributing to Scratchpad

Thank you for helping improve Scratchpad.

## Development expectations

Scratchpad supports Windows and Linux, including NixOS/Hyprland. Treat both desktop targets as first-class: keep platform-specific behavior behind the existing platform, file-dialog, and file-watch boundaries, and test shared behavior independently of either windowing environment. Discuss substantial UI, persistence-format, or architecture changes before implementing them.

Before opening a pull request, run:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo hack check --feature-powerset
cargo clippy --lib --all-features -- -D warnings
cargo test --all-features
cargo audit
```

Check the pinned Turso versions and matching application/contract-probe registry
identities with:

```bash
cargo xtask check-turso-locks
```

The standalone `xtask/` crate avoids building the GUI or engine for maintenance
checks. Its Cargo alias uses `--locked`; intentional engine upgrades require
updating `xtask/src/main.rs`, both engine lockfiles, and reviewing the contract.
When changing the task, also run:

```bash
cargo fmt --manifest-path xtask/Cargo.toml --check
cargo clippy --locked --manifest-path xtask/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path xtask/Cargo.toml
cargo audit --file xtask/Cargo.lock
```

On Linux, also run the configured cross-target checks when the required toolchains are installed:

```bash
./scripts/check-targets.sh --require-all x86_64-unknown-linux-gnu x86_64-pc-windows-gnu
```

Changes should include focused regression tests. Document user-visible changes in `CHANGELOG.md`. Avoid unrelated formatting or refactoring, and explain panic or unsafe invariants where they cannot be removed.

Use fresh RustQualityLens evidence when changing reliability-sensitive code:

```bash
cargo build --locked --manifest-path ../rust-quality-lens/Cargo.toml --bin rqlens --target-dir target/rqlens-build
cargo xtask quality target/rqlens-build/debug/rqlens target/quality-runs/review-1
```

Use `.exe` on Windows. The output directory must not exist. The task copies the
executable, preserves the configured policies in an isolated configuration, runs
`measure all`, `verify`, then strict `check`, and writes `summary.json` plus separate
stdout/stderr logs. It records tool/source hashes, toolchain versions and relevant
build flags; source changes during measurement fail the run. It never reuses
`target/analysis` artifacts. Dirty source hashes detect changes, but are not a
reconstructible archive: commit or preserve the source separately for comparisons.

A successful producer exit is not complete evidence. Missing tools, failed
commands, partial/unknown required evidence, and actual policy violations are
reported separately and cannot produce a passing summary. Read `check.stdout.log`
for the authoritative rule/freshness diagnostics. Known macro/fingerprint gaps
still prevent a complete RQL gate; do not raise budgets or suppress failures to
make it green. The configured limits exclude profiling binaries. Lower a limit
when findings are removed; increases require a documented accepted risk.

`nix develop` supplies cargo-audit, cargo-hack, cargo-llvm-cov, actionlint and
shellcheck. The Linux CI job runs the expensive strict evidence task on manual
workflow dispatch, with a pinned RQL revision, and uploads even failed/partial
reports. Normal PR jobs retain native Windows tests, all-target Clippy and an
explicit dependency audit. CI cross-target checks use `--require-all` so missing
Windows tooling is an error, not a silent skip. Without that flag the local script
still reports unavailable targets as skips; neither behavior constitutes native
Windows runtime validation.

Session performance helper commands live in the sibling Rust performance lens,
not in Scratchpad's maintenance `xtask`. From this checkout:

```bash
cargo run --locked --manifest-path ../scratchpad-performance-lens/Cargo.toml --bin splens -- \
  compare-session-baseline target/turso-migration/baseline target/turso-migration/phase5-final

cargo build --release --locked --bin resource_probe
cargo run --locked --manifest-path ../scratchpad-performance-lens/Cargo.toml --bin splens -- \
  measure-large-dirty-save --binary target/release/resource_probe \
  --output target/session-save-runs/new-run --launches 3
```

These replace `scripts/compare-session-baseline.py` and
`scripts/measure-large-dirty-save.py`. The comparison optionally accepts
`--require-all-promises`; the probe runner requires a new output directory and
an explicit executable path (adjust for `CARGO_TARGET_DIR`, cross-target builds,
or `.exe` on Windows). Measurement orchestration and report policy belong to the
lens; the production-dependent probe remains in Scratchpad. See the lens's
`docs/session-diagnostics.md` for the evidence contract.

Follow `CODE_OF_CONDUCT.md`. Report vulnerabilities using `SECURITY.md`, not a public issue.
