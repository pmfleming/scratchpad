# Scratchpad Linux x86_64 archive

The archive contains a dynamically linked Linux build:

- `bin/scratchpad` — generic Linux launcher
- `bin/scratchpad-hyprland` — forces native Wayland and the Hyprland profile
- `share/applications/scratchpad.desktop` — desktop entry
- `share/icons/hicolor/scalable/apps/scratchpad.svg` — application icon

To install for the current user from the extracted directory:

```sh
install -Dm755 bin/scratchpad "$HOME/.local/bin/scratchpad"
install -Dm755 bin/scratchpad-hyprland "$HOME/.local/bin/scratchpad-hyprland"
install -Dm644 share/applications/scratchpad.desktop \
  "$HOME/.local/share/applications/scratchpad.desktop"
install -Dm644 share/icons/hicolor/scalable/apps/scratchpad.svg \
  "$HOME/.local/share/icons/hicolor/scalable/apps/scratchpad.svg"
```

Ensure `$HOME/.local/bin` is on `PATH`. The binary requires standard GTK 3,
Wayland/X11, fontconfig, and graphics-loader libraries. User settings are stored
at `$XDG_CONFIG_HOME/scratchpad/settings.toml` or
`~/.config/scratchpad/settings.toml`; session state and diagnostics use
`$XDG_STATE_HOME/scratchpad` or `~/.local/state/scratchpad`.

NixOS users should use the repository's `scratchpad` or
`scratchpad-hyprland` flake package instead, because an ordinary dynamically
linked Linux binary does not run directly on stock NixOS. The flake also
exports `homeManagerModules.default` for declarative settings and optional
Hyprland integration.
