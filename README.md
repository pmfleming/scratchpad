# Scratchpad

Scratchpad is a local-first plain-text workspace for Windows and Linux. It is
built for notes, logs, reports, terminal output, encoded files, and temporary
text work that needs more structure and recovery than a basic editor provides.

Scratchpad is intentionally not an IDE: there are no language servers, plugin
runtime, project index, or cloud account. The focus is fast text editing,
resilient session restore, explicit file-format handling, and flexible
multi-file workspaces.

## How Fast Is Scratchpad?

The archived baseline below reported seven passing performance promises on a
16-thread AMD Ryzen 7 PRO 8840HS. These are historical measurements, not current
compliance guarantees: fresh [session-migration runs](docs/turso-migration.md)
identify existing failures and coverage risks. Timings are not cross-platform guarantees.

| Promise | Result | Measured workload |
| --- | --- | --- |
| Large Files | Pass | 1 GiB first-visible prefix in 0.57 ms; background indexing in 1.69 s |
| Many Files | Pass | 10,000-file first-visible load in 64.6 ms; background completion in 697 ms |
| Search | Pass | 1 GiB prepared search in about 160 ms; 10,000 targets in about 2.4 ms |
| Many Tabs | Pass | 10,000-tab reorder in about 0.18 ms; prepared capacity cycle in about 14 ms |
| Many Views | Pass | 1,000-view prepared operation in under 0.1 ms |
| Large Text Mutation | Pass | 128 MiB paste in about 136 ms |
| Session Persistence & Restore | Pass | 10,000-tab startup-visible restore in 14.9 ms; steady-state persistence in 66.8 ms |

See the [performance baseline](docs/performance-lens-baseline-2026-07-25.md)
for methodology, budgets, and additional results.

## Highlights

- Workspace tabs containing one editor or a tiled set of file views.
- Multiple views of the same document, directional splits, resizing, tile
  movement, promotion, tab combining, and tab overflow.
- Search and replace across a selection, file, workspace, or every open tab,
  with plain text, regular expressions, case, and whole-word modes.
- Encoding and BOM detection, mixed newline awareness, save compatibility
  checks, and external-file conflict handling.
- Visibility for control characters, ANSI escapes, carriage-return output, and
  other text artifacts.
- Per-document undo/redo, a navigable text-history view, and configurable
  history memory budgets.
- Session recovery for open files, unsaved text, workspace layouts, settings,
  and file metadata.
- Platform profiles for Windows, generic Linux desktops, and Hyprland.
- Single-instance forwarding: later launches activate the existing window and
  send their incoming files to it.

See the [user manual](docs/user-manual.md) for workflows and shortcuts.

### Pane zoom

Each editor pane has independent zoom, from 25% to 400%. **Ctrl + + / −** adjusts
it in 10-percentage-point steps; **Ctrl + 0** resets to 100% of the configured
editor font size. **Ctrl + 9** toggles line numbers. Mouse-wheel zoom affects the
pane under the pointer; keyboard zoom affects the active pane.

The status bar shows the active pane's percentage without any click action. The
editor context menu has one **Zoom · 100% [−] [+] [↺]** row and stays open while
adjusting. New splits inherit their source pane's zoom and then zoom independently;
new tabs start at 100%. Pane zoom is preserved in sessions and closed-tab recovery.
Changing the font size in Settings changes the baseline, not the pane percentages.

## Platforms

Scratchpad supports:

- Windows, including the MSI packaging flow and app-rendered window chrome.
- Linux under Wayland or X11, with native desktop integration.
- NixOS/Home Manager and a Hyprland profile that leaves global window
  management to the compositor.

Windows and Linux release artifacts are built from the same application. The
Hyprland launcher selects Wayland and the Hyprland platform profile; it is not a
separate editor.

## Run and Develop

The repository pins Rust 1.95 in `rust-toolchain.toml`. A Nix development shell
is also provided.

```sh
cargo run --release
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

On NixOS/Linux:

```sh
nix develop
nix run .#scratchpad
nix run .#scratchpad-hyprland
```

Scratchpad uses eframe's WGPU renderer by default. The local eframe patch prevents
Wayland busy-polling while waiting for compositor redraw callbacks. On Linux,
set `SCRATCHPAD_RENDERER=glow` to use the fallback OpenGL renderer; its presentation
path remains nonblocking and compositor-paced. See the
[WGPU recovery investigation](docs/wgpu-recovery-investigation.md) and
[Wayland startup validation](docs/wayland-startup.md) for the fixes and native probe.

The host's shutdown service applies [a 4 GiB Cargo cache budget](BUILD-CACHE.md).
Development-shell entry does not trim caches. Run
`python3 scripts/trim-build-cache.py --dry-run` for a manual preview. Incremental
and coverage caches go first; debug/release artifacts may also be removed.
Reports and review checkouts are preserved. `scripts/trim-target.sh` and
`SCRATCHPAD_TARGET_MAX_GIB` remain compatible manual-command aliases.

UI inspection support is optional:

```sh
cargo run --features inspection
EGUI_INSPECTION=1 cargo run --features inspection
```

## Command Line

Scratchpad accepts file paths and Windows-style startup switches on both
supported platforms:

```text
scratchpad [switches] [files...]

/clean                 skip session restore
/here                  open incoming files in the active workspace
/addto:active          add files to the active workspace
/addto:index:N         add files to restored workspace N (1-based)
/files:"a","b"         pass a comma-delimited file list
/help, /?              show help
/version               show the version
```

A second invocation forwards files and workspace targets to the running
instance. An invocation without files activates its window. Close the running
instance before using `/clean`.

## Configuration

Settings are available in the app with `Ctrl + ,` and are stored as TOML:

| Platform | Settings | Session state and diagnostics |
| --- | --- | --- |
| Windows | `%APPDATA%\Scratchpad\settings.toml` | `%LOCALAPPDATA%\Scratchpad` |
| Linux | `$XDG_CONFIG_HOME/scratchpad/settings.toml` or `~/.config/scratchpad/settings.toml` | `$XDG_STATE_HOME/scratchpad` or `~/.local/state/scratchpad` |

Sessions use a local embedded Turso `session.db`; settings remain TOML and user
files keep their existing save paths. Schemas 6–8 upgrade transactionally to
schema 9. Unsupported session schemas automatically start a fresh database,
discarding previous session/undo/recovery data, not saved text files or settings.
Upgrades and resets are reported in the `[]` status log. See [migration and recovery export](docs/turso-migration.md)
before backing up, recovering, or downgrading a session database.

Closing during startup restore leaves the previous session intact. If startup
restore fails, session saving stays disabled for that run to protect the saved
workspace; save new edits to files before closing. A protected/unavailable session
store does not prevent exit. Retryable save errors still keep the window open,
without retrying automatic last-tab shutdown every frame.

**Scrubber** appears when you undo or redo (`Ctrl+Z` / `Ctrl+Y`) and follows your
position through the active file's retained edits. It stays visible until you
resume editing or press Escape. Switch freely between the shortcuts and dragging
the timeline to update the file live, with a subtle highlight on each change;
Left/Right and Home/End work while its track has focus. **History…** (`Ctrl+H`) shows the current file, all files,
or a chosen file. Hover a row to preview the state **after** that change; click to
confirm it as a new undoable state. Escape or clicking outside discards the preview.
Hover near the list's top or bottom to scroll through more states. Up/Down preview
in on-screen order while the list has focus. The opening state remains outlined
while the preview row is highlighted. A subtle tint marks the
affected line; neither tool saves files. Diamonds mark reachable snapshots;
older snapshots remain selectable, with **Return to latest** restoring the preserved
live position. Clearing undo history requires confirmation and keeps saved snapshots. **Recently Closed** combines archived
tabs and disk-only file entries; normal reopening restores archives automatically
and separates externally changed files from recovered unsaved text.
Configure undo/snapshot/archive budgets, expiry and reopening in **Settings >
History & Recovery**, alongside measured data-directory disk usage.
See [retention, storage limits, and schema changes](SESSION-RETENTION.md).

Start with [the configuration guide](docs/configuration.md). It documents the
TOML sections, shortcut syntax, platform overrides, runtime environment
variables, and Home Manager integration. A complete example is available at
[`examples/settings.toml`](examples/settings.toml).
Saved `settings.toml` files list every configurable shortcut under `[shortcuts]`:
existing overrides remain active, and platform defaults are commented out for easy
customization. Fixed editor, search, and history shortcuts follow as a reference.

For compositor setup, see the
[NixOS/Hyprland notes](docs/hyprland-nixos-configuration.md).

## Build and Package

```sh
cargo build --release --locked
./scripts/package-linux-release.sh 0.4.2
nix build .#scratchpad
nix build .#scratchpad-hyprland
```

Windows installer packaging:

```powershell
dotnet tool restore
cargo build --release --locked
.\scripts\package-windows-installer.ps1 -Version 0.4.2
```

Release tags use `vX.Y.Z` and must match the package versions in `Cargo.toml`
and `flake.nix`.

The flake exports `homeManagerModules.default`. A minimal setup is:

```nix
programs.scratchpad = {
  enable = true;
  profile = "auto";
};
```

Set `profile = "hyprland"` and configure `hyprland.autoStart`, `workspace`, or
`enableBinds` when compositor integration is wanted.

## Project Layout

```text
src/app/domain/       buffers, piece-tree text, panes, tabs, and views
src/app/services/     file IO, search, settings, sessions, and background work
src/app/ui/           editor, tabs, dialogs, settings, and search/replace
src/app/startup/      command-line parsing and startup behavior
src/bin/              capacity, frame, resource, and profiling tools
crates/                platform file-watch and icon helper crates
packaging/             Nix/Home Manager and Windows installer definitions
benches/               Criterion benchmarks
docs/                  user, configuration, architecture, and review documents
```

## Documentation

- [User manual](docs/user-manual.md)
- [Configuration guide](docs/configuration.md)
- [NixOS/Hyprland configuration](docs/hyprland-nixos-configuration.md)
- [Measurement tools](docs/measurement-tools.md)

Scratchpad uses Rust 2024 and upstream `eframe`/`egui` 0.37.0.
See [the upgrade validation notes](EGUI-UPGRADE.md) for backend coverage and limitations. Unsafe Rust is forbidden
at the crate level. Generated performance and capacity artifacts are written to
`target/analysis/`; the measurement boundary is described in
[docs/measurement-tools.md](docs/measurement-tools.md).

Scratchpad is available under the [MIT License](LICENSE).
