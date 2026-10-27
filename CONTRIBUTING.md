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
```

On Linux, also run the configured cross-target checks when the required toolchains are installed:

```bash
./scripts/check-targets.sh x86_64-unknown-linux-gnu x86_64-pc-windows-gnu
```

Changes should include focused regression tests. Document user-visible changes in `CHANGELOG.md`. Avoid unrelated formatting or refactoring, and explain panic or unsafe invariants where they cannot be removed.

Run the sibling Rust Quality Lens checkout when changing reliability-sensitive code:

```bash
cargo run --manifest-path ../rust-quality-lens/Cargo.toml -- measure reliability --config rqlens.toml
cargo run --manifest-path ../rust-quality-lens/Cargo.toml -- check --config rqlens.toml
```

The configured limits exclude profiling binaries and prevent new application panic paths. Lower a limit whenever an existing finding is removed; do not increase one without documenting the accepted risk.

Follow `CODE_OF_CONDUCT.md`. Report vulnerabilities using `SECURITY.md`, not a public issue.
