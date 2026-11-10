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

## Stage 2: font fallback

- Explicitly enabled native system fallback in `NativeOptions`, while preserving
  all bundled text/control fonts and the isolated Phosphor icon family.
- Font switches now invalidate cached display snapshots as well as galleys;
  old row heights must not survive a family change.
- Added deterministic first-pass coverage tests for all editor presets and a
  fake discovering provider that proves configured fonts take precedence and
  providers survive `set_fonts` across font switches.
- Added an opt-in installed-font test for Arabic, Indic, emoji modifiers/ZWJ
  and actual colored atlas pixels. It passed on this Linux host with Noto Color
  Emoji. It stays ignored by default so ordinary CI is host-font independent:
  `cargo test --lib installed_fonts_render_missing_scripts_and_color_emoji -- --ignored`.
- No global fallback reordering or font asset removal: existing text metrics
  and control/icon precedence are preserved.

## Stage 3: custom editor input

- The custom editor emits `TextSelectionSettled` only for changed, settled,
  nonempty selections in its focused view. It caches range/revision identity,
  not another selection string, and does not overwrite the ordinary clipboard.
- Middle-button press positions the caret before PRIMARY paste. The previous
  focused split is excluded even if rendered first. Headless two-split tests
  verify only the target changes and an old selection is not replaced.
- Image-only paste is tested to leave text, revision and dirty state unchanged.
- Known Trackpad/Momentum deltas bypass additional wheel acceleration and reset
  its history. Wheel/Unknown retain the existing behavior, with regression tests.
- Native clipboard shortcuts remain enabled. Cross-application PRIMARY and IME
  behavior still need interactive platform acceptance; synthetic tests do not
  establish clipboard ownership behavior on every compositor.

## Stage 4: accessibility and diagnostics

- Custom icon buttons, tabs, tab actions, switches, radio controls and expandable
  headers now publish roles/names/state. Tab chrome derives child identities
  from the caller's surface key rather than screen coordinates.
- Search/replace and rename fields have explicit names. Settings use `name_inputs`
  to name otherwise-unnamed controls from their row/card labels. Floating dialogs
  have human-readable names and Dialog roles; icon-only callout actions are named.
- The custom editor exposes its name and multiline input role without copying
  the entire document into the accessibility tree. Full text-range navigation,
  SetValue and ReplaceSelectedText support is explicitly deferred; it needs a
  bounded, undo-aware custom-editor accessibility design.
- Added egui_kittest as a development dependency (no GPU/video feature required).
  Tests cover settings input names and AccessKit switch activation, search
  typing, named dialog closure/removal, and tab-primitive identity under reorder.
  Missing-glyph/accessibility checks stay enabled with deterministic bundled fonts.
- Removed the application's unconditional pass-ID-warning override. Egui's
  normal default remains unchanged; focused tests enable the warning and fail
  on captured egui warnings, with thread-local capture to isolate parallel tests.

Checks: all-feature tests and strict all-target/all-feature Clippy passed.

## Stage 5: final validation and packaging

Passed after the implementation stages:

- `cargo fmt --all -- --check`
- `cargo check --all-targets --all-features --locked`
- `cargo hack check --feature-powerset`
- `cargo test --all-features --locked` (509 library tests passed, plus binary
  and integration tests; installed-font test is opt-in and passed separately)
- `cargo clippy --all-targets --all-features --locked -- -D warnings`
- Standalone `crates/egui_phosphor` tests (both passed)
- `nix build` (initial dependency build exceeded the command timeout; retry
  completed successfully)
- One coherent egui 0.37.0 dependency family in `cargo tree -i egui`

The final Nix package passed isolated native Wayland hidden/show/hide/show/close
runs with the default renderer unset (WGPU) and the Glow override. A separate
WGPU restore run reused only its synthetic session. All exited successfully;
all 20 observed compositor pings received pongs, with zero scheduler ticks in
each settled two-second CPU sample. A WGPU probe launched with the X11 backend
selected also completed successfully at 75 visible frames/second. This is only
an X11 probe smoke check, not complete X11 application coverage.

Three alternating release `frame_metrics` runs compared the baseline commit
against the final candidate. Same host, bundled fonts and workloads; separate
synthetic state, no background build running:

| CPU-only scenario | Baseline median mean (ms) | Candidate median mean (ms) | Both median p95 (ms) |
| --- | ---: | ---: | ---: |
| Steady repaint | 0.208 | 0.210 | 0.5 |
| 1 MiB wheel scroll | 0.176 | 0.173 | 0.5 |
| 4 MiB wheel scroll | 0.172 | 0.169 | 0.5 |

The p95 histogram resolution is coarse; these short runs show no material
regression in these workloads, not improved GPU presentation or a full memory/
startup-latency assessment. Raw metrics are local under
`target/egui037-{baseline,candidate}/metrics-*.json`.

`cargo audit` completed with **seven allowed warnings**, not a warning-free
report: unmaintained `ttf-parser`, `zbus_names`, `zvariant`, `zvariant_derive`;
unsound `event-listener` (RUSTSEC-2026-0221); yanked `chacha20`, `clipboard-win`.
The baseline lockfile reports the same warning categories/crates and additionally
a blocking `webbrowser` advisory (RUSTSEC-2026-0257), no longer present in the
upgraded lockfile. The remaining warnings require separate dependency follow-up;
this upgrade does not suppress or claim to fix them.

The installed/running user application was not replaced or restarted. The built
package is available through `result/bin/scratchpad`; a launch while an older
primary is running will forward to that primary, not upgrade it in place.

## Limits and follow-up validation

- No Windows, multi-monitor, suspend/resume, or native IME validation has yet
  been performed for this upgrade. X11 has probe-only coverage. CI builds do
  not replace these interactive checks.
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
