# Scratchpad

Scratchpad is a Rust text editor built with `egui` / `eframe`.

It currently focuses on a custom desktop editing experience with a frameless window, shared tab management across a visible strip and overflow list, multi-pane editing, encoding-aware file IO, session restore, and runtime logging.

## Current Feature Set

- Custom frameless window chrome with caption controls
- Tab strip plus overflow list backed by one shared tab-order model
- Drag-and-drop tab reordering:
  - within the visible tab strip
  - within the overflow list
  - between the strip and the overflow list
- Multi-pane editing inside a workspace tab
- Native open, save, and save-as dialogs
- Dirty-state tracking and destructive-action confirmation
- Encoding-aware file loading and saving
- Control-character / ANSI artifact detection with cleaned and visible inspection modes
- Status bar with file path, line count, encoding, artifact status, and runtime logging toggle
- Session persistence for tabs, pane layout, active tab, zoom, wrap, and logging preference
- Runtime file logging for major editor actions

## Current Limitations

- Search is not implemented yet.
- Multi-pane editing currently supports multiple views of the same buffer inside a workspace tab; true multi-buffer workspace tabs are still future work.
- Packaging and release distribution are not set up.

## Keyboard Shortcuts

- `Ctrl + N`: new tab
- `Ctrl + O`: open file
- `Ctrl + S`: save active file
- `Ctrl + W`: close active tab
- `Ctrl + +` / `Ctrl + =`: increase editor font size
- `Ctrl + -`: decrease editor font size
- `Ctrl + 0`: toggle line numbers in the active view
- `Ctrl + Mouse Wheel`: zoom editor font size

## Build and Run

Prerequisites:

- Rust toolchain installed via `rustup`
- Windows environment

Run the app:

```bash
cargo run --release
```

Run tests:

```bash
cargo test
```

## Project Structure

```text
src/
├── main.rs
├── lib.rs
└── app/
    ├── app_state.rs
    ├── chrome.rs
    ├── commands.rs
    ├── logging.rs
    ├── shortcuts.rs
    ├── theme.rs
    ├── utils.rs
    ├── domain/
    ├── services/
    └── ui/
```

Key areas:

- `src/app/domain/`: buffers, views, pane trees, workspace tabs, tab manager
- `src/app/services/`: file IO, session persistence, file controller
- `src/app/ui/`: tab strip, overflow UI, drag helpers, editor area, dialogs, status bar

## Tech Stack

- Rust 2024 edition
- `eframe` / `egui`
- `egui-phosphor`
- `rfd`
- `serde` / `serde_json`
- `encoding_rs`, `encoding_rs_io`, `chardetng`

## Notes

- Runtime logs are written under `log/` during local runs.
- Session state is stored under the OS temp directory.
- The current plan and project status are tracked in [PLAN.md](PLAN.md).