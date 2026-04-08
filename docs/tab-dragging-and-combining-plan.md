# Tab Dragging and Combining Plan

This document tracks the intended behavior and remaining work for direct tab manipulation in Scratchpad.

It originally described a future plan. The project has since implemented a substantial part of the tab-dragging work, so this document now serves two purposes:

- record what is already shipped
- define the remaining work for true tab combining

## Scope

This area covers:

- dragging tabs to reorder them in the visible strip
- dragging tabs across the overflow list surface
- auto-scrolling the tab strip while dragging
- eventual combining of one workspace tab into another workspace tab
- the model changes required to make combine real instead of cosmetic

It does not cover general docking, window detaching, or arbitrary freeform layout.

## Summary

### Implemented

- visible tab strip drag-reorder
- overflow-list drag-reorder participation
- shared drop-zone resolution across strip and overflow popup
- drag ghost rendering
- reorder marker rendering
- drag threshold and close-button protection
- tab-strip auto-scroll while dragging near the strip edges

### Not Implemented

- dropping one top-level tab onto another to combine workspaces
- any persistent multi-buffer workspace model
- combine-specific hover targets or combine preview visuals
- app command(s) for combine

## Current Product State

### Tab Dragging

Tab reordering is now implemented in the current `src/app/ui/tab_drag/` module and integrated into:

- `src/app/ui/tab_strip/mod.rs`
- `src/app/ui/tab_strip/tab_cell.rs`
- `src/app/ui/tab_overflow.rs`

The current behavior includes:

- pointer-down starts a pending drag state
- a movement threshold prevents accidental drag on click
- close-button hit areas still win input and suppress drag start
- dragging shows a ghosted tab under the pointer
- dragging shows a reorder marker in the active drop zone
- releasing commits reorder if the resolved target changes index
- dragging near the tab-strip edges auto-scrolls the strip
- overflow rows participate as vertical drop zones when the overflow popup is open

This is no longer a planned feature. It is shipped behavior.

### Overflow Integration

The earlier plan assumed overflow drag support would come later. That is no longer accurate.

The current implementation already supports:

- drag initiation from overflow rows
- drag placeholders for the active drag source inside the overflow popup
- vertical drop-zone resolution in the overflow popup
- reordering between the strip and the overflow surface using one shared drag model

What is still missing is not overflow-aware reorder. What is missing is combine-aware behavior.

### Workspace Model

The current workspace model is still the architectural blocker for combine.

Today `WorkspaceTab` still owns exactly one `BufferState`:

```rust
pub struct WorkspaceTab {
    pub buffer: BufferState,
    pub views: Vec<EditorViewState>,
    pub root_pane: PaneNode,
    pub active_view_id: ViewId,
}
```

`EditorViewState` already contains a `buffer_id`, which is useful groundwork, but in practice all views in a workspace tab still refer to the same `WorkspaceTab::buffer`.

That means the current multi-pane model is still:

- one buffer per workspace tab
- many views over that one buffer
- one pane tree over those views

This is enough for splits of the same file, but not enough for true tab combining.

## What Has Changed Since The Original Plan

The original plan is outdated in four important ways:

1. Reorder is no longer hypothetical.
2. Overflow drag is already partially implemented, not deferred.
3. The code is now modularized under `src/app/ui/tab_strip/` and `src/app/ui/tab_drag/`, not a single `src/app/ui/tab_strip.rs` file.
4. `EditorViewState` already has `buffer_id`, so the model foundation has moved partway toward multi-buffer support, but not far enough to support combine.

## Remaining Problem

The missing workflow is still real:

- users can reorder workspace tabs, but cannot merge one workspace tab into another by drag

That creates a mismatch in capability:

- intra-workspace layout is fairly capable
- inter-workspace manipulation stops at reorder

The remaining work should focus on combine, not on re-planning reorder.

## Design Goals

1. Preserve the current reorder behavior and keep it predictable.
2. Add combine only when the runtime model can represent it honestly.
3. Keep transient drag state out of session persistence.
4. Reuse the existing drag infrastructure where practical.
5. Make combine visually distinct from reorder.
6. Keep implementation sequencing realistic for the current codebase.

## Current Drag Architecture

The current drag stack is centered on transient state in `src/app/ui/tab_drag/state/`.

### Existing State Model

The drag state currently tracks:

- source tab index
- drag start pointer position
- current pointer position

Resolved drop behavior is currently reorder-only:

- shared `TabDropZone` values represent horizontal strip zones and vertical overflow zones
- drop resolution yields a zone index and drop slot
- release commits a reorder from source index to resolved destination index

### Existing Visual Feedback

Already implemented:

- dragged tab ghost
- reorder marker
- overflow drag placeholder

Not yet implemented:

- combine hover state
- combine target highlight
- combine-specific drop intent

## Combine Requirements

Combine must not be implemented as a fake grouping shell.

The intended result remains:

- source top-level tab is removed
- target top-level tab remains
- source content becomes part of the target workspace
- target workspace pane tree expands deterministically
- the newly inserted content becomes focused
- the target workspace tab becomes the active top-level tab

## Current Architectural Blocker

True combine requires one workspace tab to own multiple buffers.

The current model cannot represent that cleanly because:

- `WorkspaceTab` owns one concrete `BufferState`
- there is no workspace-level buffer registry
- view-to-buffer binding exists only as an identifier, not as a fully independent storage layer
- session persistence still serializes one buffer payload per workspace tab

So the blocker is no longer “views need a `buffer_id`."

The blocker is now:

- buffer storage still lives on `WorkspaceTab`
- session persistence assumes one buffer per workspace tab
- combine would require multiple buffers per workspace tab plus view bindings into them

## Recommended Target Runtime Model

The codebase should move toward this shape:

```rust
pub struct WorkspaceTab {
    pub buffers: Vec<WorkspaceBuffer>,
    pub views: Vec<EditorViewState>,
    pub root_pane: PaneNode,
    pub active_view_id: ViewId,
}

pub struct EditorViewState {
    pub id: ViewId,
    pub buffer_id: BufferId,
    // view-local UI state
}
```

The exact concrete types can vary, but these rules should hold:

- workspace tabs own layout plus a buffer collection
- views bind to buffers by identity
- closing a view is separate from deleting a buffer
- session restore can rebuild a workspace containing multiple buffers

Without that model shift, combine would either duplicate buffers awkwardly or violate the current architecture.

## Combine Interaction Model

The current reorder interaction should stay intact.

Recommended combine model:

- dragging between tabs means reorder intent
- dragging onto the interior body of a tab means combine intent
- the target tab visually highlights differently from reorder
- releasing over the tab body commits combine

That preserves a clear distinction:

- gap = reorder
- tab body = combine

## First Combine Layout Rule

The first combine implementation should be deterministic.

Recommended rule:

- combine into the target workspace by splitting the active leaf of the target workspace
- insert the dragged workspace content as the new second pane by default

This aligns with how the current split model already works and avoids arbitrary heuristics.

Future richer placement can come later.

## Commands

The public command layer should be expanded only when combine work actually starts.

The existing reorder path already routes through:

- `AppCommand::ReorderTab`

Recommended additional command when combine work begins:

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

Internal drag helpers can stay UI-local. The public command surface should remain small.

## Session Persistence Impact

Reorder is already compatible with current persistence.

Combine is not.

Reorder persistence requirements are already satisfied:

- top-level tab order
- active tab index

Combine will additionally require persisted support for:

- multiple buffers inside one workspace tab
- view-to-buffer bindings within one workspace tab
- pane tree references across those views
- active view within the merged workspace

Transient drag state should continue to remain non-persistent.

## Revised Implementation Phases

### Phase 1: Strip Reorder

Status: completed

Delivered:

- thresholded drag start
- reorder marker
- ghost tab
- release-to-commit reorder
- active-tab preservation through reorder

### Phase 2: Overflow-Aware Reorder

Status: substantially completed

Delivered:

- strip auto-scroll near edges
- overflow popup drag participation
- vertical overflow drop zones
- shared drop resolution across strip and overflow

Still optional here:

- hover-driven overflow activation behavior
- additional polish around popup behavior during extended drag scenarios

### Phase 3: Multi-Buffer Workspace Foundation

Status: not started in the required sense

Partial groundwork exists:

- `EditorViewState` already has `buffer_id`

But the real phase is still pending because:

- `WorkspaceTab` still owns a single buffer
- persistence still assumes a single buffer per workspace tab

This remains the actual gate for combine.

### Phase 4: Basic Combine

Status: not started

Target outcome:

- drag onto tab body commits merge
- source top-level tab disappears
- target workspace absorbs source buffer/view state
- target workspace grows by a deterministic split
- source content becomes focused in target workspace

### Phase 5: Richer Combine Targets

Status: not started

Possible follow-ups:

- left/right combine placement on tab body
- editor-surface drop targets
- overflow-origin combine polish
- optional non-drag entry points for combine

## Testing Status And Gaps

### Existing Coverage

The current drag state layer already includes unit coverage for:

- drop-slot resolution
- shared strip/overflow zone selection
- auto-scroll behavior near strip edges

### Remaining Needed Coverage

The next meaningful tests should focus on:

- reorder behavior at the app-command / tab-manager level if gaps remain
- persistence of reordered top-level tab order after restart
- future multi-buffer workspace restore behavior
- future combine command semantics
- future combine behavior when target already has splits

Manual checks should continue to cover:

- click does not accidentally reorder
- close buttons still win hit testing
- reorder markers remain unambiguous
- overflow popup behavior remains stable during drag

## Non-Goals

- multi-window tab detaching
- browser-style tab grouping UI
- arbitrary docking
- persisting drag hover state or pointer position
- implementing fake combine before the model can support it

## Success Criteria

This work is successful when:

- current reorder behavior remains reliable
- overflow does not break drag usability
- combine, when added, produces a real merged workspace
- the runtime model remains coherent and persistable
- the user can understand reorder vs combine from the visuals alone

## Recommended Next Steps

1. Treat reorder as shipped and stop planning it as a future feature.
2. Decide the target multi-buffer workspace shape before any combine UI work.
3. Refactor `WorkspaceTab` and session persistence to support multiple buffers per workspace tab.
4. Add `CombineTabIntoTab` only after the runtime model can represent merged workspaces honestly.
5. Reuse the existing tab-drag infrastructure for combine intent rather than introducing a second drag system.