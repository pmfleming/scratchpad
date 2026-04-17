# Tab Overflow Plan

This document defines the intended behavior when the tab strip becomes wider than the header area available in the window.

It is a planning document, not an implementation spec. The goal is to make the next implementation pass straightforward and avoid re-deciding UX details while working inside `src/app/mod.rs`.

## Current Behavior

The current header layout is divided into three distinct regions:

1. **Fixed Left Region**: Primary action buttons:
   - `Open File`
   - `Save As`
   - `Search`
2. **Fixed Right Region**: Native-style caption buttons (right-to-left):
   - `Close`
   - `Maximize/Restore`
   - `Minimize`
3. **Middle Region**: A flexible area containing:
   - A background **Drag Handle** for window movement.
   - A horizontal `egui::ScrollArea` for the **Tab Strip**.
   - A `New Tab` (+) button, which is currently placed **inside** the scroll area at the end of the tab list.

Today, overflow is handled implicitly by horizontal scrolling. That keeps every tab reachable, but it has a few weaknesses:

- The `New Tab` button scrolls away with the tabs instead of remaining fixed/accessible.
- The active tab is not guaranteed to stay in view automatically.
- Large tab counts reduce scanability because the strip becomes one long unstructured row.
- There is no compressed overview of hidden tabs.

## Design Goals

- Keep `Open File`, `Save As`, `Search`, and `New Tab` visible at all times.
- Ensure the active tab remains visible whenever selection changes.
- Preserve one-click access to nearby tabs.
- Provide a fast way to reach distant tabs without excessive horizontal scrolling via an overflow dropdown.
- Keep the solution Windows-first and egui-native.

## Proposed Behavior

Use a hybrid overflow model:

1. The non-caption action buttons remain fixed and never scroll away:
   - `Open File`
   - `Save As`
   - `Search`
   - `New Tab`
2. The tab strip remains horizontally scrollable for direct manipulation.
3. When tabs exceed the visible width, show overflow affordances on the tab strip itself.
4. Add a tab list dropdown as a secondary access path for all open tabs.

This keeps the main interaction simple while avoiding a brittle “shrink every tab forever” approach.

## Layout Plan

Split the current middle header area into two subregions:

1. A flexible tab viewport
2. A fixed action area containing:
   - `Save As`
   - `Search`
   - `Open File`
   - `New Tab`
   - optional overflow dropdown button

The tab viewport should consume only the width left after fixed controls are measured.

Implementation direction:

- Move the action buttons outside the scrolling `ScrollArea`
- Render tabs inside a bounded-width container
- Measure the fixed action area before laying out the tab viewport
- Use a stable overflow measurement rule so the layout does not oscillate at the fit boundary:
  - either always reserve overflow-button width during the overflow check
  - or perform a second measurement pass that includes the button width before finalizing layout

## Overflow Threshold

Overflow should be considered active when:

- The total width of all rendered tab buttons plus spacing exceeds the effective tab viewport width after fixed controls are measured
- The effective width used for that check must include any reserved overflow-button width per the layout rule above

The check should be based on measured or estimated tab widths, not just tab count.

That matters because:

- File names vary significantly in width
- Dirty markers slightly change width
- Narrow windows can overflow with only a few tabs

## Tab Sizing Rules

Use bounded tab widths rather than fully dynamic shrinking.

Recommended rules:

- Preferred tab width: current visual width
- Maximum tab width: unchanged from current design intent
- Minimum tab width: enough to show:
  - dirty marker if present
  - truncated file name
  - close button

When the strip becomes crowded:

1. Tabs may shrink down to the minimum width
2. After that point, horizontal overflow behavior takes over
3. Do not shrink tabs below the point where the close button becomes hard to hit

## Active Tab Visibility

When the user:

- opens a file
- creates a tab
- selects a different tab
- restores a previous session

the active tab should auto-scroll into view.

Expected behavior:

- If the active tab is already fully visible, do nothing
- If it is partially or fully clipped, scroll just enough to reveal it
- Favor keeping a small leading and trailing margin around the selected tab

This should be treated as required behavior, not a later enhancement.

## Overflow Dropdown

Add a dedicated overflow button when not all tabs fit in the visible tab viewport.

Recommended placement:

- Immediately to the left of `Open File`

Recommended behavior:

- Clicking opens a popup menu listing all open tabs
- Each row shows:
  - dirty indicator if present
  - file name or untitled label
  - path context or another secondary label when names collide
  - active-state highlight
- Selecting a row activates that tab and scrolls it into view

Optional follow-up behavior:

- Include `Close`
- Include `Close Others`
- Include `Reveal Path` or copy-path behavior later if needed

The first implementation only needs activation.

## Scrolling Behavior

Mouse wheel and trackpad horizontal gestures should move the tab strip when the pointer is over the header tab region.

Recommended behavior:

- Shift + vertical wheel may map to horizontal movement if needed
- Scroll speed should feel deliberate, not accelerated
- Scrolling should not interfere with editor zoom behavior in the central panel

If egui makes custom wheel routing awkward, default horizontal `ScrollArea` behavior is acceptable for the first pass.

## Drag Region Rules

The unused space in the header currently starts window drag.

That should remain true, but with clearer ownership:

- Tab buttons are interactive
- Overflow button is interactive
- `Open File`, `Save As`, `Search`, and `New Tab` are interactive
- Only truly unused header space starts window drag

Implementation note:

- The drag region should be allocated only after interactive widgets claim their space
- Do not use a single pre-allocated hitbox that spans the future tab viewport and action area, because that can steal pointer input from the `ScrollArea`, tab buttons, and overflow button

## Session and Restore Expectations

The overflow solution should not change session semantics.

After session restore:

- Restored tabs load in the same order
- Restored active tab becomes selected
- The tab strip scroll position should be recalculated from the active tab rather than persisted

Persisting raw scroll offset is not recommended in the first implementation because it is fragile across window-size changes.
The implementation should carry only transient UI state needed to reveal the active tab during the current frame sequence; it should not be added to session persistence.

## Accessibility and Usability Notes

- Tooltips should show the full tab name when the visible tab label is truncated
- Close buttons must remain clickable at minimum tab width
- The overflow list should be keyboard reachable
- The active tab in the overflow list should be visually distinct

## Proposed Improvements & Refinements

Based on the initial design, several enhancements can further improve the tab management experience:

### 1. Visual Scroll Indicators (Affordances)
- **Gradient Fades**: When the tab strip is overflowed, add subtle gradient fades (matching the header background color) to the left and/or right edges of the scrollable area. This provides a clear visual cue that more tabs are available in that direction.
- **Dynamic Shadows**: Alternatively, use a small shadow or border that appears only when there is content to scroll toward.

### 2. Enhanced Interaction Patterns
- **Middle-Click to Close**: Allow users to close tabs by middle-clicking anywhere on the tab button, not just the small 'x' icon. This is a standard power-user feature in modern browsers and editors.
- **Drag-to-Scroll**: Enable "hand-style" dragging of the tab strip (clicking and dragging the empty space between tabs) to scroll, making it easier to navigate without a scroll wheel or trackpad.

### 3. Refined Tab Sizing (Flex-Behavior)
- **Proportional Shrinking**: Instead of just minimum/maximum widths, implement a "flex-shrink" logic where tabs gradually shrink as more are added, but prioritize keeping the *active* tab slightly wider or more legible than inactive ones.
- **Minimum Width for Close Button**: Ensure that even at the smallest width, the close button does not overlap with the text in a way that makes it look "broken." If a tab is too narrow, hide the text entirely and show only the icon/dirty marker.

### 4. Advanced Overflow Menu Actions
- **Bulk Actions**: Add "Close All," "Close Others," and "Close Tabs to the Right" to the overflow dropdown or a right-click context menu on tabs.
- **Search-in-Tabs**: If the tab count is very high (e.g., >20), add a small filter/search box at the top of the overflow dropdown to quickly find a specific file.

### 5. Context Menu Integration
- **Right-Click Menu**: Implement a standard context menu for each tab (both in the strip and the overflow list) with options like "Copy Path," "Reveal in Explorer," and the bulk close actions mentioned above.

## Implementation Phases

### Phase 1

- Keep horizontal scrolling
- Move `Open File`, `Save As`, `Search`, and `New Tab` out of the scroll area
- Add auto-scroll-to-active behavior
- Add tooltips for truncated tab labels if needed

This phase fixes the biggest usability issue with minimal structural risk.

### Phase 2

- Detect overflow explicitly
- Add overflow dropdown button and tab list popup
- Ensure selecting from the popup scrolls the tab into view

This phase improves navigation for large tab counts.

### Phase 3

- Tune tab width heuristics
- Add optional overflow menu actions like close/close others
- Add richer keyboard navigation if desired

This phase is polish, not a prerequisite.

## Suggested Refactor Boundaries

To keep `mod.rs` from growing further, the overflow work should likely be split into helpers.

Recommended extraction targets:

- `chrome.rs`
  - helper for measuring/rendering fixed header actions
  - overflow button helper
- `tabs.rs`
  - tab label truncation helpers
  - tab width policy helpers
- `mod.rs`
  - state orchestration
  - selection and scrolling decisions
  - transient UI state such as pending scroll-to-active requests or overflow-anchor state

## Acceptance Criteria

The overflow work is complete when:

- `Open File`, `Save As`, `Search`, and `New Tab` remain visible regardless of tab count
- Active tab selection always leaves the active tab visible
- Overflowed tabs are still reachable without excessive manual scrolling
- Close buttons remain usable on visible tabs
- Window dragging still works from unused header space
- `cargo fmt`, `cargo clippy`, and `cargo test` still pass
