# Scratchpad

Scratchpad is a local-first plain-text workspace for Windows and Linux. It is
built for notes, logs, reports, terminal output, encoded files, and temporary
text work that needs more structure and recovery than a basic editor provides.

Scratchpad is intentionally not an IDE: there are no language servers, plugin
runtime, project index, or cloud account. The focus is fast text editing,
resilient session restore, explicit file-format handling, and flexible
multi-file workspaces.

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

Linux uses eframe's wgpu renderer by default. Set
`SCRATCHPAD_RENDERER=glow` to diagnose a driver or compositor problem with the
OpenGL renderer.

The development shell runs `scripts/trim-target.sh` when entered. It removes
Cargo's dev profile only when `target/` exceeds 10 GiB, preserving release and
analysis output. Use `--dry-run` to inspect it or
`SCRATCHPAD_TARGET_MAX_GIB=0` to disable automatic trimming.

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

Start with [the configuration guide](docs/configuration.md). It documents the
TOML sections, shortcut syntax, platform overrides, runtime environment
variables, and Home Manager integration. A complete example is available at
[`docs/settings.toml`](docs/settings.toml).

For compositor setup, see the
[NixOS/Hyprland notes](docs/hyprland-nixos-configuration.md).

## Build and Package

```sh
cargo build --release --locked
./scripts/package-linux-release.sh 0.4.1
nix build .#scratchpad
nix build .#scratchpad-hyprland
```

Windows installer packaging:

```powershell
dotnet tool restore
cargo build --release --locked
.\scripts\package-windows-installer.ps1 -Version 0.4.1
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

Scratchpad uses Rust 2024 and `eframe`/`egui` 0.36.1. Unsafe Rust is forbidden
at the crate level. Generated performance and capacity artifacts are written to
`target/analysis/`; the measurement boundary is described in
[docs/measurement-tools.md](docs/measurement-tools.md).

Scratchpad is available under the [MIT License](LICENSE).
