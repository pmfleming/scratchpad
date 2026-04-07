# Tiled Editor Views Plan

## Problem

Scratchpad currently treats each tab as a single editor surface. That prevents:

- multiple views into the same file
- side-by-side comparison of different areas of one document
- split-driven workflows that are standard in tiling editors and window managers

The next step is to let one workspace tab contain a tiled layout of editor views.

## Goals

1. Follow a tiling window manager mental model.
2. Allow multiple views into the same buffer inside one tab.
3. Support both vertical and horizontal splits.
4. Make split creation feel direct, with one drag affordance used for both split directions.
5. Let each tile be closed independently with a close control that appears only on hover.
6. Keep buffer state separate from view state so multiple tiles can point at the same file safely.

## UX Model

### Workspace Tab

A tab becomes a workspace surface, not just a single buffer.

Each tab owns:

- one pane tree
- one or more editor views
- one active view id

### Tile

A tile is one visible editor view.

Each tile shows:

- the current buffer content
- local view state such as scroll position and selection
- a hover-only close button at the tile's top-right corner

The close button should:

- stay invisible by default
- fade in only when the pointer is over the tile header/hotspot area
- close only that tile, not the underlying buffer unless it is the last remaining view that owns the buffer in that workspace

### Split Creation

Split creation should use a single UI element, not separate horizontal and vertical buttons.

Recommended interaction:

- each tile gets one split handle in its top-right chrome area, near the close button
- dragging that handle left or right creates a vertical split
- dragging that handle up or down creates a horizontal split
- the dominant drag axis decides the split direction
- a drag threshold prevents accidental splits from clicks

Recommended visual feedback:

- while dragging, show a preview overlay inside the tile
- left/right preview highlights vertical split placement
- top/bottom preview highlights horizontal split placement
- releasing commits the split and duplicates the current view into the new tile

Initial behavior for a new split:

- both tiles point at the same buffer
- the new tile starts with cloned view settings where practical
- the new tile becomes active after the split

## Domain Model

The current `WorkspaceTab` still owns a single `BufferState`. That has to change.

Target runtime model:

```rust
pub struct WorkspaceTab {
    pub root_pane: PaneNode,
    pub active_view_id: ViewId,
}

pub enum PaneNode {
    Leaf { view_id: ViewId },
    Split {
        axis: SplitAxis,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

pub enum SplitAxis {
    Horizontal,
    Vertical,
}

pub struct EditorViewState {
    pub id: ViewId,
    pub buffer_id: BufferId,
    pub scroll_offset: Option<egui::Vec2>,
    pub show_line_numbers: bool,
    pub show_control_chars: bool,
}
```

Important distinction:

- `BufferState` remains the document
- `EditorViewState` becomes the per-tile view state
- `WorkspaceTab` owns layout plus view membership

This is the minimum structure needed to support multiple views into one file without duplicating buffers.

## Close Semantics

Closing a tile is different from closing a tab.

Rules:

1. If a workspace has more than one tile, closing a tile removes only that view from the pane tree.
2. If a workspace has exactly one tile, the tile close button should either:
   - be hidden, or
   - behave like normal tab close if you want aggressive symmetry

Recommended first implementation:

- hide the tile close button when there is only one tile in the workspace

When closing a tile:

- promote the sibling pane into the parent
- preserve the active view if it still exists
- if the closed tile was active, focus the nearest surviving sibling view

## Rendering Plan

### Current State

`editor_area.rs` renders one editor for `active_tab_index`.

### Target State

Replace the single-editor render path with recursive pane rendering:

1. render the active workspace tab
2. walk the pane tree recursively
3. if the node is a leaf, render one tile
4. if the node is a split, allocate the region by axis and ratio, then recurse into both children

Each tile should render:

- a small tile chrome/header region
- the hover-only tile controls
- the existing editor body
- the existing status-sensitive behaviors where relevant

Recommended extraction:

- `ui/editor_area.rs` becomes a composition layer
- add `ui/panes.rs` for recursive split rendering
- add `ui/tile.rs` for one tile's chrome and body
- add `domain/panes.rs` for `PaneNode`, `SplitAxis`, and tree mutations
- add `domain/view.rs` for `EditorViewState`

## Command Model

The pane system should be driven through commands instead of direct deep mutation from UI closures.

Recommended commands:

```rust
pub enum AppCommand {
    SplitActiveViewByDrag { direction: SplitDirection },
    CloseView { view_id: ViewId },
    ActivateView { view_id: ViewId },
    ResizeSplit { split_path: PanePath, ratio: f32 },
}
```

You may want a richer internal command layer later, but these are enough to start.

For drag-based splitting, the UI can resolve the dominant axis first and then emit one command.

## Split Interaction Details

Use one split affordance per tile.

Recommended behavior:

1. Mouse down on the split handle starts a pending split gesture.
2. While dragging, compute delta from the gesture origin.
3. If the drag does not exceed the threshold, treat it as cancelled.
4. If horizontal delta dominates, preview a vertical split.
5. If vertical delta dominates, preview a horizontal split.
6. On release, emit the split command using the resolved direction.

Recommended thresholds:

- minimum drag distance around 10 to 16 logical pixels
- dominant axis should exceed the secondary axis clearly enough to avoid jitter

This is simpler than gesture-free edge targeting and keeps the UI compact.

## Persistence Plan

Session persistence has to move from "one buffer per tab" to "workspace tab with pane tree and views".

Persist per workspace tab:

- pane tree structure
- active view id
- view-to-buffer mapping
- per-view toggles worth restoring

Persist separately:

- buffers
- app settings
- active workspace tab index

Do not persist initially:

- exact drag gesture state
- hover state
- transient split preview overlays

## Migration Phases

### Phase 1: Introduce View and Pane Types

- add `EditorViewState`
- add `PaneNode`
- make each `WorkspaceTab` contain one leaf view
- keep behavior visually unchanged

### Phase 2: Decouple Buffer State From View State

- move `show_line_numbers` and `show_control_chars` from `BufferState` to `EditorViewState`
- keep text, encoding, dirty state, and artifact summary in `BufferState`
- update session persistence accordingly

### Phase 3: Recursive Rendering

- render one tile from a leaf
- render splits recursively from `PaneNode`
- keep one active view highlight/focus path

### Phase 4: Tile Chrome

- add per-tile hover region
- add hover-only close button
- add one split drag handle
- add split preview overlay

### Phase 5: Commands and Tree Mutation

- implement split active view
- implement close view
- implement active-view focus changes
- implement sibling promotion on close

### Phase 6: Persistence and Hardening

- persist pane trees and views
- add tree mutation tests
- add session restore tests
- test repeated split/close cycles

## Testing Plan

Add tests for:

- splitting a single leaf into a two-child split
- horizontal and vertical split creation
- closing one leaf promotes the surviving sibling
- closing the active leaf reassigns focus correctly
- two views can point at the same buffer
- per-view toggles stay isolated between views
- session persistence restores pane tree shape and active view

Add manual UI checks for:

- split preview direction feels predictable
- close button appears only on tile hover
- drag threshold avoids accidental splits
- repeated nested splits remain usable

## Risks

1. If view state and buffer state are not separated early, split panes will duplicate document-level flags incorrectly.
2. Recursive UI rendering can become messy if pane mutation is done directly inside egui closures.
3. Persisting the tree too early without stable ids will create fragile session restore behavior.
4. Tile chrome can easily become visually noisy if hover affordances are always visible.

## Recommended Immediate Next Steps

1. Change `WorkspaceTab` from `buffer` ownership to `root_pane + views`.
2. Introduce `EditorViewState` and move view-local toggles out of `BufferState`.
3. Add a `PaneNode::Leaf` default path so one-tile tabs still work.
4. Build recursive pane rendering before implementing drag splitting.
5. Add the hover-only close button and single split handle after the tree render path is stable.
