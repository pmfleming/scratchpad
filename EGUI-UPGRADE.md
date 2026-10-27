# egui 0.37 upgrade validation

Baseline: `1e5ffa2` (patched eframe 0.36.1). Target: upstream eframe/egui
0.37.0, Rust 1.95. WGPU remains the default; Linux retains Glow as a fallback.
No session schema, document format or settings migration is involved.

## Stage 1: dependencies and compatibility

- Updated both eframe requirements and the local Phosphor crate together.
- Removed the 0.36.1 eframe fork and its standalone CI step. Upstream now uses
  nonblocking Wayland Glow swaps, pre-present notification for the normal
  presentation path, sleeping event dispatch, and logic-only updates when a
  requested redraw is overdue.
- Migrated ID roots, UI scope/diagnostic identities, class imports, TextEdit
  alignment, ComboBox lifetimes, and synthetic wheel events.
- Replaced mutation of egui's now-private scroll state with queued offsets
  applied through `ScrollArea::scroll_offset`. A regression test checks ID
  matching and one-shot consumption. Removed the editor's unused mirror of
  egui scroll state; the custom editor keeps its own authoritative scroll state.

Validation performed on Linux/Hyprland, AMD Radeon 780M / RADV, Mesa 26.1.2:

| Release probe | Hidden-startup CPU ticks (1 s) | Final visible-second frames | Exit |
| --- | ---: | ---: | ---: |
| Patched 0.36.1 WGPU | 0 | 75 | 0 |
| Patched 0.36.1 Glow | 0 | 75 | 0 |
| Upstream 0.37.0 WGPU | 0 | 75 | 0 |
| Upstream 0.37.0 Glow | 1 | 75 | 0 |

Real 0.37 application runs with isolated synthetic sessions completed hidden
startup / show / hide / show / native close on both renderers. WGPU answered
7/7 compositor pings; Glow answered 6/6. Settled two-second CPU samples were
0–1 scheduler ticks. These short samples demonstrate absence of the old busy
loop/hang, not a comprehensive performance benchmark. Existing user windows,
settings and sessions were not modified. Local raw results are under
`target/wgpu-investigation/*-egui037/`.

Checks: baseline and upgraded `cargo test --all-features`, upgraded
`cargo check --all-targets --all-features`, and release app/probe builds passed.

## Limits and follow-up validation

- No Windows, X11, multi-monitor, suspend/resume, or native IME validation has
  yet been performed for this upgrade. CI builds do not replace those checks.
- The unused upstream immediate-viewport Glow path still lacks the normal
  path's pre-present notification. Scratchpad does not create immediate child
  viewports. Validate pacing before adding that feature; do not assume the
  normal-window tests cover it.
- The core egui crate no longer defaults to bundled fonts; eframe still does.
  Eframe also enables native system-font fallback and clipboard by default.
- Scroll source is currently `Unknown` on Windows, Wayland and X11. Do not
  advertise touchpad classification on those platforms based on this upgrade.
- Keep bundled fonts and the `fontdb` OS-font picker until a separately measured
  font-packaging change. Color glyph availability is system-font dependent;
  configured monochrome fonts can take precedence over color fallback.
- Experimental themes, image editing, completion UI, and a replacement editor
  or scroll engine are not part of the upgrade.

Upstream references: [release](https://github.com/emilk/egui/releases/tag/0.37.0),
[native scheduler](https://github.com/emilk/egui/blob/0.37.0/crates/eframe/src/native/run.rs),
[Glow integration](https://github.com/emilk/egui/blob/0.37.0/crates/eframe/src/native/glow_integration.rs).
