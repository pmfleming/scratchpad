# Scratchpad

Scratchpad is a Windows-focused Rust text editor built with `egui` / `eframe`, with custom chrome, shared tabs, multi-pane workspaces, encoding-aware IO, TOML settings, and session restore.

## Features

- Shared tab order across strip, overflow, Settings, and drag/drop
- Multi-pane, multi-buffer workspace tabs with Open Here, tile promotion, workspace promotion, and drag-to-combine
- Native open/save/save-as dialogs, dirty confirmation, duplicate-path checks, encoding detection, BOM preservation
- Artifact-heavy file detection with cleaned and visible inspection modes
- Status bar with path, line count, encoding, artifact state, and logging
- Session persistence for tabs, pane layout, and view metadata
- TOML settings for font size, wrap, logging, and editor font

Gaps: search, context menus / command palette actions, and installer packaging.

## Shortcuts

- `Ctrl + N`: new tab
- `Ctrl + O`: open file
- `Ctrl + Shift + O`: open file here as tile(s)
- `Ctrl + ,`: open settings
- `Ctrl + S`: save active file
- `Ctrl + W`: close active tab
- `Ctrl + Shift + W`: close active tile
- `Ctrl + Shift + Arrow`: split active tile
- `Ctrl + +` / `Ctrl + =`, `Ctrl + -`, `Ctrl + Mouse Wheel`: zoom editor font
- `Ctrl + 0`: toggle line numbers for the current workspace tab

## Build and Run

Prerequisites: Rust via `rustup` on Windows.

```bash
cargo run --release
cargo test
powershell -ExecutionPolicy Bypass -File scripts\ci.ps1
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

Release flow: push a tag like `v0.2.0` or run the `Release` workflow manually. GitHub Actions builds, checks, packages, and attaches the Windows `.zip` and checksum.

## Measurement Tools

- `scripts/hotspots.py`: complexity / maintainability JSON
- `scripts/slowspots.py`: benchmark / performance JSON
- `scripts/map.py`: architecture JSON with hotspot and slowspot data

All three scripts support `--mode cli`, `--mode analysis`, and `--mode visibility`.

Example:

```bash
.venv\Scripts\python.exe scripts\hotspots.py --mode cli --paths src --scope all
.venv\Scripts\python.exe scripts\hotspots.py --mode visibility --paths src
.venv\Scripts\python.exe scripts\slowspots.py --mode analysis --skip-bench --output target/analysis/slowspots.json
.venv\Scripts\python.exe scripts\map.py --mode visibility
.venv\Scripts\python.exe scripts\map.py --refresh --mode visibility
```

Open the static viewer:

```bash
.venv\Scripts\python.exe -m http.server 8000
```

Browse to `http://localhost:8000/viewer/`. It reads `target/analysis/` and supports file inputs.

## Project Structure

```text
src/
├── main.rs
├── lib.rs
└── app/
    ├── app_state.rs
    ├── chrome/
    ├── commands.rs
    ├── domain/
    ├── services/
    └── ui/
```

Key areas:

- `src/app/domain/`: buffers, views, panes, tabs, tab manager
- `src/app/services/`: file IO, session persistence, settings store
- `src/app/ui/`: tab strip, overflow, drag helpers, editor area, dialogs, status bar

## Notes

- Stack: Rust 2024, `eframe` / `egui`, `egui-phosphor`, `rfd`, `serde`, `encoding_rs`
- Runtime logs go under `log/`.
- Session state and `settings.toml` use the OS temp directory.
- The current plan and project status are tracked in [PLAN.md](PLAN.md).
