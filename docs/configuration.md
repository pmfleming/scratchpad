# Scratchpad Configuration

Scratchpad can be configured from its Settings page or through
`settings.toml`. In-app controls are the recommended interface; direct TOML is
useful for managed installations, shortcut overrides, and settings that need to
be reproduced across machines.

## File locations

| Platform | Settings file | Session state, recovery data, and `error.log` |
| --- | --- | --- |
| Windows | `%APPDATA%\Scratchpad\settings.toml` | `%LOCALAPPDATA%\Scratchpad` |
| Linux | `$XDG_CONFIG_HOME/scratchpad/settings.toml`, falling back to `~/.config/scratchpad/settings.toml` | `$XDG_STATE_HOME/scratchpad`, falling back to `~/.local/state/scratchpad` |

If the platform root cannot be resolved, Scratchpad falls back to the system
temporary directory under `scratchpad`. Older settings and session data from
that temporary location are migrated when possible.

Open the active settings file from **Settings → Advanced → Settings file**. A
settings-file tab behaves like an ordinary text buffer. Scratchpad validates and
applies it when you leave or close that buffer. Invalid TOML leaves the current
settings in place and reports the parse error.

Scratchpad owns this file and rewrites it when settings are persisted. Formatting,
comments, unknown keys, and unknown top-level sections are not preserved. Keep
explanatory or generated source configuration elsewhere and generate
`settings.toml` from it when comments must be retained.

## TOML structure

Only six top-level tables are used:

```toml
[editor]
[workspace]
[ui]
[history]
[platform]
[shortcuts]
```

Missing tables and keys use application defaults. A complete copyable example
is in [`settings.toml`](settings.toml).

### Editor

```toml
[editor]
appearance_source = "app"       # app, system
font_size = 14.0
word_wrap = true
editor_gutter = 0               # line-number gutter width; 0 uses no fixed gutter
editor_tab_width = 4            # 1..16
indentation_style = "tab_character" # spaces, tab_character
tab_display = "hidden"         # hidden, character, tablines
editor_font = "standard"        # standard, flex, mono, serif
font_source = "scratchpad"      # scratchpad, os
os_font_family = ""             # empty selects the OS default editor font
theme_mode = "system"           # system, light, dark
editor_text_color = "#ffffff"
editor_background_color = "#15181d"
editor_text_highlight_color = "#fff36d"
editor_text_highlight_text_color = "#0b0f3d"
```

With `appearance_source = "system"`, Scratchpad resolves the editor font,
font size, theme, and palette from the desktop and uses conservative fallbacks
when a value is unavailable. With `appearance_source = "app"`, the remaining
editor values are authoritative. Colors use six-digit `#RRGGBB` notation.

`font_source = "scratchpad"` selects one of the bundled font presets.
`font_source = "os"` uses `os_font_family`, or the desktop's default editor
font when the family is empty.

### Workspace

```toml
[workspace]
tab_list_position = "top"       # top, bottom, left, right
tab_order_mode = "custom"       # custom, file_name, file_size, file_age, recent_edit
tab_order_direction = "ascending" # ascending, descending
file_open_disposition = "new_tab" # new_tab, current_tab
new_tab_placement = "end"       # start, end, before_selection, after_selection
startup_session_behavior = "continue_previous_session" # or start_fresh_session
tab_list_width = 184.0
auto_hide_tab_list = false
tab_list_auto_hide_delay_seconds = 3.0
recent_files_enabled = true
```

Scratchpad also writes `recently_closed_files` in this table. It is runtime
history rather than a setting normally managed by hand. Entries contain `path`
and `closed_at_unix_seconds`; older path-only entries are still accepted and
appear after timestamped entries in Recently Closed.

### UI state

```toml
[ui]
status_bar_visible = true
settings_tab_open = true
settings_tab_index = 1

[ui.window_state]
maximized = false
```

`settings_tab_open`, `settings_tab_index`, and `window_state` are maintained by
the application. They are documented so generated configurations do not mistake
them for unsupported values.

### Undo and text-history budgets

Memory budget values are bytes:

```toml
[history]
per_file_byte_budget = 67108864
aggregate_byte_budget = 536870912
derived_from_memory = true
```

**Reset to auto** recalculates limits from available memory. These limits evict
cached text chunks; they do not delete edits or closed documents. The in-app
controls display MiB. History has no age limit, entry limit, or disk budget.

### History and recovery

Opening a retained path always restores its document and branches. The former
`reopen_archives` and `open_changed_disk_files` options and `[history.recovery]`
section have been removed. External changes never create a separate document.
Clean documents load changed disk text as a reversible external edit. Dirty open
or restored documents keep their local text and path and report a disk conflict.
A retained closed document loads changed disk text on explicit reopening, with
its preceding unsaved state still reachable. Missing dirty startup files recover
as untitled documents with the same identity.

**Settings > History & Recovery** provides memory limits and measured storage
usage. Durable history, activity and branches remain until **Forget history**
for that file, or an explicit **Start new session**. See
[retention details](../SESSION-RETENTION.md) and [navigation](../HISTORY-NAVIGATION.md).

### Platform profile

```toml
[platform]
profile = "auto" # auto, windows, linux_generic, hyprland
```

`auto` selects Windows on Windows, detects Hyprland from the Linux desktop
environment, and otherwise selects generic Linux. Profiles control window
chrome, resize and drag behavior, shortcut defaults, and whether global window
management belongs to the compositor. They do not change document behavior or
file formats.

Set `SCRATCHPAD_PLATFORM_PROFILE` to `windows`, `linux`, `linux_generic`, or
`hyprland` to override automatic detection for a launch. An explicit non-`auto`
TOML profile takes precedence over normal desktop detection; the environment
override participates in resolving the automatic profile.

## Shortcut overrides

`[shortcuts]` changes app-level actions only. Editor text movement and search
field editing retain their built-in behavior.

```toml
[shortcuts]
open_file = "ctrl+o"
save_file = "ctrl+s"
increase_font_size = "ctrl+equals, ctrl+plus"
reset_zoom = "ctrl+0"
toggle_line_numbers = "ctrl+9"
split_tile = "ctrl+alt+enter"
```

The existing `increase_font_size` and `decrease_font_size` keys now control zoom
for the active pane, not the global font-size preference. `reset_zoom` returns
that pane to 100%. The default line-number shortcut is now `ctrl+9`.

Names and modifiers are case-insensitive. Supported modifiers are `ctrl`,
`shift`, `alt`, and `command`/`super`/`win`. Separate multiple bindings with
commas. Invalid overrides produce a warning and fall back to the active
platform profile's default.

The complete action-key list is in the user manual under
[Customizing app shortcuts](user-manual.md#customizing-app-shortcuts). Tooltips
show the resolved binding and are the best reference after profile selection
and user overrides have been combined.

Hyprland's defaults use `Alt` as the focused-app equivalent of the compositor's
`Super` leader. See [NixOS/Hyprland Configuration Notes](hyprland-nixos-configuration.md)
for the complete mapping.

## Runtime environment

| Variable | Purpose |
| --- | --- |
| `SCRATCHPAD_PLATFORM_PROFILE` | Override automatic platform-profile detection for the launch. |
| `SCRATCHPAD_RENDERER=wgpu` | Explicitly select WGPU, the default renderer. Unset, empty, or unrecognized values also select WGPU. The local scheduler patch prevents the former Wayland busy-polling problem. |
| `SCRATCHPAD_RENDERER=glow` | On Linux, select the fallback Glow (OpenGL) renderer. Renderer names are case-insensitive. |
| `SCRATCHPAD_TRACE_WINDOW=1` | Write timestamped lifecycle, viewport-state, and eframe repaint diagnostics to stderr; disabled by default. See [visibility investigation](startup-visibility-investigation.md). |
| `SCRATCHPAD_SYSTEM_APPEARANCE_FILE` | Read a system-appearance bridge TOML from an explicit path. |
| `EGUI_INSPECTION=1` | With the `inspection` Cargo feature, expose egui inspection support. |
| `CARGO_TARGET_MAX_GIB` | Manual cleanup command's Cargo cache budget in GiB (default 4; 0 disables). Shutdown cleanup uses the host's module configuration. See [cache policy](../BUILD-CACHE.md); not an app setting. |
| `SCRATCHPAD_TARGET_MAX_GIB` | Legacy fallback for `CARGO_TARGET_MAX_GIB`. |

On Linux the optional system-appearance bridge is also read from
`$XDG_CONFIG_HOME/scratchpad/system-appearance.toml`,
`~/.config/scratchpad/system-appearance.toml`, or
`/etc/scratchpad/system-appearance.toml`:

```toml
[font]
family = "JetBrains Mono"
size = 14.0

[palette]
color_scheme = "dark"
text = "#f2f2f2"
background = "#15181d"
accent = "#6ea8fe"
highlight = "#fff36d"
highlight_text = "#0b0f3d"
```

This bridge is optional; normal Windows and Linux desktop detection remains the
default.

## Home Manager

The flake exports `homeManagerModules.default`:

```nix
programs.scratchpad = {
  enable = true;
  profile = "auto";

  settings = {
    editor = {
      appearance_source = "system";
      word_wrap = true;
    };
    workspace.tab_list_position = "left";
  };

  shortcuts = {
    open_file = "ctrl+o";
    save_file = "ctrl+s";
  };
};
```

`programs.scratchpad.settings` maps to TOML tables. The module always writes
`platform.profile` from `programs.scratchpad.profile` and writes `[shortcuts]`
from `programs.scratchpad.shortcuts`, so use the dedicated options for those two
tables.

For Hyprland:

```nix
programs.scratchpad = {
  enable = true;
  profile = "hyprland";
  hyprland = {
    autoStart = true;
    workspace = "5";
    enableBinds = true;
  };
};
```

The Hyprland profile selects the wrapped package by default. Generated global
bindings and window rules remain opt-in through the `hyprland` options.
