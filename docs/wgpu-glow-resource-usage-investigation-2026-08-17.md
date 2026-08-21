# WGPU vs Glow Resource Usage Investigation

Date: 2026-08-17

## Executive summary

Scratchpad consumed approximately one full CPU core while idle when using eframe's WGPU renderer, but approximately 0% CPU with Glow. The difference was not caused by editor rendering, search, caret animation, background I/O, Vulkan shader work, or a recent WGPU upgrade.

The issue is a redraw-scheduling failure involving:

1. Scratchpad's Wayland app ID, `scratchpad`
2. a Hyprland rule that routes that class to workspace 5 during initial mapping
3. eframe/winit switching the event loop to `ControlFlow::Poll` before requesting a redraw
4. the expected redraw event not arriving after WGPU's second startup frame

Once that redraw is withheld, eframe remains in non-blocking polling mode. The main thread repeatedly polls epoll and timer file descriptors instead of sleeping, saturating one CPU core even though Scratchpad's UI code is no longer being called.

Glow does not enter the same stuck state. Its EGL/glutin startup path produces enough additional window/redraw activity for Scratchpad to complete startup and return the event loop to a blocking wait.

The production mitigation is to use Glow by default on Linux while retaining `SCRATCHPAD_RENDERER=wgpu` as an explicit override.

## User-visible symptoms

With WGPU:

- idle CPU stayed at approximately 99-100% of one core
- the main Scratchpad thread remained runnable
- memory stayed broadly stable around 85-107 MiB, depending on the selected WGPU backend and loaded session
- disk I/O did not grow
- background threads were mostly asleep

With Glow:

- idle CPU fell to approximately 0%
- the main thread slept normally
- memory remained around 101-104 MiB
- I/O remained negligible

This was therefore primarily a CPU scheduling problem, not excessive memory allocation, file activity, or useful rendering work.

## Environment

The investigation was performed with:

- Linux on Wayland
- Hyprland 0.55.4
- AMD HawkPoint1 graphics
- Mesa 26.1.x
- eframe/egui 0.36.1
- WGPU 30.0.0
- winit 0.30.13
- a 1920 x 1200 display at approximately 60 Hz and scale 1.25

The older eframe 0.35.0 / WGPU 29.0.3 build was also tested to determine whether the recent dependency update introduced the problem.

## Initial runtime measurements

The original release build was started and sampled once per second for 30 seconds.

Observed WGPU steady state:

- CPU: approximately 98-100% every sample
- RSS: approximately 103.6 MiB
- threads: 25
- file descriptors: 46
- read growth: 0 MiB during the sample
- write growth: 0 MiB during the sample
- process state: runnable

The equivalent Glow run remained at approximately 0% CPU, approximately 102-103 MiB RSS, and a sleeping process state.

## Syscall evidence

An eight-second WGPU `strace -f -c` sample showed very high event-loop activity, including approximately:

- 48,000 `epoll_pwait` calls
- 48,000 `timerfd_settime` calls
- 96,000 `epoll_ctl` calls
- 50,000 `read` calls, most returning no useful event

A focused per-thread trace showed that the main thread dominated the activity. Its repeating loop was equivalent to:

```text
epoll_pwait(..., timeout=0) = 0
read(...) = -1 EAGAIN
epoll_ctl(...)
timerfd_settime(...)
epoll_ctl(...)
```

The zero epoll timeout is the important detail: the event loop was polling rather than blocking.

## Frame-delivery evidence

Temporary frame instrumentation was added to `ScratchpadApp::ui` during the investigation and removed afterward.

Glow delivered at least three startup frames:

```text
frame 1
frame 2
frame 3
```

It then completed startup and slept.

WGPU delivered only two startup frames:

```text
frame 1
frame 2
```

After frame 2, CPU remained at approximately 100%, but `ScratchpadApp::ui` was not called again. This proves the CPU was not being spent in Scratchpad's editor, tab layout, search, persistence, caret animation, or frame rendering code.

An eframe trace showed the final WGPU sequence:

```text
request_repaint_callback: delay=0
UserEvent::RequestRepaint scheduling repaint
EventResult::RepaintAt(...)
request_redraw for WindowId(...)
```

No corresponding `WindowEvent::RedrawRequested` followed.

The Glow trace continued with additional `RepaintNext` / `RepaintNow` activity and delivered the third frame.

## Minimal reproduction and isolation

A temporary minimal eframe application was built using the same dependency stack. The probe displayed only one label and performed no Scratchpad work.

### Baseline renderer comparison

| Probe configuration | Idle CPU | Result |
| --- | ---: | --- |
| Minimal Glow app | 0% | Sleeps normally |
| Minimal WGPU app | 0% | Sleeps normally |

This ruled out the theory that WGPU, Vulkan, Mesa, or eframe always busy-polls on this machine.

### App ID comparison

The probe was then run with different Wayland app IDs.

| WGPU probe app ID | Hyprland behavior | Idle CPU | Delivered startup frames |
| --- | --- | ---: | ---: |
| no explicit app ID | no Scratchpad class rule | 0% | 4+ |
| `scratchpad2` | no matching Scratchpad rule | 0% | 4+ |
| `scratchpad` | matched dedicated-workspace rule | 99-100% | 2 |

Setting only the app ID to `scratchpad` was sufficient to reproduce the failure in the minimal application.

Other app classes with explicit Hyprland workspace routing also reproduced the two-frame, full-core behavior when their rule routed initial mapping away from the current placement path. This established compositor workspace routing, rather than the literal string `scratchpad`, as the trigger.

## Relevant Scratchpad configuration

Scratchpad deliberately sets its Wayland app ID in `src/main.rs`:

```rust
.with_app_id("scratchpad")
```

This allows desktop integration and Hyprland matching by class.

The active Hyprland configuration contained:

```lua
hl.window_rule({
    match = { class = "^(scratchpad)$" },
    workspace = "5 silent",
})
```

The repository's Hyprland documentation and Home Manager module recommend the equivalent dedicated-workspace rule.

The rule is useful and should not need to be removed merely to make a renderer idle correctly.

## Event-loop mechanism

The relevant behavior is in eframe's native event-loop wrapper.

When a repaint time becomes due, eframe effectively does the following:

```rust
event_loop.set_control_flow(ControlFlow::Poll);
window.request_redraw();
```

The design expects a later redraw event to run the UI and return `EventResult::Wait`, which changes the event loop back to a blocking wait.

winit's own `request_redraw` documentation does not guarantee that the redraw event arrives in the same or next event-loop iteration. On Wayland, redraw delivery is aligned with compositor frame callbacks after `pre_present_notify`.

In this failure mode:

1. Hyprland applies the class-based workspace placement during initial mapping.
2. WGPU presents the first startup frames.
3. Scratchpad/egui requests another immediate repaint.
4. eframe switches to `ControlFlow::Poll` and calls `request_redraw`.
5. The expected redraw event is withheld or lost in the initial workspace/surface transition.
6. No event handler returns `EventResult::Wait`.
7. winit continues polling with a zero timeout indefinitely.

This explains both the high CPU and the lack of further Scratchpad frames.

## Why Glow differs

Both renderers use the same Scratchpad model and egui UI. Their native surface integrations differ:

- WGPU uses egui-wgpu and WGPU surface presentation.
- Glow uses glutin/EGL and swaps the OpenGL surface through the Glow integration.

In the observed trace, the Glow path generated additional window/redraw activity during startup. That activity allowed the third Scratchpad frame to run, after which the event loop returned to a sleeping state.

The WGPU path did not receive the equivalent redraw after frame 2. Because eframe had already selected `ControlFlow::Poll`, the missing event became a permanent busy loop.

Glow's low resource use is therefore not because OpenGL draws Scratchpad dramatically more efficiently. It avoids the redraw starvation state and lets the process sleep when no work is pending.

## Hypotheses ruled out

### Editor or search workload

Ruled out because `ScratchpadApp::ui` stopped being invoked while CPU remained at 100%.

### Caret animation

Ruled out for the same reason. The busy period occurred outside delivered Scratchpad frames.

### Background I/O or persistence

Ruled out by stable disk counters, sleeping worker threads, and the minimal eframe reproduction.

### Excessive GPU rendering

Ruled out because no additional UI frames were delivered during the full-core busy loop.

### Vulkan-specific driver behavior

Ruled out because forcing WGPU's OpenGL backend still produced approximately 99% CPU in the failing placement scenario.

### Recent eframe/WGPU upgrade

Ruled out because a separately built eframe 0.35.0 / WGPU 29.0.3 version reproduced the same behavior.

### General WGPU incompatibility on the machine

Ruled out because a minimal WGPU application without the matching workspace-routed class idled at 0% CPU.

## Production mitigation

Scratchpad now selects Glow by default on Linux.

WGPU remains available explicitly:

```sh
SCRATCHPAD_RENDERER=wgpu scratchpad
```

The renderer choice is implemented through a pure selection helper with tests covering:

- Linux defaulting to Glow
- explicit WGPU selection
- case-insensitive explicit Glow selection

Documentation was updated in:

- `README.md`
- `docs/configuration.md`

Post-change validation showed:

- default Linux backend: Glow
- idle CPU: 0.00%
- RSS: approximately 103.6 MiB
- process state: sleeping
- renderer selection tests: 3 passed
- release build: successful

## Why the mitigation is appropriate

Scratchpad is a text workspace and does not currently depend on WGPU-specific rendering features. Glow provides the required egui rendering behavior and stable idle scheduling on the supported Linux/Hyprland path.

Changing or removing the `scratchpad` app ID would weaken desktop integration and break recommended compositor rules. Removing the dedicated-workspace rule would change intended user behavior. Defaulting to Glow avoids both regressions.

## Potential upstream fix

The durable fix belongs in eframe's redraw scheduling rather than Scratchpad's editor code.

An upstream fix should ensure that eframe does not remain in `ControlFlow::Poll` indefinitely while waiting for a redraw event that a compositor may defer. Possible directions include:

- return to `ControlFlow::Wait` after issuing `request_redraw`
- retain a bounded fallback wakeup instead of unbounded polling
- track outstanding redraw requests explicitly
- handle occluded, unmapped, or workspace-routed surfaces similarly to invisible/minimized windows
- add a regression test for a requested redraw that is not immediately delivered

A minimal upstream reproduction consists of:

1. an eframe app using WGPU
2. a stable Wayland app ID
3. a Hyprland rule that routes that class to another workspace during mapping
4. one or more immediate startup repaint requests
5. observation that only two frames arrive while the event loop stays in `ControlFlow::Poll`

## Final conclusion

The large resource difference between WGPU and Glow was caused by event-loop state, not rendering throughput.

WGPU entered a redraw-starvation condition during Hyprland's class-based workspace placement. eframe waited for a redraw while leaving winit in non-blocking poll mode, saturating one CPU core. Glow's native integration delivered enough startup events to complete initialization and sleep normally.

Using Glow as the Linux default is the safest application-level mitigation while preserving an explicit WGPU override and the existing Hyprland desktop behavior.
