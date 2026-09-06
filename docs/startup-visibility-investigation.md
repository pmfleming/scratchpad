# Startup visibility and repaint investigation

## Baseline capture

Captured before changing lifecycle behavior, from commit `c17c92a` plus opt-in
trace instrumentation. Environment: Hyprland 0.55.4, native Wayland, Mesa 26.1.x,
eframe/egui 0.36.1, winit 0.30.13. The `scratchpad` class rule initially placed
the test window on inactive workspace 5.

Each process used separate `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `TMPDIR`
roots and `/clean`. Isolating `TMPDIR` is important: otherwise the legacy
fallback can migrate the user's old session into the probe. The existing
Scratchpad process was not restarted or modified.

Captured eframe repaint scheduling, app logic/UI entry and exit, viewport
focus/minimize/occlusion flags, and Wayland protocol events. Compositor commands
and process CPU samples were recorded separately. Successful probes checked
actual workspace placement, showed/hid/reshowed the window, and closed it
normally (exit code 0). Early probes using obsolete Hyprland dispatcher syntax
were discarded for visibility-return analysis.

### Observations

| Scenario | Before | After |
| --- | --- | --- |
| Glow, initial inactive workspace | Three UI calls; 0% CPU | No startup gate; 0% CPU |
| Glow, show/hide/reshow by moving the window | Resumes, closes normally | Resumes, closes normally |
| Glow, actual workspace switches without moving/resizing the window | Resumes, closes normally | Resumes; explicit one-shot focus-return repaints; closes normally |
| WGPU, initial inactive workspace | Two UI calls, ~99.5% CPU | Still two UI calls, ~99.5% CPU |
| WGPU, show/hide/reshow by moving the window | Resumes, closes normally | Resumes, closes normally |

CPU samples were two-second intervals with tracing enabled, not performance
benchmarks. Transition samples include compositor animation/rendering and are
not useful steady-state comparisons.

Representative baseline Glow trace:

```text
+0.041893s ui begin pass=0 show_count=0 shown=false
+0.049478s ui begin pass=1 show_count=1 shown=false
+0.051155s ui begin pass=2 show_count=2 shown=false
+2.154825s ui begin pass=3 show_count=2 shown=true  # shown on active workspace
```

Representative updated Glow workspace-return trace:

```text
+2.167020s resume repaint visible=None focused=true
+6.213714s resume repaint visible=None focused=true
```

Wayland did not supply occlusion information (`occluded=None`), so workspace
return must not depend exclusively on an `Occluded(false)` event. Focus return
is another useful signal. Unfocused does **not** mean hidden.

The baseline already recovered in these isolated Glow tests. This does not
prove that the reported persistent startup unresponsiveness is resolved for
all sessions. It does confirm the fragile startup dependency and reproduce the
separate WGPU redraw-starvation problem described in the
[earlier renderer investigation](wgpu-glow-resource-usage-investigation-2026-08-17.md).

## Changes

- Remove the application-level two-pass warm-up/third-call `Visible(true)` gate.
  Its counter counted UI calls, not completed native paints; egui can also rerun
  UI for layout within one native frame.
- Let eframe handle showing the window after the first paint. winit's Wayland
  `set_visible` is a no-op anyway.
- Apply saved maximization in the initial viewport builder, alongside geometry,
  rather than waiting for the second UI call.
- Run close handling, file-watch/dialog polling, background I/O, broker launch
  processing, and session persistence in `App::logic`, which eframe can invoke
  without painting minimized/occluded windows.
- Keep dropped-file/input handling, layout, fonts/theme, and painting in the UI
  path. Hidden `run_logic` retains the previous UI input, which must not be
  replayed as fresh events.
- Request one repaint when rendering becomes available again or focus returns.
  Do not add a permanent repaint timer or treat an unfocused window as hidden.
- Update the headless rendering profiler to call `App::logic` before `App::ui`,
  matching eframe's lifecycle.

Simply adding `request_repaint()` after `Visible(true)` would be redundant:
`Context::send_viewport_cmd` already requests a repaint internally.

## Limits and regression coverage

The backend must still deliver a wakeup. An app-level logic hook cannot repair
an event loop that never calls it, or force compositor frame callbacks to
arrive. The WGPU busy-poll persists; retain Glow as the Linux default. No eframe,
winit, compositor configuration, or dependency changes were made.

Headless tests cover startup restore, real broker activation, close handling,
and dirty-session persistence without any UI passes; avoiding replay of stale
dropped files; initial viewport geometry/maximization; removal of UI-issued
startup visibility commands; and bounded visibility/focus-return repaint
transitions. Native Windows minimize/restore and startup-flash behavior still
need testing on Windows.

Validation on Linux:

- `cargo test --tests`: 516 passed, including 10 new regression tests.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo build --release --bin scratchpad`: passed.

The updated binary is `target/release/scratchpad`; the installed Nix-store
binary and already-running user instance remain unchanged.

## Repeating the capture

On Linux, with a trace-enabled build:

```sh
root=$(mktemp -d /tmp/scratchpad-window-trace.XXXXXX)
mkdir -p "$root/config" "$root/state" "$root/tmp"
XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state" TMPDIR="$root/tmp" \
  WINIT_UNIX_BACKEND=wayland SCRATCHPAD_RENDERER=glow \
  SCRATCHPAD_TRACE_WINDOW=1 WAYLAND_DEBUG=1 \
  target/release/scratchpad /clean 2>"$root/trace.log"
```

Switch to its workspace, away, and back, then close the probe. Repeat with
`SCRATCHPAD_RENDERER=wgpu`. Also check session restore without `/clean` in a
**synthetic** session root. Keep the app ID `scratchpad` so placement rules are
actually exercised. No production settings or compositor rules need changing.

`SCRATCHPAD_TRACE_WINDOW=1` is optional, writes to stderr rather than
`error.log`, and is disabled by default. App trace records include event counts
but not document contents or key values. `WAYLAND_DEBUG=1` is a separate, much
more verbose protocol trace and may contain input/window metadata; inspect
logs before sharing them.

Local investigation artifacts are under `/tmp/scratchpad-visibility-traces/`:
`baseline-glow`, `baseline-wgpu`, `after-glow`, `after-wgpu`,
`workspace-before-glow`, and `workspace-after-glow`. Each directory contains
`trace.log` and `events.jsonl`. These are temporary evidence, not tracked files.
