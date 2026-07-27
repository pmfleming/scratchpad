# Scratchpad

Scratchpad is a Windows-first plain-text editor written in Rust. It is built as
a safer, more capable Notepad replacement for everyday text work: notes, logs,
exports, terminal output, reports, encoded files, and temporary scratch work.

The app is deliberately not an IDE. It focuses on fast local editing, resilient
session restore, visible file-format risk, and multi-file workspaces without
language servers, plugin execution, or cloud sync.

Windows remains the primary desktop and release target. Scratchpad also builds
and runs on Linux, with first-class NixOS/Hyprland packaging and a Hyprland
platform profile for compositor-managed window behavior.

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
  tab placement, status bar visibility, undo memory budgets, platform profile,
  and shortcut overrides.
- Supports command-line startup switches for clean launches, session restore,
  opening files, and adding files into an existing workspace.
- Supports Windows, generic Linux, and Hyprland platform profiles so native
  window decorations, app-rendered caption buttons, drag regions, and resize
  behavior can match the current desktop environment.

## Design Principles

- Keep everyday text editing fast, predictable, and local.
- Show risky encoding, newline, and disk-state decisions before bytes are
  written.
- Treat restored state as important user data, especially unsaved work.
- Keep the product centered on plain text instead of drifting into a coding
  workflow.
- Use profiling, capacity probes, benchmarks, and CI as regular development
  inputs.
- Let the operating system or compositor own global window-management shortcuts;
  Scratchpad owns only in-app actions while focused.

## Build, Test, and Run

Prerequisites:

- Rust 1.95 via the pinned `rust-toolchain.toml`, or the provided Nix development shell
- Windows for the primary app target and MSI release flow
- Linux desktop libraries for Linux builds (`gtk3`, Wayland/X11, fontconfig,
  OpenGL/Vulkan loader, and related `egui`/`winit` runtime dependencies)
- .NET local tools when packaging the Windows MSI

Common development commands:

```powershell
cargo run --release
cargo test --all-features
cargo hack check --feature-powerset
cargo clippy --lib --all-features -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo audit
cargo build --release
```

Nix development and Linux runs:

```sh
nix develop
cargo check
nix run .#scratchpad
nix run .#scratchpad-hyprland
```

Entering the Nix development shell automatically runs
`scripts/trim-target.sh`. If `target/` is larger than 10 GiB, the script cleans
only Cargo's dev profile, preserving release builds and `target/analysis/`.
Run it directly to trim without entering the shell, use `--dry-run` to inspect
the command, set `SCRATCHPAD_TARGET_MAX_GIB` to change the limit, or set the
limit to `0` to disable automatic trimming.

The Hyprland wrapper sets `WINIT_UNIX_BACKEND=wayland` and
`SCRATCHPAD_PLATFORM_PROFILE=hyprland` so Scratchpad hides app-rendered caption
buttons and leaves compositor-level window management to Hyprland.

Linux defaults to eframe's wgpu renderer on egui 0.35+. Set
`SCRATCHPAD_RENDERER=glow` to force the OpenGL renderer when diagnosing driver
or compositor issues.

Optional egui inspection / MCP support is available for UI automation and
accessibility-tree debugging:

```sh
cargo run --features inspection
EGUI_INSPECTION=1 cargo run --features inspection
```

With `EGUI_INSPECTION=1`, eframe listens on egui's inspection port so tools such
as `egui_mcp` can inspect and drive the running app.

Useful runtime switches:

```powershell
scratchpad.exe /help
scratchpad.exe /version
scratchpad.exe /clean "C:\notes\a.txt"
scratchpad.exe /here "C:\notes\a.txt"
scratchpad.exe /addto:active /files:"C:\notes\a.txt","C:\notes\b.txt"
scratchpad.exe /addto:index:2 "C:\notes\c.txt"
```

## Configuration

Scratchpad stores user-editable settings in `settings.toml` under the platform
settings root (`%APPDATA%\Scratchpad` on Windows and
`$XDG_CONFIG_HOME/scratchpad` or `~/.config/scratchpad` on Linux). The platform
profile defaults to automatic detection but can be pinned explicitly:

```toml
[platform]
profile = "hyprland" # auto, windows, linux_generic, or hyprland
```

Indentation behavior is also available from Settings → Editing and can be
configured directly:

```toml
[editor]
indentation_style = "spaces" # spaces or tab_character
editor_tab_width = 4          # width used by both indentation styles
tab_display = "tablines"     # hidden, character, or tablines
```

Top-level in-app shortcuts can be overridden without changing compositor/global
shortcuts. This example uses the Hyprland tile bindings (generic Windows/Linux
tile defaults are listed below):

```toml
[shortcuts]
open_file = "ctrl+o"
save_file = "ctrl+s"
close_tab = "ctrl+w"
toggle_tab_list = "ctrl+alt+b"
split_tile = "alt+enter"
split_left = "alt+ctrl+left"
split_right = "alt+ctrl+right"
split_up = "alt+ctrl+up"
split_down = "alt+ctrl+down"
move_tile_left = "alt+left"
move_tile_right = "alt+right"
move_tile_up = "alt+up"
move_tile_down = "alt+down"
resize_tile_left = "alt+shift+left"
resize_tile_right = "alt+shift+right"
resize_tile_up = "alt+shift+up"
resize_tile_down = "alt+shift+down"
```

Under the Hyprland profile, tiling shortcuts all include an `alt` leader — the
in-app stand-in for the compositor's `super` key, which Hyprland reserves
globally. The base key mirrors the equivalent Hyprland bind, so muscle memory
carries over: where Hyprland uses `super+enter` to spawn a tile, Scratchpad uses
`alt+enter` to split; `super+arrow` ↔ `alt+arrow` to move a tile, and so on.
Generic Windows/Linux defaults use `ctrl+alt+enter` to split,
`ctrl+shift+arrow` for directional splits, `ctrl+alt+arrow` to resize, and
`ctrl+alt+shift+arrow` to move. This keeps editor `ctrl+arrow` and `alt+arrow`
word navigation available. Tile movement
follows Hyprland's dwindle model: the active tile is removed, its old split is
collapsed, and it is reinserted beside the tile nearest the requested edge.
Under the Hyprland profile, active and inactive tile borders also use the
`general:col.*_border` colors and border width reported by the compositor at
startup, when available. The editor reserves only bindings that are active in
the resolved app keymap; unassigned `alt` combinations remain available to text
editing.

Shortcut strings are case-insensitive. Supported modifiers are `ctrl`, `shift`,
`alt`, and `command`/`super`/`win`. Multiple bindings can be separated with
commas, so you can add focused-app `super` bindings such as
`split_tile = "alt+enter, super+enter"` if those keys are not reserved by the
compositor. Tooltips resolve from the active OS profile and valid user
overrides, so they always show the binding currently in use. The complete list
of action names and defaults is in `docs/user-manual.md` under **Shortcut
Reference**.

## Packaging and Release

The release workflow builds Windows and Linux artifacts in parallel, then
publishes them together only after both jobs pass. It checks formatting, clippy,
tests, Cargo/Nix version consistency, and the Nix flake. Windows produces a
signed MSI when signing is configured; Linux produces an x86_64 archive with
generic and Hyprland launchers, a desktop entry, an icon, and a checksum.

Release tags use the `vX.Y.Z` format, matching the package versions in both
`Cargo.toml` and `flake.nix`.

Local Windows installer packaging:

```powershell
dotnet tool restore
cargo build --release --locked
.\scripts\package-windows-installer.ps1 -Version 0.4.1
```

Linux/Nix packaging entry points:

```sh
cargo build --release --locked
./scripts/package-linux-release.sh 0.4.1
nix build .#scratchpad
nix build .#scratchpad-hyprland
nix run github:pmfleming/scratchpad#scratchpad-hyprland
```

The flake also exposes `homeManagerModules.default`, which can install the
regular package or the Hyprland wrapper, write `~/.config/scratchpad/settings.toml`,
and generate Hyprland autostart, binds, and window rules:

```nix
programs.scratchpad = {
  enable = true;
  profile = "hyprland";
  hyprland = {
    autoStart = true;
    workspace = "5";
  };
};
```

This starts Scratchpad tiled on regular workspace `5` at login without changing
the active workspace. A Waybar icon assigned to workspace 5 remains the direct
entry point for Scratchpad. For a non-Nix installation, install the GitHub
release archive and add the equivalent Hyprland settings:

```ini
exec-once = scratchpad-hyprland
windowrule = match:class ^(scratchpad)$, workspace 5 silent
```

The release workflow publishes `scratchpad-vX.Y.Z-linux-x86_64.tar.gz` and its
SHA-256 checksum. See
[the Hyprland configuration notes](docs/hyprland-nixos-configuration.md) for the
full Home Manager module and plain compositor configuration.

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
    platform.rs           Platform profile detection and desktop capabilities
    platform_file.rs      Platform-specific file dialogs and reveal/open actions
  bin/                    Profiling, resource, frame, and capacity probes
crates/
  linux_file_watch/       Linux file-change watching helper crate
  windows_file_watch/     Windows file-change watching helper crate
benches/                  Criterion benchmark targets
scripts/                  Release and analysis helper scripts
packaging/
  nix/                    Home Manager module and Linux packaging helpers
  windows/                Windows installer definition and support files
.github/workflows/        CI and release automation
docs/                     User manual, architecture notes, and reviews
assets/                   Project artwork
fonts/                    Bundled editor and control-symbol fonts
```

## Key Documentation

- [User manual](docs/user-manual.md)
- [Measurement tools](docs/measurement-tools.md)
- [NixOS/Hyprland configuration notes](docs/hyprland-nixos-configuration.md)

## Technical Notes

- Stack: Rust 2024, `eframe`/`egui`, `egui-phosphor`, `rfd`, `serde`,
  `encoding_rs`, `chardetng`, `regex`, `smallvec`, `sysinfo`, and a local
  Windows file-watch crate.
- Unsafe Rust is forbidden at the crate level.
- Text storage uses a piece-tree-backed document model with undo/redo history
  and cheap snapshots for save and session flows.
- Runtime logs are written under `log/`.
- Settings use the platform settings root; session state uses `%LOCALAPPDATA%\Scratchpad`
  on Windows and `$XDG_STATE_HOME/scratchpad` or `~/.local/state/scratchpad` on
  Linux, with the legacy temp directory used as a fallback/migration source.
