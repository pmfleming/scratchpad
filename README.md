# Scratchpad

Scratchpad is a Windows-first Rust text editor for everyday text work. It is
intended to be a safer, more resilient Notepad replacement: small enough to
trust, explicit about file-format risk, and measured continuously so
performance and complexity stay visible while the editor evolves.

The project is deliberately focused on plain text instead of software
development workflows. It does not try to be an IDE, language server host, or
plugin platform. Its center of gravity is notes, logs, exports, copied terminal
output, reports, encoded files, and temporary scratch work.

## What Scratchpad Does

- Opens text files into separate tabs or into multi-pane workspace tabs.
- Lets one workspace tab contain multiple editor tiles, including multiple views
  of the same buffer.
- Supports split creation, split resizing, tile promotion, tab combining,
  drag/drop tab ordering, and tab overflow.
- Searches and replaces across the current selection, current file, current
  workspace tab, or all open tabs.
- Provides plain-text and regex search, case-sensitive matching, whole-word
  matching, replace current, and replace-all where the active scope permits it.
- Detects common encodings and BOMs, preserves supported BOM state, tracks line
  endings, and warns before unsafe saves.
- Surfaces artifact-heavy text such as control characters, ANSI escape
  sequences, carriage-return output, and overprint patterns.
- Maintains document-local undo/redo plus a text history and transaction-log
  surface for reviewing recent edits and workspace operations.
- Restores sessions, settings, tab layout, pane layout, and open-buffer
  metadata.
- Keeps measurement-friendly Rust probe binaries in-tree while the reusable
  measurement producers and dashboard live in sibling repositories.

## Design Goals

- Keep general text editing fast and predictable, including large buffers and
  many open tabs.
- Make risky file-format decisions visible before bytes are written.
- Prefer durable local state over cloud sync, update systems, or plugin
  execution.
- Use performance, capacity, resource, and complexity measurements as normal
  development inputs.
- Keep the product useful for non-code text instead of drifting into a
  coding-first editor.

## Current Status

Scratchpad is active and usable, but still evolving. Current known gaps include
a planned command palette, narrower context-menu command coverage than a mature
editor, and no folder-wide search for unopened files.

## Screenshots

![Scratchpad search dialog](assets/Search%20Dialog.png)

![Scratchpad transaction log](assets/Transaction%20Log.png)

## Build, Test, and Run

Prerequisites:

- Rust via `rustup`
- Windows for the primary desktop target and packaging flow

Common commands:

```powershell
cargo run --release
cargo test
cargo build --release
```

Useful runtime switches:

```powershell
scratchpad.exe /help
scratchpad.exe /version
scratchpad.exe /clean "C:\notes\a.txt"
scratchpad.exe /addto:active /files:"C:\notes\a.txt","C:\notes\b.txt"
```

## Packaging and Releases

The GitHub release workflow runs formatting, clippy, tests, validates that the
tag version matches `Cargo.toml`, builds the Windows MSI installer, uploads it
as a workflow artifact, and publishes the release asset. Push a tag such as
`v0.40.0` or run the `Release` workflow manually.

## Measurement Workflow

Scratchpad treats measurement as part of the product. Measurement producers now
live in sibling lens repositories, and the local React/TypeScript dashboard lives
in the sibling `project-management-board` repository. Scratchpad keeps the Rust
probe binaries under `src/bin/` and JSON artifacts under `target/analysis/`.

Start the dashboard with:

```powershell
cd ..\project-management-board
npm run dev
```

The detailed measurement catalog lives in
[docs/measurement-tools.md](docs/measurement-tools.md).

## Repository Map

```text
src/
  main.rs                 Desktop entry point and native window setup
  lib.rs                  Public crate surface
  app/
    app_state/            Frame loop, workspace state, settings state, search state
    chrome/               Custom window chrome and caption buttons
    commands/             Command dispatch and tab/view transfer operations
    domain/               Buffers, piece-tree storage, panes, tabs, views
    services/             File IO, search, session persistence, settings persistence
    startup/              Command-line parsing and startup options
    ui/                   Editor, dialogs, search/replace, settings, tabs, scrolling
  bin/                    Profiling and capacity probe entry points
scripts/                  Rust-only helper binaries wired through Cargo
docs/                     User, design, architecture, performance, and review notes
assets/                   README and product screenshots
fonts/                    Bundled editor and control-symbol fonts
```

## Key Documentation

- [User manual](docs/user-manual.md)
- [Measurement tools](docs/measurement-tools.md)
- [Encoding review report](docs/encoding-review-report.md)
- [Project plan](PLAN.md)

## Technical Notes

- Stack: Rust 2024, `eframe`/`egui`, `egui-phosphor`, `rfd`, `serde`,
  `encoding_rs`, `chardetng`, `regex`, `smallvec`, and `sysinfo`.
- Unsafe Rust is forbidden at the crate level.
- Text storage uses a piece-tree-backed document model with undo/redo history
  and cheap snapshots for save/session flows.
- Runtime logs go under `log/`.
- Session state and `settings.toml` are stored under the Scratchpad directory in
  the OS temp location.
