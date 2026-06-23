{ scratchpad, scratchpad-hyprland }:
{ config, lib, pkgs, ... }:
let
  cfg = config.programs.scratchpad;
  settingsFormat = pkgs.formats.toml { };
  selectedPackage =
    if cfg.package != null then cfg.package
    else if cfg.profile == "hyprland" then scratchpad-hyprland
    else scratchpad;
  settings = lib.recursiveUpdate cfg.settings {
    platform.profile = cfg.profile;
    shortcuts = cfg.shortcuts;
  };
in
{
  options.programs.scratchpad = {
    enable = lib.mkEnableOption "Scratchpad text workspace";

    package = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = ''
        Scratchpad package to install. Defaults to the Hyprland wrapper when
        `profile` is `hyprland`, otherwise the regular Scratchpad package.
      '';
    };

    profile = lib.mkOption {
      type = lib.types.enum [ "auto" "windows" "linux_generic" "hyprland" ];
      default = "auto";
      description = "Scratchpad platform profile written to settings.toml.";
    };

    settings = lib.mkOption {
      type = lib.types.attrs;
      default = { };
      description = "Additional settings.toml values merged before profile and shortcuts.";
    };

    shortcuts = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      example = {
        open_file = "ctrl+o";
        save_file = "ctrl+s";
        close_tab = "ctrl+w";
        split_tile = "ctrl+alt+enter";
        split_left = "ctrl+shift+left";
        resize_tile_left = "ctrl+alt+left";
        move_tile_left = "ctrl+left";
      };
      description = "Scratchpad in-app shortcut overrides, including focused tile split/resize/move bindings.";
    };

    hyprland = {
      enableBinds = lib.mkEnableOption "Hyprland Scratchpad binds and window rules";

      mainMod = lib.mkOption {
        type = lib.types.str;
        default = "SUPER";
        description = "Hyprland main modifier used by generated binds.";
      };

      launchBind = lib.mkOption {
        type = lib.types.str;
        default = "$mainMod, S";
        description = "Hyprland bind prefix used to launch Scratchpad.";
      };

      toggleBind = lib.mkOption {
        type = lib.types.str;
        default = "$mainMod SHIFT, S";
        description = "Hyprland bind prefix used to toggle the special workspace.";
      };

      moveBind = lib.mkOption {
        type = lib.types.str;
        default = "$mainMod SHIFT, S";
        description = "Hyprland bind prefix used to move Scratchpad to its special workspace.";
      };

      specialWorkspace = lib.mkOption {
        type = lib.types.str;
        default = "scratchpad";
        description = "Hyprland special workspace name for Scratchpad.";
      };

      windowClass = lib.mkOption {
        type = lib.types.str;
        default = "scratchpad";
        description = "Window class matched by generated Hyprland window rules.";
      };

      windowSize = lib.mkOption {
        type = lib.types.str;
        default = "1200 800";
        description = "Hyprland window size rule value.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ selectedPackage ];

    xdg.configFile."scratchpad/settings.toml".source =
      settingsFormat.generate "scratchpad-settings.toml" settings;

    wayland.windowManager.hyprland.settings = lib.mkIf cfg.hyprland.enableBinds {
      "$mainMod" = cfg.hyprland.mainMod;
      bind = [
        "${cfg.hyprland.launchBind}, exec, ${selectedPackage}/bin/scratchpad"
        "${cfg.hyprland.toggleBind}, togglespecialworkspace, ${cfg.hyprland.specialWorkspace}"
        "${cfg.hyprland.moveBind}, movetoworkspace, special:${cfg.hyprland.specialWorkspace}"
      ];
      windowrulev2 = [
        "workspace special:${cfg.hyprland.specialWorkspace} silent, class:^(${cfg.hyprland.windowClass})$"
        "center, class:^(${cfg.hyprland.windowClass})$"
        "size ${cfg.hyprland.windowSize}, class:^(${cfg.hyprland.windowClass})$"
      ];
    };
  };
}
