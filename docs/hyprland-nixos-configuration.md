# NixOS/Hyprland Configuration Notes

Scratchpad is a cross-platform text workspace with a dedicated Hyprland
profile. Under that profile, Hyprland owns global window management and
Scratchpad owns only focused in-app actions. General TOML locations, sections,
and environment variables are documented in the
[configuration guide](configuration.md).

## Scratchpad settings

Use the Hyprland platform profile so Scratchpad hides app-rendered window controls and lets Hyprland own window management:

```toml
[platform]
profile = "hyprland"
```

The Nix flake also exposes a Hyprland wrapper that selects this profile when the settings file leaves `platform.profile` at its default `auto` value:

```sh
nix run .#scratchpad-hyprland
```

Scratchpad can use app-defined editor appearance or follow system appearance. For NixOS/Hyprland, system appearance uses fontconfig's configured monospace default for the editor font, GTK settings for font size/theme when available, and conservative Linux fallbacks:

```toml
[editor]
appearance_source = "system"
```

If you want the app to define appearance while still choosing from installed OS fonts, keep app appearance and set an OS font explicitly:

```toml
[editor]
appearance_source = "app"
font_source = "os"
os_font_family = "JetBrains Mono" # empty means default OS editor font
```

The system/default OS font path respects NixOS/Home Manager fontconfig settings such as `fonts.fontconfig.defaultFonts.monospace`.

Top-level in-app shortcuts can be overridden in `settings.toml`:

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

Shortcut syntax is case-insensitive. Supported modifiers are `ctrl`, `shift`, `alt`, and `command`/`super`/`win`. Multiple bindings can be separated with commas:

```toml
[shortcuts]
increase_font_size = "ctrl+equals, ctrl+plus"
```

Invalid shortcut strings show a Settings warning and fall back to the built-in default binding for that action.

## Hyprland principle

Hyprland should own global shortcuts. Scratchpad should own only shortcuts that operate while Scratchpad is focused.

Good division:

- Hyprland: launch/toggle/move Scratchpad windows.
- Scratchpad: open file, save file, split panes, resize/move focused panes, search, close tab, etc.

### `super` → `alt`: one leader for tiling

Scratchpad's tiling shortcuts deliberately mirror Hyprland's window-management
binds under a single substitution rule: **replace the compositor's `super` with
`alt`, keep the same base key**. Hyprland reserves `super` globally so a focused
app never sees it; `alt` is the in-app stand-in, and the editor leaves every
`alt`-modified key to the tiling layer (word-wise navigation stays on
`ctrl+arrow`), so there is no clash with text editing.

| Hyprland (window manager) | Scratchpad (focused app) | Action |
| --- | --- | --- |
| `super+enter` | `alt+enter` | split / new tile |
| `super+arrow` | `alt+arrow` | move tile |
| `super+shift+arrow` | `alt+shift+arrow` | resize tile |
| `super+ctrl+arrow` | `alt+ctrl+arrow` | directional split |

Because the base key is identical, the only thing to remember is "swap `super`
for `alt`." If you also want the literal `super` chord to work while Scratchpad
is focused, add it as a secondary binding — but only for keys Hyprland does not
reserve globally for the window:

```toml
[shortcuts]
split_tile = "alt+enter, super+enter"
move_tile_left = "alt+left, super+left"
move_tile_right = "alt+right, super+right"
```

## Example Hyprland config

Plain Hyprland config example:

```ini
$mainMod = SUPER

# Starts Scratchpad at login without switching away from the current workspace.
# The silent rule sends it to the regular fifth workspace, whose Waybar entry
# can display the Scratchpad icon.
exec-once = scratchpad-hyprland

bind = $mainMod, S, exec, scratchpad-hyprland
bind = $mainMod, 5, workspace, 5
bind = $mainMod SHIFT, 5, movetoworkspace, 5

windowrule = match:class ^(scratchpad)$, workspace 5 silent
```

Adjust class matching after checking the actual window class with:

```sh
hyprctl clients
```

## Home Manager style snippet

Conceptual Home Manager configuration:

```nix
{ config, pkgs, ... }:
{
  wayland.windowManager.hyprland.settings = {
    "$mainMod" = "SUPER";

    exec-once = [ "scratchpad-hyprland" ];

    bind = [
      "$mainMod, S, exec, scratchpad-hyprland"
      "$mainMod, 5, workspace, 5"
      "$mainMod SHIFT, 5, movetoworkspace, 5"
    ];

    windowrule = [
      "match:class ^(scratchpad)$, workspace 5 silent"
    ];
  };

  xdg.configFile."scratchpad/settings.toml".text = ''
    [platform]
    profile = "hyprland"

    [shortcuts]
    open_file = "ctrl+o"
    save_file = "ctrl+s"
    close_tab = "ctrl+w"
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
  '';
}
```

## Home Manager module

The flake exposes a first-class Home Manager module:

```nix
{
  inputs.scratchpad.url = "github:pmfleming/scratchpad";

  outputs = { home-manager, scratchpad, ... }: {
    homeConfigurations.your-user = home-manager.lib.homeManagerConfiguration {
      modules = [
        scratchpad.homeManagerModules.default
        {
          programs.scratchpad = {
            enable = true;
            profile = "hyprland";

            settings.editor = {
              appearance_source = "system";
            };

            shortcuts = {
              open_file = "ctrl+o";
              save_file = "ctrl+s";
              close_tab = "ctrl+w";
              toggle_tab_list = "ctrl+alt+b";
            };

            hyprland = {
              autoStart = true;
              workspace = "5";
            };
          };
        }
      ];
    };
  };
}
```

When `profile = "hyprland"`, the module installs the Hyprland wrapper by
default and writes `~/.config/scratchpad/settings.toml`. Values in
`programs.scratchpad.settings` become ordinary TOML tables; `profile` and
`shortcuts` are supplied by their dedicated module options. Setting
`hyprland.autoStart = true` adds an `exec-once` entry and the workspace rule
even when generated key bindings are disabled. Scratchpad starts tiled on
regular workspace `5` without changing the active workspace. This keeps the
fifth Waybar workspace/icon occupied and ready at login.

## Verify the window rule

The Wayland build explicitly uses `scratchpad` as its app ID, which Hyprland exposes as the window class. After launching, verify the installed compositor and package combination with:

```sh
hyprctl clients | grep -A8 -B2 scratchpad
```

The client should report class `scratchpad` and workspace `5`. If a downstream launcher changes the class, override `programs.scratchpad.hyprland.windowClass` or adjust the plain `windowrule` expression. These examples use the current Hyprland rule syntax; older Hyprland releases may require the deprecated `windowrulev2` form.
