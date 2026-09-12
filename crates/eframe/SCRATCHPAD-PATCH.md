# Local eframe presentation patch

This is the published **eframe 0.36.1** crate's `Cargo.toml`, `README.md`,
`src/`, and `data/`, with two native implementation files patched. Upstream:
<https://github.com/emilk/egui/tree/4c1f2fae95475a40e524884ebb298bcb1714b08e/crates/eframe>.
Published `.crate` SHA-256:
`2afc0cbcdb6896b7bfb1dbbebaf7b9af9635ff38fd01e89bdd0174c1717b1857`.
The MIT and Apache licenses are included from that upstream revision.
Cargo cache markers, the original workspace manifest, and the upstream lockfile
are not needed by consumers. `.gitignore` and this note are local additions.

## Why a local dependency

On Hyprland 0.55.4 / Mesa 26.1.5, Scratchpad's initial Glow frames can block
inside EGL `swap_buffers` when the compositor places the window on an inactive
workspace. The event thread then cannot dispatch Wayland pings and Scratchpad
is reported unresponsive. Session restore and font initialization finish quickly;
removing session data does not address the presentation wait.

The application API cannot install winit's pre-present notification at the swap
boundary. Disabling vsync alone is not an acceptable workaround: it removes frame
pacing. The small renderer patch therefore lives here rather than in unsafe
application-level Wayland calls or a machine-wide Mesa override.

## Changes to upstream source

- `src/native/glow_integration.rs`: select `DontWait` for the **actual Wayland
  display handle**, not environment heuristics; notify winit immediately before
  both normal and immediate-viewport swaps. Compositor callbacks pace Wayland
  redraw delivery instead of a blocking EGL wait. Other display backends retain
  the configured EGL swap interval. Includes a swap-policy regression test.
- `src/native/run.rs`: use `ControlFlow::Wait`, not `Poll`, when requesting a
  Wayland redraw. Winit wakes the loop itself; pending timed repaints still use
  `WaitUntil`. Otherwise withheld frame callbacks burn a CPU core. Non-Wayland
  behavior is unchanged. Includes a control-flow regression test.

All three pieces are necessary: nonblocking swaps, pre-present notification,
and sleeping event dispatch. No timer-based frame cap or render-quality reduction
is introduced. Wayland redraws remain compositor-paced even if EGL vsync is off.

Both root manifest declarations pin `=0.36.1` so `cargo update` cannot silently
bypass this patch by choosing a newer unpatched eframe. eframe 0.36.2 fixes the
busy-poll issue (<https://github.com/emilk/egui/issues/8326>), but its Glow presenter
still lacks the notification and still selects a blocking swap interval.

## Testing and removal

From the Scratchpad root (also run by Linux CI):

```sh
cp Cargo.lock crates/eframe/Cargo.lock
cargo test --manifest-path crates/eframe/Cargo.toml --lib --features glow --target-dir target
```

The ignored standalone test lock starts with production dependency versions;
Cargo adds the upstream test-only dependency as needed. Production builds use
only the root lockfile. Native validation is documented in
[`docs/wayland-startup.md`](../../docs/wayland-startup.md).

When upstream supplies an equivalent nonblocking, callback-paced Glow presenter,
repeat the hidden/show/hide/show and continuous-redraw checks. Then update eframe,
remove the exact-version pins, `[patch.crates-io]`, this directory and its dedicated
CI step together. Keep the native probe as a regression diagnostic.
