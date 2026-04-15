# Settings Responsive Layout Plan

## Goal

Make the Settings surface behave predictably across large desktops, standard laptop-sized windows, and smaller resized windows.

Target behavior:

- preserve the current large-screen presentation with a centered column and a capped maximum width
- introduce a defined minimum settings surface size based on the app's client area, not the full monitor size
- when the window becomes smaller than that minimum, stop compressing the settings layout and expose scrolling instead
- keep all settings reachable and readable without clipped controls or collapsed labels

## Current State

The current settings page already follows the intended large-screen design:

- one centered vertical scroll area in `src/app/ui/settings.rs`
- a maximum page width of `980 px` in `SettingsUi::LAYOUT.page_max_width`
- card and inner-row helpers in `src/app/ui/settings/widgets.rs`

The current implementation does not define a minimum page size. Instead, it keeps shrinking against `ui.available_width()`. That creates three practical problems on narrow windows:

1. The page container only has a max width, so it keeps compressing below laptop-sized dimensions.
2. The page only uses `ScrollArea::vertical()`, so width overflow cannot fall back to horizontal scrolling.
3. Several helpers mix fixed trailing controls with hard minimum text widths, which makes the card layout progressively tighter before any overflow behavior can take over.

Relevant constraints today:

- `page_content_width`: `available_width.min(page_max_width)`
- `page_horizontal_margin`: centers the column but does not account for a minimum page width
- `header_text_width`: `(available_width - 240.0).max(220.0)`
- `row_label_width`: `(available_width - 250.0).max(180.0)`
- fixed control width: `190 px`
- preview width cap: `420 px`

## Proposed Sizing Model

Treat the settings surface as having two viewport thresholds:

- max viewport behavior: same as today, centered content with a `980 px` content cap
- min viewport behavior: keep a minimum internal settings surface and allow overflow scrolling below it

Recommended first pass constants:

- `page_max_width = 980 px` to preserve the current desktop layout
- `page_min_viewport_width = 1180 px`
- `page_min_viewport_height = 720 px`
- `page_side_padding = 24 px`

Why `1180 x 720`:

- it is close to a standard laptop experience after accounting for the app's own chrome, tab strip, and status bar
- it is comfortably above the widths where the current two-column card layout starts feeling compressed
- it leaves room to tune later without changing the design model

Important detail:

- the minimum should be applied to the settings page's internal layout surface, not as an OS-level minimum window size for the whole app

That keeps the editor window resizable while making the Settings surface scroll when there is not enough room.

## Behavior Rules

### Above Max Width

- keep the content column centered
- keep the current `980 px` maximum content width
- keep the existing roomy spacing and card presentation
- no horizontal scrolling

### Between Min And Max

- let the settings page flex with the available client width
- maintain side padding and centered presentation
- keep the card layout in its normal two-column form
- vertical scrolling remains available for page height as it is now

### Below Min Width Or Height

- stop shrinking the internal settings surface below the minimum viewport size
- expose scrolling on the settings page so the user can reach clipped content
- keep controls at usable widths rather than squeezing them further
- prefer `ScrollArea::both()` for the page container so width and height overflow are both covered

This means the settings page should behave like a scrollable canvas once the window drops below the minimum, instead of trying to reflow indefinitely.

## Layout Strategy

### 1. Add Responsive Metrics To `SettingsUi`

Extend `src/app/ui/settings/style.rs` with explicit responsive sizing tokens and helpers.

Suggested additions:

- `page_min_viewport_width`
- `page_min_viewport_height`
- `page_side_padding`
- `page_scroll_gutter` if scrollbar spacing needs to be reserved
- helper methods such as:
  - `page_viewport_size(ui)`
  - `page_surface_width(ui)`
  - `page_surface_min_size()`
  - `is_below_min_viewport(ui)`

The page-level helpers should decide:

- the width of the internal settings surface
- whether the page is in normal mode or overflow-scroll mode
- how much centering margin to apply when there is spare width

### 2. Refactor The Top-Level Page Container

Update `src/app/ui/settings.rs` so the outer page container becomes the responsive boundary.

Planned structure:

1. Measure the current content rect or available size.
2. Compute an internal settings surface size with:
   - width clamped to `page_max_width` on large windows
   - width allowed to flex above the minimum threshold
   - width held at the minimum surface width once the window goes below threshold
3. Wrap the page in `egui::ScrollArea::both().auto_shrink([false, false])`.
4. Inside the scroll area, allocate the internal settings surface at the computed size.
5. Center the content only when the viewport is larger than the surface.

This is the core behavior change that makes resize response deterministic.

### 3. Keep The Existing Large-Screen Look

Do not redesign the Settings page for desktop widths. The large-screen appearance should stay functionally the same:

- same heading placement
- same content cap
- same category spacing
- same card presentation

The responsive work should be an overflow strategy, not a visual redesign.

### 4. Relax Narrow-Width Pressure Inside Cards

Even with page-level scrolling, the card helpers should stop assuming generous width at all times.

Review and adjust these helpers in `src/app/ui/settings/style.rs` and `src/app/ui/settings/widgets.rs`:

- `header_text_width`
- `row_label_width`
- `fixed_width_control`
- `settings_file_card` pill width handling
- preview panel width allocation

Planned changes:

- replace magic subtraction values with helper methods derived from shared tokens
- clamp trailing controls to `min(control_width, available_width)` where appropriate
- ensure text columns wrap instead of fighting fixed-width trailing content
- keep the preview panel from forcing awkward widths when horizontal room is reduced

This work is mainly about preventing ugly compression before the page reaches the hard minimum threshold.

### 5. Introduce A Compact Card Mode Only If Needed

First implement page-level min-size plus scrolling. Then test whether any cards still feel too wide at laptop-sized viewports.

Only if needed, add a second layout mode for rows near the lower bound:

- normal mode: current side-by-side title and trailing control
- compact mode: trailing control stacks below the description or uses a wrapped row

Do not start with a full compact redesign unless manual testing shows that the minimum viewport still feels crowded.

## Implementation Touch Points

Primary files:

- `src/app/ui/settings.rs`
- `src/app/ui/settings/style.rs`
- `src/app/ui/settings/widgets.rs`

Secondary files to verify during implementation:

- `src/app/ui/settings/appearance.rs`
- `src/app/ui/settings/opening.rs`
- `src/app/ui/settings/text_formatting.rs`

Existing resize infrastructure that should be reused rather than replaced:

- `src/app/chrome/resize.rs`

The app already repaints when the viewport content rect changes, so the settings page should be able to respond to window resize without new global resize state.

## Validation Plan

### Manual Resize Cases

Test at minimum these client-area sizes:

1. `1600 x 900` or larger
2. `1366 x 768`
3. `1180 x 720`
4. `1050 x 680`
5. `900 x 600`

Expected results:

1. Large desktop: centered `980 px` page, no horizontal scroll, existing visual rhythm preserved.
2. Standard laptop: no clipped controls, no forced horizontal scroll, comfortable side padding retained.
3. Minimum threshold: page still readable, no visible squeeze artifacts.
4. Below minimum: scrollbars appear and all controls remain reachable.
5. Very small window: settings are still usable through scrolling and no card content becomes inaccessible.

### Functional Checks

- Expandable cards still open and close correctly while inside the scroll area.
- Combo boxes, toggles, and icon buttons remain clickable near scrollbars.
- The settings-file path pill still truncates correctly.
- Preview content remains visible and does not overflow its card unexpectedly.
- Switching between Workspace and Settings continues to work without stale layout state.

### Low-Cost Automated Coverage

Add helper-level tests if the final implementation introduces pure sizing functions, for example:

- width selection above max
- width selection between min and max
- surface sizing below min viewport

UI interaction tests are optional for this change. The first useful automated coverage is likely around the new pure layout helpers.

## Risks

### Scroll Nesting

Some expanded sections already contain content that manages its own width assumptions. Moving to a page-level `ScrollArea::both()` may expose awkward nested-scroll interactions if inner widgets later add scroll areas of their own.

Mitigation:

- keep scrolling centralized at the page level
- avoid adding new inner scroll regions during this change

### Over-Constraining The App Window

Using an OS-level minimum window size would solve the squeeze problem but would also make the overall app less flexible than the request implies.

Mitigation:

- keep the minimum at the settings layout level only

### Hidden Width Couplings

The current card helpers have a few implicit width contracts. If only the page container changes, some rows may still look cramped near the threshold.

Mitigation:

- review the shared widget helpers in the same pass
- avoid leaving magic numbers scattered across individual category renderers

## Recommended Execution Order

1. Add responsive sizing tokens and helper functions in `style.rs`.
2. Refactor `settings.rs` to use a minimum internal surface inside `ScrollArea::both()`.
3. Update shared widget helpers to use the new width helpers instead of fixed subtraction math.
4. Manually test resize behavior at the five target sizes.
5. Add pure helper tests if the sizing math is extracted cleanly.

## Definition Of Done

This work is complete when:

- the Settings page keeps its current large-screen look
- the page behaves comfortably at a standard laptop-sized viewport
- the page stops compressing below the defined minimum size
- scrolling appears below that minimum and preserves access to all content
- no settings card has clipped or inaccessible controls during resize