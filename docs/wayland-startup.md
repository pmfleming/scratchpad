# Wayland startup responsiveness

## Reproduction and cause

On this investigation's Hyprland 0.55.4 / Mesa 26.1.5 system (AMD Ryzen 7 PRO
8840HS), the desktop starts Scratchpad silently on workspace 5. Both the installed
0.4.2 binary and the release build at `019fa4a` stalled when that workspace was
inactive. The current checkout rendered its first UI passes in roughly 50 ms,
then stopped progressing after pass 2. `strace` showed the main thread waiting
on Wayland inside the EGL presentation path. An eight-second client protocol
trace showed no dispatched ping/pong replies during the stall.

This was not a reason to discard the session, disable restore, reduce rendering
quality, force X11, or remove startup features. Tests used isolated XDG directories
with a private copy of the 119-tab session. The installed process and original
session data were left intact.

## Fix

The [local eframe patch](../crates/eframe/SCRATCHPAD-PATCH.md) uses nonblocking EGL
swaps on Wayland, calls winit's pre-present notification to retain compositor
frame pacing, and sleeps rather than busy-polling while redraw delivery is
withheld. Detection uses the actual display backend; X11 and other platform
swap-interval policies are unchanged. Linux still defaults to Glow.

Disabling Mesa vsync alone avoided the stall but rendered 141 UI passes in about
0.25 seconds. Adding callbacks without changing eframe's polling policy consumed
an entire CPU core on the hidden workspace. Neither partial workaround was kept.

## Native validation

- The patched hidden startup dispatched all six observed compositor pings and
  queued their pong replies promptly (within 0.1 ms of client dispatch).
- A real 119-tab restore survived hidden startup, show, hide, show, and a native
  close request while visible. All four observed pings had corresponding pongs;
  the process exited normally, retaining all 119 tabs in the isolated session.
  Hidden steady-state CPU growth was 0 scheduler ticks over 1.5 seconds.
- A continuous-redraw probe was compiled against original and patched eframe,
  then run serially, alternating order over six trials per variant. Both produced
  **74–75 frames/second** on the 75 Hz output. Median visible process CPU was
  **3.75% of one core for both** (two-second samples). The patched trials did not
  spin while hidden, resumed on reveal, and closed normally.

These are native presentation/event-loop observations, not input-to-photon
measurements or a guarantee across every compositor/driver. CPU frequency and
scheduling were not pinned; scheduler-tick resolution limits short CPU samples.
The probe is lightweight, not a replacement for editor/search/capacity benchmarks.
No application editing, search, or session algorithms were changed.

Validation also passed 528 application tests (all features), both native
presentation-policy tests, strict all-target/all-feature Clippy, and formatting.
The policy tests run in Linux CI; native compositor checks remain opt-in.

Raw local evidence is retained in the sibling lens checkout's
`target/startup-investigation/`: before/fixed protocol traces, `pacing-repetitions.json`,
`pacing-summary.log`, `restore-cycle-results.json`, and build/test logs. Private
session copies and protocol traces are deliberately not committed.

## Repeating the checks

```sh
cargo run --release --example presentation_probe
```

The opt-in probe reads no user session/settings/files, continuously requests
redraws, prints frame counts once per second, and requests close after five
seconds. It uses app ID `scratchpad` so the same compositor routing rules apply.
When testing hidden startup, move **only the probe's window** to a visible
workspace before five seconds and verify it resumes and exits. Otherwise use
an external timeout. Compare settled frame counts with the output refresh rate,
and inspect process CPU both hidden and visible. An uncapped or busy-polling
workaround must not pass this check.

For the real app, use isolated XDG directories, `SCRATCHPAD_TRACE_WINDOW=1`, and
`WAYLAND_DEBUG=client`; verify ping/pong dispatch while hidden and UI resumption
on reveal. Those logs can contain private window/session metadata: keep them local.
Use a Nix development shell (or the package runtime library path) when launching
a locally built binary on NixOS.

The fix changes source, not an already installed Nix store binary. Rebuild the
package and restart Scratchpad to use it; `nix run .#scratchpad-hyprland` builds
and runs this checkout's wrapper. Existing desktop/autostart entries require
rebuilding the Nix configuration that supplies them.
