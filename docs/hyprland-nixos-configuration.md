# NixOS/Hyprland Configuration Notes

This document shows the intended way to run Scratchpad under Hyprland: Hyprland owns global window-management shortcuts, and Scratchpad owns focused in-app tile shortcuts.

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
split_tile = "ctrl+alt+enter"
split_left = "ctrl+shift+left"
split_right = "ctrl+shift+right"
split_up = "ctrl+shift+up"
split_down = "ctrl+shift+down"
resize_tile_left = "ctrl+alt+left"
resize_tile_right = "ctrl+alt+right"
resize_tile_up = "ctrl+alt+up"
resize_tile_down = "ctrl+alt+down"
move_tile_left = "ctrl+left"
move_tile_right = "ctrl+right"
move_tile_up = "ctrl+up"
move_tile_down = "ctrl+down"
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

If you want Scratchpad to mirror focused Hyprland-style `SUPER` bindings, add them as secondary bindings and make sure Hyprland does not reserve those exact keys globally for Scratchpad windows:

```toml
[shortcuts]
split_tile = "ctrl+alt+enter, super+enter"
resize_tile_left = "ctrl+alt+left, super+left"
resize_tile_right = "ctrl+alt+right, super+right"
move_tile_left = "ctrl+left, super+shift+left"
move_tile_right = "ctrl+right, super+shift+right"
```

## Example Hyprland config

Plain Hyprland config example:

```ini
$mainMod = SUPER

bind = $mainMod, S, exec, scratchpad
bind = $mainMod SHIFT, S, togglespecialworkspace, scratchpad
bind = $mainMod SHIFT, S, movetoworkspace, special:scratchpad

windowrulev2 = workspace special:scratchpad silent, class:^(scratchpad)$
windowrulev2 = center, class:^(scratchpad)$
windowrulev2 = size 1200 800, class:^(scratchpad)$
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

    bind = [
      "$mainMod, S, exec, scratchpad"
      "$mainMod SHIFT, S, togglespecialworkspace, scratchpad"
      "$mainMod SHIFT, S, movetoworkspace, special:scratchpad"
    ];

    windowrulev2 = [
      "workspace special:scratchpad silent, class:^(scratchpad)$"
      "center, class:^(scratchpad)$"
      "size 1200 800, class:^(scratchpad)$"
    ];
  };

  xdg.configFile."scratchpad/settings.toml".text = ''
    [platform]
    profile = "hyprland"

    [shortcuts]
    open_file = "ctrl+o"
    save_file = "ctrl+s"
    close_tab = "ctrl+w"
    split_tile = "ctrl+alt+enter"
    split_left = "ctrl+shift+left"
    split_right = "ctrl+shift+right"
    split_up = "ctrl+shift+up"
    split_down = "ctrl+shift+down"
    resize_tile_left = "ctrl+alt+left"
    resize_tile_right = "ctrl+alt+right"
    resize_tile_up = "ctrl+alt+up"
    resize_tile_down = "ctrl+alt+down"
    move_tile_left = "ctrl+left"
    move_tile_right = "ctrl+right"
    move_tile_up = "ctrl+up"
    move_tile_down = "ctrl+down"
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
              enableBinds = true;
              toggleBind = "$mainMod SHIFT, S";
              specialWorkspace = "scratchpad";
            };
          };
        }
      ];
    };
  };
}
```

When `profile = "hyprland"`, the module installs the Hyprland wrapper by default and writes `~/.config/scratchpad/settings.toml`.
