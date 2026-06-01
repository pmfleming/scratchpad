# Scratchpad

Scratchpad is a Windows-first plain-text editor written in Rust. It is built as
a safer, more capable Notepad replacement for everyday text work: notes, logs,
exports, terminal output, reports, encoded files, and temporary scratch work.

The app is deliberately not an IDE. It focuses on fast local editing, resilient
session restore, visible file-format risk, and multi-file workspaces without
language servers, plugin execution, or cloud sync.

## What It Does Today

- Opens files as separate tabs or into the active workspace as editor tiles.
- Lets a workspace tab contain multiple tiled views, including several views of
  the same file.
- Supports tile splitting, divider resizing, tile promotion, tab combining,
  drag/drop tab ordering, multi-tab selection, and tab overflow.
- Provides search and replace across the selection, active file, current
  workspace tab, or all open tabs.
- Supports plain-text and regex search, case-sensitive matching, whole-word
  matching, replace current, and guarded replace-all.
- Detects BOMs and common encodings, preserves supported BOM and line-ending
  metadata, and warns before saves that may lose or corrupt characters.
- Makes control characters and artifact-heavy text visible, including ANSI
  escape sequences, carriage-return output, overprint patterns, and other
  non-printing characters.
- Keeps per-document undo/redo history plus a text-history dialog for reviewing
  and navigating recent edit operations.
- Restores sessions, open buffers, workspace layout, settings, tab placement,
  pane layout, and file metadata.
- Includes in-app settings for text formatting, appearance, opening behavior,
  tab placement, status bar visibility, and undo memory budgets.
- Supports command-line startup switches for clean launches, session restore,
  opening files, and adding files into an existing workspace.

## Design Principles

- Keep everyday text editing fast, predictable, and local.
- Show risky encoding, newline, and disk-state decisions before bytes are
  written.
- Treat restored state as important user data, especially unsaved work.
- Keep the product centered on plain text instead of drifting into a coding
  workflow.
- Use profiling, capacity probes, benchmarks, and CI as regular development
  inputs.

## Build, Test, and Run

Prerequisites:

- Rust via `rustup`
- Windows for the primary app target and installer flow
- .NET local tools when packaging the Windows MSI

Common development commands:

```powershell
cargo run --release
cargo test
cargo clippy --lib --all-features -- -D warnings
cargo fmt --check
cargo build --release
```

Useful runtime switches:

```powershell
scratchpad.exe /help
scratchpad.exe /version
scratchpad.exe /clean "C:\notes\a.txt"
scratchpad.exe /here "C:\notes\a.txt"
scratchpad.exe /addto:active /files:"C:\notes\a.txt","C:\notes\b.txt"
scratchpad.exe /addto:index:2 "C:\notes\c.txt"
```

## Packaging and Release

The release workflow is Windows-based. It checks formatting, runs clippy and
tests, verifies that the release tag matches `Cargo.toml`, builds the release
binary, packages a Windows MSI, signs artifacts when signing is configured, and
uploads both the installer and checksum.

Release tags use the `vX.Y.Z` format, matching the package version in
`Cargo.toml`.

Local installer packaging:

```powershell
dotnet tool restore
cargo build --release --locked
.\scripts\package-windows-installer.ps1 -Version 0.40.0
```

## Measurement

Scratchpad owns the Rust probes and benchmarks that compile against the app.
The reusable measurement producers and dashboard live in sibling repositories.

Scratchpad-owned measurement entry points include:

- `src/bin/capacity_probe.rs`
- `src/bin/frame_metrics.rs`
- `src/bin/resource_probe.rs`
- `src/bin/profile_*.rs`
- `benches/search_speed.rs`
- `benches/frame_budget.rs`

Generated measurement artifacts are written under `target/analysis/` for the
sibling dashboard to consume.

The current measurement boundary is documented in
[docs/measurement-tools.md](docs/measurement-tools.md).

## Repository Map

```text
src/
  main.rs                 Desktop entry point and native window setup
  lib.rs                  Public crate surface
  app/
    app_state/            Frame loop, workspace state, settings, search state
    chrome/               Custom window chrome and caption buttons
    commands/             Command dispatch and workspace operations
    domain/               Buffers, piece-tree storage, panes, tabs, views
    services/             File IO, search, sessions, settings, background work
    startup/              Command-line parsing and startup options
    ui/                   Editor, dialogs, settings, search/replace, tabs
  bin/                    Profiling, resource, frame, and capacity probes
crates/
  windows_file_watch/     Windows file-change watching helper crate
benches/                  Criterion benchmark targets
scripts/                  Release and analysis helper scripts
packaging/                Windows installer definition
docs/                     User manual, architecture notes, and reviews
assets/                   Project artwork
fonts/                    Bundled editor and control-symbol fonts
```

## Key Documentation

- [User manual](docs/user-manual.md)
- [Measurement tools](docs/measurement-tools.md)

## Technical Notes

- Stack: Rust 2024, `eframe`/`egui`, `egui-phosphor`, `rfd`, `serde`,
  `encoding_rs`, `chardetng`, `regex`, `smallvec`, `sysinfo`, and a local
  Windows file-watch crate.
- Unsafe Rust is forbidden at the crate level.
- Text storage uses a piece-tree-backed document model with undo/redo history
  and cheap snapshots for save and session flows.
- Runtime logs are written under `log/`.
- Session state, `settings.toml`, and eframe persistence live under the
  Scratchpad data directory in the OS temp location.
