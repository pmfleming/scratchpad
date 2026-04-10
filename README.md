# Scratchpad

Scratchpad is a Rust text editor built with `egui` / `eframe`.

It currently focuses on a custom desktop editing experience with a frameless window, shared tab management across a visible strip and overflow list, multi-pane editing, encoding-aware file IO, TOML-backed user settings, session restore, and runtime logging.

## Current Feature Set

- Custom frameless window chrome with caption controls
- Tab strip plus overflow list backed by one shared tab-order model
- Settings page available as a real tab:
  - open via `Ctrl + ,`
  - reorderable in the strip and overflow list
  - closeable like any other tab
- Drag-and-drop tab reordering:
  - within the visible tab strip
  - within the overflow list
  - between the strip and the overflow list
- Multi-pane editing inside a workspace tab
- Multi-buffer workspace tabs with per-file tile groups
- Drag-to-combine across top-level tabs
- Open Here for loading one or more files into the current workspace tab as tiles
- Equal-share rebalancing when Open Here adds new files to a workspace
- Tile promotion for extracting one file's tiles into a new top-level tab
- Workspace promotion for exploding a multi-file workspace tab into one top-level tab per file
- Native open, save, and save-as dialogs
- Dirty-state tracking and destructive-action confirmation
- Encoding-aware file loading and saving
- Control-character / ANSI artifact detection with cleaned and visible inspection modes
- Status bar with file path, line count, encoding, artifact status, and runtime logging toggle
- TOML-backed settings persistence for font, wrap, logging, and editor font selection
- Session persistence for tabs, pane layout, active tab, and view/session metadata
- Runtime file logging for major editor actions

## Current Limitations

- Search is not implemented yet.
- There is no context menu or command palette layer yet for tile/tab actions; promotion and combine actions are currently button- and drag-driven.
- Installer packaging is not set up yet; release distribution is currently a Windows `.zip` archive.

## Keyboard Shortcuts

- `Ctrl + N`: new tab
- `Ctrl + O`: open file
- `Ctrl + Shift + O`: open file here in the current tab as new tile(s)
- `Ctrl + ,`: open settings
- `Ctrl + S`: save active file
- `Ctrl + W`: close active tab
- `Ctrl + +` / `Ctrl + =`: increase editor font size
- `Ctrl + -`: decrease editor font size
- `Ctrl + 0`: toggle line numbers for the current workspace tab
- `Ctrl + Shift + W`: close active tile
- `Ctrl + Shift + Up`: split active tile upward
- `Ctrl + Shift + Down`: split active tile downward
- `Ctrl + Shift + Left`: split active tile left
- `Ctrl + Shift + Right`: split active tile right
- `Ctrl + Mouse Wheel`: zoom editor font size

## Workspace Behaviors

- Dragging one top-level tab onto another combines the source workspace into the target workspace.
- `Open Here` loads new files into the current workspace tab instead of creating new top-level tabs.
- Tile promotion extracts the current file's tile group into a new top-level tab.
- Workspace promotion appears on tabs with 3 or more files and splits that workspace into one top-level tab per file.

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

Run the standardized code health checks:

```bash
powershell -ExecutionPolicy Bypass -File scripts\ci.ps1
```

Build a local Windows release archive:

```bash
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

Release flow:

- Push a tag like `v0.1.0`, or run the `Release` workflow manually with version `0.1.0`.
- GitHub Actions builds `scratchpad.exe`, runs format, clippy, and tests, packages a Windows x64 `.zip`, writes a `.sha256` checksum, and attaches both to a GitHub Release.
- The archive includes `scratchpad.exe`, `README.md`, and `register-open-with.ps1`.

Direct measurement tools:

- `scripts/hotspots.py`: standardized complexity and maintainability measurement; emits JSON
- `scripts/slowspots.py`: standardized benchmark/performance measurement; emits JSON
- `scripts/map.py`: standardized architecture/interrelatedness mapping enriched with hotspot and slowspot data; emits JSON

Example JSON artifact generation:

```bash
.venv\Scripts\python.exe scripts\hotspots.py --paths src --scope all --output hotspots.json
.venv\Scripts\python.exe scripts\slowspots.py --skip-bench --output slowspots.json
.venv\Scripts\python.exe scripts\map.py --output map.json
```

The JSON outputs are the intended data contract for a separate viewer, such as a Java/React tabbed interface with one tab each for hotspots, slowspots, and the architecture map.

Open the bundled static data viewer:

```bash
.venv\Scripts\python.exe -m http.server 8000
```

Then browse to `http://localhost:8000/viewer/`. The viewer has tabs for hotspots, slowspots, and the architecture map, and reads the default JSON artifacts from `target/analysis/`. If the browser blocks local fetches or the artifacts live elsewhere, use the file inputs at the top of the viewer.

## Project Structure

```text
src/
├── main.rs
├── lib.rs
└── app/
    ├── app_state.rs
    ├── chrome/
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
- Session state and `settings.toml` are currently stored under the OS temp directory.
- `scripts/ci.ps1` is the standard local/CI entry point and runs formatting, linting, tests, hotspot review, and slowspot review.
- The current plan and project status are tracked in [PLAN.md](PLAN.md).
