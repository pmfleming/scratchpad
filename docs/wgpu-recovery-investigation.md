# WGPU recovery investigation

Follow-up to the [original WGPU/Glow investigation](wgpu-glow-resource-usage-investigation-2026-08-17.md), using application commit `8546cb4` and its existing local eframe patch. The attempted cross-platform Glow-default change was reverted before testing. The native measurements below used explicit renderer overrides without changing production defaults or dependencies.

**Adoption:** following this investigation, Scratchpad now explicitly defaults to WGPU. Linux retains `SCRATCHPAD_RENDERER=glow` as a fallback. The exact eframe pin and local presentation/scheduler patch are unchanged. The native coverage limitations below still apply.

## Conclusion

**WGPU works with the current Scratchpad checkout on the tested Wayland system.** The earlier busy-polling problem is addressed by the shared event-loop change already introduced in `0b4faa6`, not by a requirement to use Glow.

An isolated control build restoring `ControlFlow::Poll` reproduced approximately 99% of one CPU core at hidden startup. The same WGPU probe with the current sleeping event loop used zero scheduler ticks in the corresponding samples and resumed compositor-paced rendering when shown. Upstream eframe 0.36.2 also passed the probe.

This validates a solution to the reported scheduling defect. It is not a guarantee for every GPU, compositor, or editor feature.

## Mechanism

The original eframe 0.36.1 loop selected `ControlFlow::Poll` before `window.request_redraw()`. Hyprland can withhold frame callbacks when routing the window to an inactive workspace. No redraw then arrives to clear the polling state, so the event loop consumes a core without useful rendering.

The current `crates/eframe/src/native/run.rs` selects `ControlFlow::Wait` for actual Wayland display handles before requesting a redraw, and retains `WaitUntil` for scheduled timers. **This wrapper is shared by Glow and WGPU.** Its regression test is `withheld_wayland_redraws_sleep_instead_of_busy_polling`.

WGPU's egui-wgpu 0.36.1 presenter already calls `window.pre_present_notify()` immediately before `queue.present(...)`. It does not need the Glow-specific notification patch copied into it. The separate nonblocking EGL swap changes remain necessary for the Glow fallback.

I also inspected the locally available upstream eframe 0.36.2 source. Its `native/run.rs` removes the switch to polling and always selects either `WaitUntil(next_repaint)` or `Wait` afterward, citing [egui issue 8326](https://github.com/emilk/egui/issues/8326). This is an upstream solution to the same defect.

## Native evidence

Environment:

- Hyprland 0.56.2, Wayland, 3440 × 1440 output at 75.05 Hz, scale 1.25.
- AMD Radeon 780M; default WGPU adapter was Vulkan/RADV, Mesa 26.1.2.
- Explicit `WGPU_BACKEND=gl` selected WGPU's GL backend with Mesa 26.2.3; this is still WGPU, not the Glow renderer.
- Current app: eframe/egui 0.36.1, WGPU 30.0.0.
- Optimized builds were used for the final comparisons. Earlier debug smoke tests also passed.

All app runs used separate XDG settings, state and cache roots, a separate temporary directory for legacy fallback, and synthetic UTF-8 files. Instance-broker identity was therefore isolated. Only windows belonging to each test PID were moved and closed. Existing app instances, real session data and compositor rules were not modified.

### Scheduler A/B control

Built two isolated probe packages against the same example and dependency requirements. One used the current local eframe; the other used a private copy differing only in `redraw_control_flow`, returning `Poll` instead of choosing `Wait` for Wayland. The control is not an application patch.

Each probe started on the existing class-routed hidden workspace, was shown, hidden, and shown again. Repeated in reversed order, with two runs per variant. CPU is process scheduler time divided by wall time, as a percentage of one core.

| Release probe | Hidden-startup CPU, 1-second sample | Final full visible-second frames | Exit |
| --- | ---: | ---: | --- |
| WGPU Vulkan, polling control | 98.98%, 98.98% | 75, 75 | 0, 0 |
| WGPU Vulkan, current sleeping loop | 0 ticks, 0 ticks | 75, 75 | 0, 0 |
| WGPU Vulkan, upstream eframe 0.36.2 | 0 ticks, 0 ticks | 75, 75 | 0, 0 |
| WGPU GL, current sleeping loop | 0 ticks | 75 | 0 |

The continuous-redraw workload naturally consumes CPU while visible. Transition-containing samples are not steady-state benchmarks: moving a window initiates compositor animations. The important observations are the elimination of hidden startup spin, resumed frame delivery, and retained refresh-rate pacing—not a claim that WGPU is faster than Glow.

### Real Scratchpad app

| Current release app run | Session after close | Hidden-startup CPU, 2-second sample | Ping/pong pairs | Exit |
| --- | --- | ---: | ---: | ---: |
| WGPU Vulkan: open fixture files | 5 tabs / 4 file buffers | 0 ticks | 7/7 | 0 |
| WGPU Vulkan: restore that session | 5 tabs / 4 file buffers | 0 ticks | 6/6 | 0 |
| WGPU Vulkan: open larger fixture set | 119 tabs / 118 file buffers | 0 ticks | 7/7 | 0 |
| WGPU Vulkan: restore larger session | 119 tabs / 118 file buffers | 0 ticks | 7/7 | 0 |
| WGPU GL: open fixture files | 5 tabs / 4 file buffers | 0 ticks | 6/6 | 0 |

Every run survived show/hide/show, delivered subsequent UI passes, and closed via the native window-close request. All 33 observed compositor pings in these release runs received corresponding pongs; the longest logged client-dispatch-to-pong interval was 0.187 ms. These are client protocol timestamps, not an end-to-end latency guarantee. Session manifests retained every synthetic file tab.

The five-tab Vulkan restore had zero scheduler ticks in all four settled two-second phase samples. Larger-session visible samples included hydration/UI work and were not zero; no claim of zero CPU for all visible workloads is made. The 119-tab fixture uses small synthetic files, not the user's original large session.

Raw local evidence is under `target/wgpu-investigation/`: probe/app stdout and stderr, phase CPU samples in each `result.json`, isolated control/upstream manifests and sources, build logs, and the PID-scoped native test driver. The first debug attempt failed because this Hyprland requires Lua dispatch rather than the old string dispatcher; it was cleaned up and excluded from the successful comparisons.

## Recommended solutions

1. **Immediate, lowest-risk route: use WGPU with the existing patch.** Rebuild this checkout and explicitly select `SCRATCHPAD_RENDERER=wgpu`. No additional runtime source patch was necessary to pass these tests. An installed older Nix package will not inherit the checkout's fix automatically.
2. **Longer-term route: rebase onto upstream eframe 0.36.2 or later with the sleeping-loop fix.** The 0.36.2 WGPU probe passed. Rebase rather than blindly delete the fork: upstream 0.36.2 still lacks the local nonblocking/callback-paced Glow presenter, so deleting the patch can reintroduce the independent Glow startup hang. Full-app and fallback testing is required before a dependency migration.
3. **WGPU has now been promoted to the production default.** Keep `SCRATCHPAD_RENDERER=glow` as an escape route while extending UI/backend coverage. No need to force Glow on Windows or remove the `scratchpad` app ID/workspace rule.

Do not treat forcing WGPU's GL backend, disabling vsync, adding an unconditional repaint timer, or removing workspace routing as the primary fix. Both WGPU backends share the defective scheduler in the control case; sleeping dispatch fixes the cause while preserving pacing.

## Reproduce safely

The presentation probe now honors `SCRATCHPAD_RENDERER` and prints its selected WGPU adapter:

```sh
SCRATCHPAD_RENDERER=wgpu cargo run --release --example presentation_probe
SCRATCHPAD_RENDERER=wgpu WGPU_BACKEND=gl cargo run --release --example presentation_probe
SCRATCHPAD_RENDERER=glow cargo run --release --example presentation_probe
```

It reads no user session. It requests close after five seconds; use an external timeout when deliberately leaving it on a hidden workspace, and move only the probe window to test resumption. Preserve the `scratchpad` app ID so the real workspace-routing rule participates.

For the application, run with isolated `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`, and `TMPDIR` directories. Set `SCRATCHPAD_TRACE_WINDOW=1` to confirm `startup renderer=Wgpu`; `WAYLAND_DEBUG=client` allows ping/pong checks. Keep protocol/session logs local. To use the normal saved session later, save and exit the existing primary first: a second launch forwards to that process and cannot switch its renderer.

## Changes and remaining validation

The investigation added the opt-in probe enhancement and documentation. Subsequent adoption changes `src/main.rs` and its renderer-selection tests to default to WGPU, with matching user documentation. `Cargo.toml`, `Cargo.lock` and the existing eframe patch are unchanged.

Passed: release app/probe builds, formatting, `git diff --check`, all 536 application tests (`cargo test --all-features`), and strict all-target/all-feature Clippy. A final release-app native check with `SCRATCHPAD_RENDERER` **unset** logged `startup renderer=Wgpu`, completed hide/show cycles, answered all seven observed pings, and exited normally. Upstream 0.36.2 was tested in an isolated probe, not substituted into the full app.

Remaining native coverage includes editing/selection/IME, search and split interactions under WGPU, resize/DPI/multi-monitor transitions, suspend/resume and longer idle runs. Windows, X11 and other GPUs were not tested in this investigation. Ordinary headless tests cannot substitute for those native graphics checks.
