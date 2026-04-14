# Scratchpad

Scratchpad is a Windows-focused Rust text editor built with `egui` / `eframe`, with custom chrome, shared tab/workspace state, tiled editor layouts, encoding-aware file handling, TOML settings, transaction history, and session restore.

## Features

- Shared tab order across the tab strip, overflow list, Settings surface, and drag/drop flows
- Multi-pane workspace tabs with split creation, split resizing, Open Here, tile promotion, and tab combining
- Native open/save/save-as flows with dirty confirmation, duplicate-path checks, encoding detection, and BOM preservation
- Artifact-heavy text detection with cleaned and raw inspection modes
- Document-local undo/redo plus a separate transaction log for text edits and workspace operations
- Status bar with path, line count, encoding, artifact state, runtime logging, and transaction-log access
- TOML-backed settings plus session persistence for tabs, views, pane layout, and metadata

Gaps: search, context menus / command palette actions, and installer packaging.

## Docs

- [User manual](docs/user-manual.md)
- [Measurement tools](docs/measurement-tools.md)
- [Project plan](PLAN.md)

## Build and Test

Prerequisites: Rust via `rustup` on Windows. Some optional analysis scripts also expect the local `.venv` Python environment.

```bash
cargo run --release
cargo test
powershell -ExecutionPolicy Bypass -File scripts\ci.ps1
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

Release flow: push a tag like `v0.2.0` or run the `Release` workflow manually. GitHub Actions builds, checks, packages, and attaches the Windows `.zip` and checksum.

## Project Structure

```text
src/
├── main.rs
├── lib.rs
└── app/
    ├── app_state.rs
    ├── transactions.rs
    ├── chrome/
    ├── startup/
    ├── commands.rs
    ├── domain/
    ├── services/
    └── ui/
```

Key areas:

- `src/app/app_state/`: settings state, display-tab ordering, startup state, and settings TOML refresh logic
- `src/app/chrome/`: custom caption buttons, resize logic, and top-level chrome behavior
- `src/app/commands/`: command dispatch and tab/view transfer operations
- `src/app/domain/`: buffers, views, panes, tabs, layout/promotion helpers, and shared tab manager
- `src/app/services/`: file IO, file controller flows, session persistence, settings persistence, and store helpers
- `src/app/startup/`: CLI/startup parsing
- `src/app/ui/`: dialogs, editor area/content, settings pages, status bar, tab strip, overflow, tile header, and tab drag state
- `viewer/`: static analysis viewer
- `tests/`: integration tests for app, buffers, files, session storage, startup, tab manager, and tab behavior

## Notes

- Stack: Rust 2024, `eframe` / `egui`, `egui-phosphor`, `rfd`, `serde`, `encoding_rs`
- Runtime logs go under `log/`.
- Session state and `settings.toml` use the OS temp directory.
- End-user usage details live in the [user manual](docs/user-manual.md).
- Analysis workflow details live in [measurement-tools.md](docs/measurement-tools.md).
