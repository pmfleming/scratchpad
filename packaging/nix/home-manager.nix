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
      description = ''
        Additional settings.toml tables and values. Use the dedicated `profile`
        and `shortcuts` options for those tables. Scratchpad may rewrite its
        active settings file, so this generated file should be treated as
        declarative Home Manager state.
      '';
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
        move_tile_left = "ctrl+alt+shift+left";
      };
      description = ''
        Scratchpad app-action shortcut overrides written to `[shortcuts]`.
        Values use case-insensitive `ctrl`, `shift`, `alt`, and
        `command`/`super`/`win` modifiers; comma-separate multiple bindings.
      '';
    };

    hyprland = {
      enableBinds = lib.mkEnableOption "Hyprland launch/focus/move binds and Scratchpad workspace rule";

      autoStart = lib.mkEnableOption "starting Scratchpad on its Hyprland workspace";

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

      focusBind = lib.mkOption {
        type = lib.types.str;
        default = "$mainMod, 5";
        description = "Hyprland bind prefix used to focus the Scratchpad workspace.";
      };

      moveBind = lib.mkOption {
        type = lib.types.str;
        default = "$mainMod SHIFT, 5";
        description = "Hyprland bind prefix used to move a window to the Scratchpad workspace.";
      };

      workspace = lib.mkOption {
        type = lib.types.str;
        default = "5";
        description = "Hyprland workspace used by generated rules, focus binds, and autostart.";
      };

      windowClass = lib.mkOption {
        type = lib.types.str;
        default = "scratchpad";
        description = "Window class matched by generated Hyprland window rules.";
      };

      floating = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Whether Scratchpad floats instead of tiling on its workspace.";
      };

      windowSize = lib.mkOption {
        type = lib.types.str;
        default = "1200 800";
        description = "Hyprland window size used when `floating` is enabled.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ selectedPackage ];

    xdg.configFile."scratchpad/settings.toml".source =
      settingsFormat.generate "scratchpad-settings.toml" settings;

    wayland.windowManager.hyprland.settings = lib.mkMerge [
      (lib.mkIf cfg.hyprland.enableBinds {
        "$mainMod" = cfg.hyprland.mainMod;
        bind = [
          "${cfg.hyprland.launchBind}, exec, ${selectedPackage}/bin/scratchpad"
          "${cfg.hyprland.focusBind}, workspace, ${cfg.hyprland.workspace}"
          "${cfg.hyprland.moveBind}, movetoworkspace, ${cfg.hyprland.workspace}"
        ];
      })
      (lib.mkIf (cfg.hyprland.enableBinds || cfg.hyprland.autoStart) {
        windowrule = [
          "match:class ^(${cfg.hyprland.windowClass})$, workspace ${cfg.hyprland.workspace} silent"
        ] ++ lib.optionals cfg.hyprland.floating [
          "match:class ^(${cfg.hyprland.windowClass})$, float on"
          "match:class ^(${cfg.hyprland.windowClass})$, center on"
          "match:class ^(${cfg.hyprland.windowClass})$, size ${cfg.hyprland.windowSize}"
        ];
      })
      (lib.mkIf cfg.hyprland.autoStart {
        exec-once = [ "${selectedPackage}/bin/scratchpad" ];
      })
    ];
  };
}
