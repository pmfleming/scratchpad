# Tab Dragging and Combining Plan

This document defines the intended behavior for direct tab manipulation in Scratchpad.

The scope is broader than simple tab reordering. It covers:

- dragging tabs to reorder them in the strip
- dragging tabs across overflowed tab lists
- combining one tab into another workspace tab
- the model changes required to make tab combining real instead of cosmetic

It is a planning document, not an implementation spec.

## Problem

The current tab strip supports activation, closing, horizontal scrolling, and an overflow menu, but tabs are still static objects.

That leaves several missing workflows:

- users cannot reorder tabs by drag
- overflowed tabs are harder to organize than they should be
- one workspace tab cannot absorb another tab by direct manipulation
- the current data model still treats one `WorkspaceTab` as owning exactly one `BufferState`, which blocks true multi-buffer tab combining

The result is that the app supports more pane behavior inside a tab than it supports between tabs.

## Current State

### Tab Strip

`src/app/ui/tab_strip.rs` currently provides:

- tab activation
- close requests
- horizontal scrolling
- overflow access
- `New Tab`

It does not provide:

- drag state for tabs
- reorder previews
- drop targets
- combine behavior

### Workspace Model

`src/app/domain/tab.rs` currently defines `WorkspaceTab` like this in practice:

- one `BufferState`
- one or more `EditorViewState`
- one pane tree (`root_pane`)
- one `active_view_id`

That means current split panes are multiple views into the same buffer, not multiple buffers inside one workspace tab.

This matters because a real “combine tabs” feature means one source tab must be absorbed into another target workspace. That cannot be implemented cleanly while `WorkspaceTab` still owns only one buffer.

## Design Goals

1. Make tab order directly manipulable by drag.
2. Keep drag behavior predictable in crowded and overflowed tab strips.
3. Support combining one tab into another workspace through a visible drop gesture.
4. Avoid fake combining behavior that only rearranges labels without merging workspace state.
5. Keep transient drag UI state out of session persistence.
6. Sequence implementation so reorder can land before full combine support.

## Terminology

### Reorder

Move a tab to a different position in the top-level tab strip.

### Combine

Drop one source tab onto a target tab so the source workspace is absorbed into the target workspace instead of remaining a separate top-level tab.

### Workspace Tab

The top-level tab visible in the tab strip.

### Drag Preview

The temporary visual state shown while a tab is being dragged, including insertion markers and combine targets.

## User Interaction Model

## 1. Reordering Tabs

Expected behavior:

- pointer down on a tab starts a pending drag gesture
- a small movement threshold prevents accidental reorders from clicks
- once the threshold is crossed, the tab enters drag mode
- dragging left or right across the strip shows a clear insertion marker
- releasing commits the new order

Important details:

- clicking without crossing the threshold still activates the tab normally
- close buttons must keep their current behavior and should not start tab drag
- dragging the active tab should keep it active after reorder
- the reorder marker should snap between tabs, not float ambiguously over them

## 2. Auto-Scroll While Dragging

When the tab strip is horizontally overflowed, drag should not dead-end at the visible edge.

Expected behavior:

- dragging near the left edge auto-scrolls left
- dragging near the right edge auto-scrolls right
- scroll speed ramps gently based on proximity to the edge
- the dragged tab remains visually anchored to the pointer while the strip moves under it

This is required for practical reordering when many tabs are open.

## 3. Overflow Menu Integration

The overflow menu is already a second access path for tabs. Dragging should eventually work across both the strip and overflow presentation, but the first implementation should be intentionally scoped.

Recommended sequencing:

- first implementation: drag only within the visible strip
- second implementation: allow the overflow list to activate tabs during drag hover so hidden tabs can be brought into view
- third implementation: optional direct drag from overflow list rows

Do not block core reorder on overflow-list drag support.

## 4. Combining Tabs

Combining should be a distinct gesture from simple reorder.

Recommended model:

- dragging across tab gaps means reorder
- dragging onto the body of a target tab means combine intent
- the target tab should visually change to indicate combine mode
- releasing over the target commits the combine

This split between “between tabs” and “onto a tab” gives a clear mental model and avoids mode switches.

## Combine Result

The combined result should be real workspace merging, not a temporary tab group.

Recommended behavior for the first combine implementation:

- source tab is removed from the top-level tab strip
- source buffer becomes part of the target workspace
- target workspace gains a new view or pane for the source content
- the source content becomes focused after combine

Recommended first layout rule:

- combine creates a two-way split in the target workspace
- if the target currently has a single leaf, split that leaf
- if the target already has a pane tree, insert the source into a predictable default location rather than trying to infer an arbitrary best leaf

Good default:

- combine onto a tab produces a vertical split with the source inserted as the new second pane

This should stay deterministic. Users can resize or restructure after the combine.

## 5. Future Combine Targets

After the basic drop-onto-tab combine works, richer drop zones can be added.

Possible later behaviors:

- drop on tab center: merge into target workspace using default split placement
- drop on left half of tab: combine and place source first
- drop on right half of tab: combine and place source second
- drop onto editor surface: combine into a specific pane target instead of the workspace tab shell

These are follow-up features, not phase-one requirements.

## Required Model Changes

## Current Blocker

Today `WorkspaceTab` owns exactly one `BufferState`.

That blocks true tab combining because after a combine, one workspace would need to contain:

- multiple buffers
- multiple views bound to potentially different buffers
- one pane tree referencing those views

## Target Runtime Model

To support combine properly, the model needs to move toward:

- `WorkspaceTab` owns layout and views
- each `EditorViewState` points to a buffer identity
- buffers live in a workspace-level or app-level registry

Conceptually:

```rust
pub struct WorkspaceTab {
    pub root_pane: PaneNode,
    pub views: Vec<EditorViewState>,
    pub active_view_id: ViewId,
}

pub struct EditorViewState {
    pub id: ViewId,
    pub buffer_id: BufferId,
    // view-local fields...
}
```

The exact final shape can vary, but the key rule should hold:

- views point at buffers
- tabs own layouts, not document storage directly

Without that change, combine would require awkward special cases or buffer copying that fights the architecture.

## Command Model

Drag and combine should be expressed through explicit commands, not hidden deep inside egui closures.

Recommended commands:

```rust
pub enum AppCommand {
    ReorderTab {
        from_index: usize,
        to_index: usize,
    },
    CombineTabIntoTab {
        source_index: usize,
        target_index: usize,
    },
}
```

Possible internal helpers later:

- `StartTabDrag`
- `UpdateTabDragHover`
- `CancelTabDrag`
- `CommitTabDrag`

The public app command layer should stay small. The UI can keep temporary drag state locally or in ephemeral app UI state.

## Drag State

Drag state should be transient and never persisted.

Recommended fields:

- dragged tab index
- drag start position
- current pointer position
- current drop intent:
  - none
  - reorder before index
  - reorder after index
  - combine into target index
- whether auto-scroll is currently active

This can live in temporary egui memory or a dedicated non-persisted UI state field on the app.

## Visual Design Rules

## Reorder Feedback

Use a strong, narrow insertion marker between tabs.

Recommended visuals:

- a vertical accent line between tab buttons
- slight spacing expansion where the tab will land
- dragged tab rendered with reduced opacity or as a lifted ghost

## Combine Feedback

Use a different visual from reorder so the user can tell the action changed.

Recommended visuals:

- target tab body highlights
- target tab border or fill changes
- optional overlay text such as `Combine into workspace`

Do not reuse the reorder insertion marker for combine hover. They need to read as different outcomes.

## Accessibility and Usability Rules

- tab drag threshold should prevent accidental reorder on normal clicks
- close buttons should continue to win pointer input inside their hit area
- the active tab should remain obvious during drag
- if a dragged tab cannot be combined because the model is not yet capable, the UI should not suggest that it can
- keyboard-driven tab movement can come later, but drag should not preclude it

## Edge Cases

## Dirty Tabs

Combining or reordering dirty tabs must preserve dirty state exactly as-is.

No save prompt should appear during reorder or combine. These are layout operations, not close operations.

## Duplicate Names

If multiple tabs share the same file name, drag previews and overflow surfaces should use the existing duplication context labels where needed.

## Combining Into Self

Dropping a tab onto itself must do nothing.

## Combining Active and Inactive Tabs

After combine:

- the source content should become active inside the target workspace
- the target workspace tab should become the active top-level tab

This gives the user a predictable “the thing I just dragged is what I now see” outcome.

## Combining a Workspace That Already Has Splits

The first implementation should use a deterministic insertion rule, not a context-sensitive heuristic.

Examples of acceptable first rules:

- always split the active leaf
- always split the root and insert second

Pick one rule and keep it stable.

## Session Persistence

Reorder and combine change persistent workspace structure, so their results must survive restart.

Persist after reorder:

- top-level tab order
- active tab index

Persist after combine:

- new workspace tab count
- target workspace pane tree
- target workspace views
- buffer bindings for those views
- active view and active tab

Do not persist:

- drag hover state
- insertion marker state
- pointer position
- transient auto-scroll state

## Implementation Phases

## Phase 1: Reorder Only

- add transient tab drag state
- support dragging within the visible strip
- commit reorder with a clear insertion marker
- keep overflow behavior unchanged
- add tests for index movement and active-tab preservation

This phase should land first because it is useful immediately and does not require the multi-buffer workspace model.

## Phase 2: Overflow-Aware Dragging

- auto-scroll the strip while dragging near edges
- keep dragged tabs reorderable even when many tabs are hidden
- optionally allow hover activation of overflowed tabs during drag

## Phase 3: Multi-Buffer Workspace Foundation

- move buffer ownership out of `WorkspaceTab`
- bind each view to a buffer identity
- update session persistence for multi-buffer workspaces
- add migration and restore tests

This is the architectural gate for real tab combining.

## Phase 4: Basic Combine

- allow dropping one tab onto another tab
- merge source workspace into target workspace
- remove source tab from the strip
- create a deterministic split in the target workspace
- focus the dragged content after the merge

## Phase 5: Richer Drop Targets

- optional left/right combine placement
- optional drop-on-editor-surface targeting
- optional drag from overflow list
- optional context menu alternatives to combine

## Testing Plan

Add tests for:

- moving a tab left and right in the strip
- preserving active tab identity across reorder
- preserving dirty state across reorder
- combining one workspace into another
- combining when the target already has a split tree
- restoring combined workspaces from session state
- large-tab-count drag behavior near overflow edges

Manual checks should cover:

- no accidental reorder on click
- close button still works during crowded layouts
- insertion marker is unambiguous
- combine hover looks visually different from reorder
- active content after combine matches user expectation

## Non-Goals for the First Pass

- multi-window tab detaching
- browser-style tab grouping UI
- arbitrary freeform docking
- persisting raw drag positions
- drag support inside every overflow presentation from day one

## Success Criteria

The feature is successful when:

- tabs can be reordered reliably by drag
- overflow does not break drag usability
- tab combining produces a real merged workspace, not a fake grouping shell
- the data model stays coherent and persistable
- the implementation reduces friction instead of creating ambiguous drag modes

## Recommended Immediate Next Steps

1. Implement tab-strip reorder drag without changing the workspace data model.
2. Refactor `WorkspaceTab` toward view-to-buffer binding so combine has a sound foundation.
3. Add combine only after the multi-buffer workspace model exists.