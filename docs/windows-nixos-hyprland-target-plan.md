# Windows + NixOS/Hyprland Target Plan

Date: 2026-06-16

## Goal

Update Scratchpad so it can be built and used cleanly on both primary environments:

- Windows
- NixOS with Hyprland

Hyprland-specific behavior must not break or regress the existing Windows version.

## Core idea

Do not treat `target_os = "linux"` as equivalent to Hyprland.

Hyprland is a Linux/Wayland runtime environment, not a Rust compile target. The app should support:

- a Windows profile
- a generic Linux profile
- a Hyprland profile

The Hyprland profile can be selected by runtime detection, config, or a wrapped Nix package.

## Platform/profile layer

Add a small platform abstraction, for example:

```text
src/app/platform/
```

Conceptual model:

```text
PlatformProfile:
  Windows
  LinuxGeneric
  Hyprland
```

Profile detection can use:

- `cfg(target_os = "windows")`
- `cfg(target_os = "linux")`
- Hyprland environment variables, such as:
  - `HYPRLAND_INSTANCE_SIGNATURE`
  - `XDG_CURRENT_DESKTOP=Hyprland`
- optional explicit config override:

```toml
[platform]
profile = "hyprland"
```

The profile should expose capabilities rather than spreading platform checks around the UI:

```text
show_window_caption_buttons: bool
use_native_decorations: bool
allow_app_resize_grips: bool
allow_app_drag_regions: bool
global_shortcuts_managed_externally: bool
```

## Window chrome behavior

Relevant current areas:

- `src/main.rs`
  - `ViewportBuilder`
  - `with_decorations(...)`
- `src/app/chrome/buttons.rs`
  - app-rendered minimize/maximize/close buttons
- `src/app/ui/tab_strip/actions.rs`
- `src/app/ui/tab_strip/actions/vertical.rs`
- `src/app/ui/tab_strip/layout.rs`

For Hyprland:

```text
show_window_caption_buttons = false
```

Expected result:

- no minimize button
- no maximize/restore button
- no close button
- no reserved blank space where caption buttons used to be
- optional disabling of app resize grips if Hyprland/window manager should fully own resizing

For Windows:

- preserve the current app chrome behavior
- preserve existing window controls
- preserve existing drag/resize behavior

## Shortcut/keybinding design

Current shortcut handling is mostly hard-coded in:

```text
src/app/shortcuts.rs
```

Refactor toward an action-driven keymap:

```text
Action:
  OpenFile
  SaveFile
  CloseTab
  OpenSearch
  OpenReplace
  SplitLeft
  SplitRight
  SplitUp
  SplitDown
  PromoteTile
  CloseTile
  ToggleStatusBar
  ...
```

And map actions to one or more bindings:

```text
ShortcutMap:
  Action -> Vec<KeyBinding>
```

Default maps:

- Windows default keymap: keep current bindings
- Linux default keymap: usually same as Windows unless a Linux-specific reason exists
- Hyprland default keymap: avoid conflicting with compositor-owned global shortcuts

Config override example:

```toml
[shortcuts]
open_file = "ctrl+o"
save_file = "ctrl+s"
close_tab = "ctrl+w"
split_left = "ctrl+shift+left"
split_right = "ctrl+shift+right"
split_up = "ctrl+shift+up"
split_down = "ctrl+shift+down"
```

All new config fields should use serde defaults so old Windows configs continue to load.

## Hyprland global shortcuts

Do not try to make Scratchpad own global shortcuts on Hyprland.

Hyprland should own global shortcuts through its config/Nix config. Scratchpad should only handle in-app shortcuts when the app has focus.

Example Hyprland concept:

```nix
wayland.windowManager.hyprland.settings.bind = [
  "$mainMod, S, exec, scratchpad"
  "$mainMod SHIFT, S, togglespecialworkspace, scratchpad"
];
```

Potential Nix/Home Manager shape:

```nix
programs.scratchpad = {
  enable = true;
  profile = "hyprland";

  shortcuts = {
    openFile = "ctrl+o";
    saveFile = "ctrl+s";
  };

  hyprland = {
    enableBinds = true;
    toggleBind = "$mainMod, S";
  };
};
```

This keeps compositor-level behavior outside the app and follows normal Hyprland principles.

## Config and storage paths

Current app/session stores should be reviewed for stable per-OS paths.

Desired direction:

```text
Windows:
  %APPDATA%/Scratchpad/settings.toml
  %LOCALAPPDATA%/Scratchpad/session.json

Linux/NixOS:
  $XDG_CONFIG_HOME/scratchpad/settings.toml
  $XDG_STATE_HOME/scratchpad/session.json
```

If changing paths, add migration or fallback behavior so existing Windows users are not broken.

## Nix/flake work

Update `flake.nix` to expose useful Linux outputs, such as:

```text
packages.x86_64-linux.default
packages.x86_64-linux.scratchpad-hyprland
apps.x86_64-linux.default
devShells.x86_64-linux.default
```

The Hyprland package/wrapper can set Wayland-oriented environment defaults if needed, for example:

```text
WINIT_UNIX_BACKEND=wayland
```

Also add documentation or scripts for checking both build targets:

```text
cargo check --target x86_64-unknown-linux-gnu
cargo check --target x86_64-pc-windows-msvc
```

or `x86_64-pc-windows-gnu` when cross-building from Nix/Linux.

## Compatibility rules

To keep Windows safe:

- keep Hyprland behavior behind runtime profile/capabilities
- do not replace Windows defaults
- do not import Linux/Hyprland-specific code unconditionally
- use `cfg(target_os = "linux")` only for Linux-only implementation details
- keep old settings TOML valid
- keep existing Windows shortcut defaults unchanged
- add tests for platform/profile decisions

## Suggested implementation order

1. Add the `PlatformProfile` and capability layer.
2. Route native decorations and app caption-button visibility through that layer.
3. Update tab-strip layout so hidden caption buttons reserve zero width.
4. Hide vertical-mode caption buttons for Hyprland.
5. Refactor shortcut handling into action/keybinding lookup.
6. Add config-backed shortcut overrides with defaults.
7. Add Hyprland/Nix documentation or module snippets.
8. Add Linux/Windows build checks.
9. Add regression tests for Windows and Hyprland behavior.

## Implementation status

Started 2026-06-16:

- Added `PlatformProfile` and `PlatformCapabilities`.
- Added `platform.profile` to `settings.toml`, defaulting to `auto`.
- Added Hyprland auto-detection from `HYPRLAND_INSTANCE_SIGNATURE` and `XDG_CURRENT_DESKTOP`.
- Routed startup native-decoration selection through platform capabilities.
- Hid horizontal and vertical app caption buttons for the Hyprland profile.
- Removed reserved caption-button width for the Hyprland profile.
- Disabled app resize grips and app drag-region behavior for the Hyprland profile.
- Added regression tests for platform capabilities, settings compatibility, and caption-width behavior.
- Added an initial action-driven shortcut keymap for top-level app shortcuts.
- Routed existing top-level shortcut handling through `ShortcutAction -> KeyBinding` lookup while preserving Windows defaults.
- Added config-backed shortcut overrides under `[shortcuts]`.
- Added NixOS/Hyprland configuration snippets in `docs/hyprland-nixos-configuration.md`.
- Added Linux flake outputs for `packages.x86_64-linux.default`, `packages.x86_64-linux.scratchpad-hyprland`, and matching app outputs.
- Added a Hyprland wrapper that sets `WINIT_UNIX_BACKEND=wayland` and `SCRATCHPAD_PLATFORM_PROFILE=hyprland`.
- Added `scripts/check-targets.sh` for Linux/Windows target `cargo check` runs when the targets are installed.
- Added stable runtime paths for Linux/NixOS settings and session state, with fallback migration from the previous temp-dir store.
- Added Settings status warnings when configured shortcut override strings are invalid and the app falls back to defaults.
- Added a first-class Home Manager module at `packaging/nix/home-manager.nix`, exposed through `homeManagerModules.default` and `homeManagerModules.scratchpad`.
- Wired `scripts/check-targets.sh` into CI with an Ubuntu cross-target job for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu`.
- Split native file-dialog/reveal handling behind `platform_file`, with explicit Linux XDG portal/open-containing-folder behavior and Windows Explorer behavior.
- Added `toggle_tab_list` (`Ctrl+Alt+B`) for opening/closing the tab list, while keeping `toggle_tab_list_auto_hide` (`Ctrl+Shift+B`) for the auto-hide setting.
- Moved the Phosphor icon font ahead of broad Unicode fallback fonts so open/save/search toolbar icons render in Linux environments.

Still pending:

- Run the new CI job in GitHub Actions and address any runner-specific dependency gaps.

## Test ideas

- Windows profile reports caption buttons visible.
- Hyprland profile reports caption buttons hidden.
- Header layout reserves caption width on Windows.
- Header layout reserves no caption width on Hyprland.
- Existing Windows shortcut map contains current bindings.
- Old settings files deserialize successfully after new fields are added.
- Hyprland profile does not consume compositor-level/global shortcuts unless configured as in-app shortcuts.
