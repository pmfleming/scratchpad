{
  description = "Scratchpad Rust development environment and Linux package";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      lib = pkgs.lib;

      nativeBuildInputs = with pkgs; [
        cargo
        clippy
        pkg-config
        rust-analyzer
        rustc
        rustfmt
        stdenv.cc
      ];

      buildInputs = with pkgs; [
        atk
        cairo
        dbus
        egl-wayland
        fontconfig
        freetype
        gdk-pixbuf
        glib
        gtk3
        libGL
        libglvnd
        libxkbcommon
        mesa
        pango
        vulkan-loader
        wayland
        wayland-protocols
        libx11
        libxcursor
        libxext
        libxi
        libxrandr
      ];

      runtimeLibraryPath = lib.makeLibraryPath buildInputs;

      scratchpadDesktopEntry = pkgs.writeText "scratchpad.desktop" ''
        [Desktop Entry]
        Type=Application
        Name=Scratchpad
        Comment=Plain-text scratch workspace
        Exec=scratchpad %F
        Icon=scratchpad
        Terminal=false
        Categories=Utility;TextEditor;
        MimeType=text/plain;
        StartupWMClass=scratchpad
      '';

      scratchpad = pkgs.rustPlatform.buildRustPackage {
        pname = "scratchpad";
        version = "0.4.1";
        src = lib.cleanSource ./.;

        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [
          makeWrapper
          pkg-config
        ];
        inherit buildInputs;

        postInstall = ''
          install -Dm644 ${scratchpadDesktopEntry} \
            $out/share/applications/scratchpad.desktop
          install -Dm644 ${./assets/Scratchpad.svg} \
            $out/share/icons/hicolor/scalable/apps/scratchpad.svg

          wrapProgram $out/bin/scratchpad \
            --prefix LD_LIBRARY_PATH : ${runtimeLibraryPath}
        '';

        meta = {
          description = "Scratchpad text workspace";
          mainProgram = "scratchpad";
          platforms = lib.platforms.linux;
        };
      };

      scratchpad-hyprland = pkgs.stdenvNoCC.mkDerivation {
        pname = "scratchpad-hyprland";
        inherit (scratchpad) version;
        dontUnpack = true;
        nativeBuildInputs = [ pkgs.makeWrapper ];

        installPhase = ''
          mkdir -p $out/bin
          makeWrapper ${scratchpad}/bin/scratchpad $out/bin/scratchpad \
            --set WINIT_UNIX_BACKEND wayland \
            --set SCRATCHPAD_PLATFORM_PROFILE hyprland
          ln -s ${scratchpad}/share $out/share
        '';

        meta = scratchpad.meta // {
          description = "Scratchpad wrapped for Hyprland/Wayland";
          mainProgram = "scratchpad";
        };
      };

      scratchpadHomeManagerModule = import ./packaging/nix/home-manager.nix {
        inherit scratchpad scratchpad-hyprland;
      };
    in
    {
      packages.${system} = {
        default = scratchpad;
        scratchpad = scratchpad;
        scratchpad-hyprland = scratchpad-hyprland;
      };

      apps.${system} = {
        default = {
          type = "app";
          program = "${scratchpad}/bin/scratchpad";
          meta = scratchpad.meta;
        };
        scratchpad-hyprland = {
          type = "app";
          program = "${scratchpad-hyprland}/bin/scratchpad";
          meta = scratchpad-hyprland.meta;
        };
      };

      checks.${system} = {
        scratchpad = scratchpad;
      };

      homeManagerModules = {
        default = scratchpadHomeManagerModule;
        scratchpad = scratchpadHomeManagerModule;
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = nativeBuildInputs ++ buildInputs;

        LD_LIBRARY_PATH = runtimeLibraryPath;

        shellHook = ''
          if [[ -x "$PWD/scripts/trim-target.sh" ]]; then
            "$PWD/scripts/trim-target.sh"
          fi
          echo "Scratchpad dev shell"
          echo "Try: cargo check"
        '';
      };
    };
}
