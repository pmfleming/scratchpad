# Visible Window Cursor Follow Report

Date: 2026-04-26

## Question

How does the current editor implementation keep the visible area aligned with the cursor, especially for arrow-key movement, mouse-wheel scrolling, mouse clicks, and programmatic location jumps? Which parts of the older cursor-follow concern are already fixed, and what still remains?

## Summary

The main architectural concern described in the earlier report has mostly been addressed in the current implementation.

The editor now has a shared cursor-reveal model on `EditorViewState` with two explicit modes:

- `KeepVisible` for ordinary cursor movement and editing
- `Center` for explicit navigation such as search/location jumps

The visible-window path is also cursor-aware before paint. If a reveal is pending, or if a pending cursor exists, the visible line window is selected around that cursor before the galley is rendered. Reveal requests are only consumed after a stable frame where reveal was attempted, so the old one-frame-drop behavior is no longer the primary failure mode.

Edge-drag selection autoscroll is now implemented as well. Active primary-button selection drags continue to track after the pointer leaves the editor response, and the tile scroll layer applies a dedicated edge autoscroll step while that drag is active near the viewport boundary.

Mouse-wheel scrolling is intentionally separate from cursor-follow. It scrolls the viewport without relocating the cursor, which is normal editor behavior and should not be treated as a cursor-follow bug.

## Current Implementation Status

### 1. Cursor reveal is centralized

`EditorViewState` stores both `scroll_to_cursor` and a `CursorRevealMode`.

That means cursor-follow is no longer just an incidental side effect of one input route. The editor can explicitly request either:

- keep the caret visible
- center the destination for explicit navigation

This is a significant improvement over the older model described in the previous report.

### 2. Pending cursor jumps are visible-window aware before paint

The visible-window editor no longer depends on a stale window when a jump lands outside the current rendered slice.

Two parts of the current code work together here:

- pending cursor synchronization promotes `pending_cursor_range` into `cursor_range` and requests `Center`
- visible-window selection treats either a reveal request or a pending cursor as a reason to build a cursor-centered line window before paint

That means search result activation, jump-to-line style behavior, and similar location jumps are now handled by a cursor-aware visible-window selection step rather than hoping the old layout already contains the destination.

### 3. Reveal requests are durable enough across frames

The older report described `scroll_to_cursor` as a one-frame request that could be lost if the cursor was not paintable on that frame.

That is no longer an accurate description of the main path. The current reveal consumption logic clears the request only when one of these is true:

- no document change occurred and reveal was attempted, or
- no document change occurred and no reveal request remains

In practice, document-changing frames keep the reveal request alive until a stable frame can paint the fresh layout. There are also focused tests covering this behavior.

### 4. Scroll ownership is per view, which is correct for split panes

Each editor view owns its runtime scroll offset. That keeps cursor reveal and viewport updates local to the active tile instead of assuming a shared scroll position across split views.

This matches the expected behavior for multi-pane editing.

## Route Audit

### Arrow keys

Arrow-key movement is currently wired into cursor-follow.

In both the full-editor and visible-window editor paths, keyboard input updates `view.cursor_range`. After input handling, if the cursor changed, the editor requests `KeepVisible`. That means ordinary cursor navigation should continue to follow the caret instead of leaving it outside the comfortable viewport band.

This part of the earlier report is outdated: arrow-key movement is no longer the primary weak case.

### PageUp and PageDown

Page navigation has a dedicated viewport-scroll path in addition to the cursor update.

The current implementation computes a page-sized vertical delta from the viewport and row height, applies that delta to the view scroll offset, and also updates the cursor through the keyboard movement pipeline. That is the right separation for page navigation.

### Typing, paste, and edit keys

Typing, paste, Enter, Backspace, Delete, undo, redo, cut, and other editing routes update the cursor and then rely on the same post-input `KeepVisible` reveal request.

This is also stronger than the earlier report suggested. Text mutation is not relying on an unrelated scroll side effect.

### Mouse clicks inside the editor

Normal mouse click relocation is handled correctly.

For the full-editor path, a click updates `view.cursor_range`, and the post-input cursor-change check requests `KeepVisible`.

For the visible-window path, click relocation is even more explicit:

- the clicked cursor is written into `cursor_range`
- the same range is copied into `pending_cursor_range`
- `KeepVisible` is requested immediately

That extra `pending_cursor_range` step is important because it ensures the next visible-window selection can rebuild around the clicked destination if needed.

So ordinary click relocation should be considered fixed in the current implementation.

### Search results and other external location jumps

Programmatic jumps are handled as explicit navigation, which is the correct policy.

Search result activation and other workspace mutation paths write the destination into `pending_cursor_range` and request `Center`. That produces a cursor-centered visible window before paint and a centered reveal policy afterward.

This was the major weakness in the older report, and it is the area that has most clearly improved.

### Mouse-wheel scrolling

Mouse-wheel scrolling does not relocate the cursor, and it should not.

The tile scroll container applies wheel delta directly to the view-owned scroll offset before render. No cursor reveal is requested, because wheel scrolling is viewport-first input, not caret relocation.

This behavior is correct. If the user scrolls away from the caret with the wheel, the editor should not immediately snap back to the cursor.

### Mouse drag selection near the viewport edge

Edge-drag selection now has a dedicated autoscroll path.

Selection dragging continues to track while the primary-button drag remains active, even after the pointer leaves the editor response rect. While that drag is active, the tile scroll layer applies a bounded edge autoscroll delta near the viewport boundary so the selection can continue growing beyond the currently visible region.

This closes the main interaction gap that remained in the previous revision of this report.

## What Was Outdated In The Earlier Report

The following statements are no longer accurate as the primary description of the implementation:

- The visible-window path chooses the render window before handling cursor synchronization in a way that commonly leaves jumps on the old region.
- `scroll_to_cursor` is effectively a one-frame reveal request that is routinely lost.
- Arrow keys and ordinary click relocation are the fragile cases.

Those were reasonable concerns earlier, but the current code now has:

- centralized reveal intent
- pending-cursor-aware visible-window selection
- durable reveal consumption across changed frames
- explicit center-vs-keep-visible policies per route

## Current Best-Practice Policy

The current code is closest to the following policy, which still looks right:

- Arrow keys and normal editing should request `KeepVisible`
- Explicit jumps should request `Center`
- Mouse clicks inside the viewport should relocate the caret and keep it visible without unnecessary viewport jumps
- Mouse-wheel input should scroll the viewport only
- Edge-drag selection should continue tracking and autoscroll near viewport boundaries

## Conclusion

The earlier report correctly identified a design problem, but it overstates the current risk. The present implementation already fixes the most important parts of cursor-follow:

- arrow-key movement is tied to reveal
- ordinary click relocation is tied to reveal
- external jumps are centered before paint in visible-window mode
- reveal requests persist across edit frames until they can be applied
- selection drag near the viewport edge now continues tracking and autoscrolls as the pointer approaches or moves beyond the viewport boundary

The route-specific cursor-follow issues called out in the earlier report are now addressed for arrows, clicks, search jumps, and edge-drag selection. Any follow-up work should be based on fresh behavior testing rather than the older architectural concern.