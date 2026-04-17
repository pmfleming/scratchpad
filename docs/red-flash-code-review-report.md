# Red Flash During Editor Layout Changes

## Scope
This report is based on code review only, plus comparison against the existing `docs/red-flash-report.md`.

Reviewed areas:
- `src/main.rs`
- `src/app/app_state/frame.rs`
- `src/app/app_state/settings_state.rs`
- `src/app/app_state/settings_state/mutators.rs`
- `src/app/ui/editor_area/*`
- `src/app/ui/tab_strip/*`
- `src/app/ui/tile_header/*`

I did not use other design/planning docs.

## Executive Summary
The most likely explanation is not a single bug, but a short-lived exposure of an unowned or differently-owned background during layout transitions. The strongest code-backed risks are:

1. No explicit native/window clear color.
2. No explicit workspace-wide base fill before pane rendering.
3. Edge ownership changes when tab-strip position/layout mode changes.
4. Repaint/transition coverage is narrower than the set of layout-changing actions.
5. Split gutters are intentionally wider than the divider line, so they depend on the parent background staying visually stable.

## Top 5 Likely Causes And Simplest Solutions

### 1. Missing explicit native clear color
**Why this is likely**
- `src/main.rs:23-29` builds `eframe::NativeOptions` but does not set a clear color.
- The app uses a borderless custom window via `.with_decorations(false)` in `src/main.rs:24-27`.
- If a frame is cleared before egui repaints all regions, the user can briefly see the backend/native clear color at the window edge.

**Simplest solution**
- Set the native clear color to the same dark base used by the editor/workspace.

**Why this ranks #1**
- It directly explains why the flash appears at the edge of the screen, not just inside the editor.

### 2. The workspace surface is not explicitly painted before pane layout
**Why this is likely**
- `src/app/ui/editor_area/mod.rs:15-42` uses `egui::CentralPanel::default().show_inside(...)`.
- The code computes `workspace_rect` (`src/app/ui/editor_area/mod.rs:24-27`) and renders panes into it, but never paints that whole rect first.
- Individual tiles do paint themselves (`src/app/ui/editor_area/tile.rs:49-54`, `147-166`) and editor content also fills its background (`src/app/ui/editor_content/mod.rs:35-38`), but any temporary gap between pane-tree recomputation and tile repaint falls back to the panel/root background instead of the intended workspace background.

**Simplest solution**
- Paint `workspace_rect` with the workspace/editor background before rendering any panes.

**Why this ranks #2**
- It is a clean explanation for flashes that happen during split changes, tab combine/split operations, and other structure edits inside the central area.

### 3. Edge ownership changes when tab-strip layout mode changes
**Why this is likely**
- `src/app/app_state/frame.rs:51-59` conditionally renders very different chrome layouts:
  - top header when tabs are on top,
  - only a floating top-drag button when tabs are left/right (`src/app/ui/tab_strip/top_drag/button.rs:9-27`),
  - optional bottom tab bar,
  - optional vertical side panel.
- For left/right tab positions, the top edge is not owned by a persistent panel; it is sometimes just a foreground `Area` button (`src/app/ui/tab_strip/top_drag/button.rs:21-25`).
- Vertical tab panels can collapse to a 6 px auto-hide peek (`src/app/ui/tab_strip/layout.rs:8`, `193-199`, `272-281`; `src/app/ui/tab_strip/panels.rs:29-48`).

**Simplest solution**
- Add an always-present root background or persistent edge panel so the top/side edges remain painted even while layout mode is switching.

**Why this ranks #3**
- The bug description specifically mentions moving tab positions around, and this is the most code-specific explanation for edge-localized artifacts during that operation.

### 4. Transition/repaint handling does not cover all structure-changing operations
**Why this is likely**
- `begin_chrome_transition()` only sets a two-frame flag (`src/app/app_state/frame.rs:79-95`).
- That transition is triggered for tab-list settings changes like position, auto-hide, and width (`src/app/app_state/settings_state/mutators.rs:155-167`, `186-195`, `216-226`).
- But other layout-shaping actions do not start that transition:
  - split creation (`src/app/commands.rs:143-156`)
  - split resizing (`src/app/commands.rs:134-141`)
  - display-tab reordering (`src/app/commands.rs:129-132`)
  - settings surface open/close (`src/app/app_state/settings_state/mutators.rs:229-247`)
- Existing transition logic mostly suppresses interactive chrome, not visual instability (`src/app/ui/transition.rs:16-20`).

**Simplest solution**
- Request immediate repaint and/or start the same transition path for every operation that changes panel topology or pane geometry, not just tab-list setting changes.

**Why this ranks #4**
- The code already acknowledges that chrome transitions need special handling, but it applies that protection to only part of the problem space.

### 5. Split gutters are intentionally wider than the painted divider
**Why this is likely**
- Pane splits create a 6 px gap via `TILE_GAP` (`src/app/ui/tile_header/split.rs:8`).
- `split_rect` leaves that gap open between sibling tiles (`src/app/ui/editor_area/divider.rs:140-157`).
- The divider paint only fills a 2 px center line (`DIVIDER_VISUAL_THICKNESS`) plus the handle (`src/app/ui/editor_area/divider.rs:6-9`, `91-109`, `111-130`).
- That means roughly 4 px of the split gutter is effectively the parent/background surface, not tile paint.

**Simplest solution**
- Paint the full split gutter with a stable background color, or slightly overlap tile backgrounds into the gutter.

**Why this ranks #5**
- This is a real code-level exposure path, but it is more likely to explain flashes near split boundaries than flashes exactly on the outer screen edge.

## Comparison With The Existing Report

### Where the existing report looks strong
**1. Clear color**
- I agree strongly.
- Your item 1 is well supported by `src/main.rs:23-29`.
- I would keep this as a top-tier cause.

**2. Missing base fill behind layout changes**
- I agree with the direction.
- Your item 3 maps well to `src/app/ui/editor_area/mod.rs:15-42`.
- I would phrase it a bit more broadly as "workspace-wide base fill is missing", because the issue is not only `CentralPanel`; it is the lack of a guaranteed paint pass for the whole workspace rect before child layout runs.

### Where the existing report is directionally right but the code suggests a different framing
**3. Split-gap issue**
- Your item 4 blames sub-pixel rounding.
- The code shows a stronger and simpler explanation: the split gap is explicit, not just fractional.
- `src/app/ui/editor_area/divider.rs:140-157` leaves a 6 px gutter, and only `src/app/ui/editor_area/divider.rs:91-109` paints a 2 px divider line.
- So I agree there is a gap-related issue, but it is better described as "an intentionally exposed gutter depends on stable parent background", not just "rounding error".

**4. Delayed follow-up repaint**
- Your item 5 is partially supported.
- I do think repaint timing matters, but the code suggests the gap is broader than focus/layout sync for `TextEdit`.
- The stronger code-backed finding is that layout-changing actions are inconsistently covered by transition/repaint handling.

### Where the existing report looks weaker after code review
**5. ScrollArea as a primary cause**
- This looks weaker than the report suggests.
- The main editor scroll area already uses `.auto_shrink([false, false])` in `src/app/ui/editor_area/tile.rs:202-205`.
- Tiles already paint a solid frame before the scroll area (`src/app/ui/editor_area/tile.rs:49-54`, `147-166`), and editor content also fills its own background (`src/app/ui/editor_content/mod.rs:35-38`).
- I would not rank `ScrollArea` behavior in the top five by itself. It may still contribute, but the code already contains several mitigations.

## Final Integrated Recommendation List
If the team wants the simplest, highest-yield sequence, I would combine both reports into this order:

1. Set an explicit native clear color that matches the workspace/editor base color.
2. Paint the full workspace rect before any pane/tree rendering begins.
3. Add a persistent root or edge background so top/side edges remain covered while tab-strip position modes change.
4. Trigger repaint/transition handling for all topology-changing actions, not just tab-list preference changes.
5. Paint the full split gutter, or overlap pane backgrounds enough that the gutter cannot reveal a different parent/background color.

## Bottom Line
Your report identified two of the strongest causes correctly: missing clear-color control and missing base fill during relayout. After reviewing the code, I would replace "ScrollArea delay" with "edge ownership changes during tab-layout mode switches", and I would refine "sub-pixel split gaps" into the more concrete finding that the code intentionally leaves most of the split gutter unpainted by the divider itself.
