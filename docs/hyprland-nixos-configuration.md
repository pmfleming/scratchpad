# NixOS/Hyprland Configuration Notes

This document shows the intended way to run Scratchpad under Hyprland without making Scratchpad own compositor-level shortcuts.

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

Top-level in-app shortcuts can be overridden in `settings.toml`:

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

Shortcut syntax is case-insensitive. Supported modifiers are `ctrl`, `shift`, `alt`, and `command`/`super`/`win`. Multiple bindings can be separated with commas:

```toml
[shortcuts]
increase_font_size = "ctrl+equals, ctrl+plus"
```

Invalid shortcut strings fall back to the built-in default binding for that action.

## Hyprland principle

Hyprland should own global shortcuts. Scratchpad should own only shortcuts that operate while Scratchpad is focused.

Good division:

- Hyprland: launch/toggle/move Scratchpad windows.
- Scratchpad: open file, save file, split panes, search, close tab, etc.

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
    split_left = "ctrl+shift+left"
    split_right = "ctrl+shift+right"
    split_up = "ctrl+shift+up"
    split_down = "ctrl+shift+down"
  '';
}
```

## Future Nix module shape

A future first-class module could look like this:

```nix
programs.scratchpad = {
  enable = true;
  profile = "hyprland";

  shortcuts = {
    openFile = "ctrl+o";
    saveFile = "ctrl+s";
    closeTab = "ctrl+w";
  };

  hyprland = {
    enableBinds = true;
    toggleBind = "$mainMod, S";
    specialWorkspace = "scratchpad";
  };
};
```

That module does not exist yet; for now use the direct Hyprland/Home Manager snippets above.
