# Multi-Pane Architecture Plan

This document describes how to restructure Scratchpad so it can support many tabs, multiple editor windows inside each tab, and later multiple files inside one tab for comparison editing.

## Core Design Goals

1. Keep domain state separate from egui rendering.
2. Keep each Rust file focused on one concern.
3. Make layout changes possible without rewriting editor state management.
4. Make persistence independent from the UI tree.
5. Support many tabs and many panes without turning `src/app/mod.rs` into a new monolith.

## Target Mental Model

The app should treat these as separate concepts:

1. `Buffer`
   - the text content for one logical document
   - owns path, dirty state, and persistence identity
2. `EditorView`
   - one visual/editor instance pointing at a buffer
   - owns cursor, selection, scroll, search state, and other view-local state
3. `WorkspaceTab`
   - one tab in the tab strip
   - owns the pane layout for that workspace surface
4. `PaneLayout`
   - a split tree for horizontal and vertical pane divisions
5. `Workspace`
   - the collection of tabs, active tab, settings, and session metadata

The key shift is that a tab should stop being "the document" and become "the workspace surface".

## Recommended Module Structure

```text
src/
  app/
    mod.rs
    app_state.rs
    commands.rs
    domain/
      mod.rs
      buffer.rs
      tab.rs
    services/
      mod.rs
      session_store.rs
    ui/
      mod.rs
      tab_strip.rs
      editor_area.rs
      dialogs.rs
    chrome.rs
    theme.rs
```

This structure is intentionally simple. The goal is easy navigation and easy change.

## File Encapsulation Rules

1. Each file should have one primary reason to change.
2. UI files should not perform file I/O directly.
3. Domain files should not import egui types.
4. Persistence DTOs should stay separate from runtime structs.
5. A file longer than roughly 250 to 350 lines should be reviewed for extraction.
6. Avoid catch-all helper files.

## State Mutation Strategy

UI should emit commands and the app shell should handle them.

```rust
pub enum AppCommand {
    NewTab,
    ActivateTab { index: usize },
    RequestCloseTab { index: usize },
    CloseTab { index: usize },
    OpenFile,
    SaveFile,
    SaveFileAs,
}
```

This keeps egui closures smaller and lets buttons and keyboard shortcuts share the same behavior path.

## Persistence Plan

The current session store is a good base, but it should evolve from "persist tabs" to "persist workspace".

Persist separately:

1. app settings
2. open buffers
3. workspace tabs
4. pane layout per tab
5. editor view state that is worth restoring

Recommended first restore set:

- open tabs
- split tree per tab
- buffer to file-path association
- active tab
- active view in each tab
- font size and wrap settings

Defer if needed:

- exact scroll positions
- selections
- transient search state
- undo history

## Migration Plan

### Phase 1: Rename and Isolate Core Concepts

- rename `TabState` to `BufferState`
- introduce `WorkspaceTab` while keeping one-buffer-per-tab behavior
- move session storage into `services/session_store.rs`

### Phase 2: Extract Domain and UI Modules

- move header rendering into dedicated UI files
- move tab strip rendering into `ui/tab_strip.rs`
- move editor rendering into `ui/editor_area.rs`
- move modal handling into `ui/dialogs.rs`
- keep `mod.rs` as a composition root only

### Phase 3: Introduce Editor Views

- add `EditorViewState`
- make each tab own one initial view pointing to one buffer
- keep one visible pane initially

### Phase 4: Introduce Split Tree Layout

- add a binary split tree
- add split commands for horizontal and vertical split
- render the tree recursively

### Phase 5: Add Multi-Buffer Tabs

- allow different buffers in different leaves of a split tree
- define title derivation and close semantics

### Phase 6: Hardening

- add tests for command handling and layout mutations
- add session restore tests for split layouts
- add stress tests for many tabs and later many panes

## Suggested Immediate Next Steps

1. Add `EditorViewState` and keep it one-per-tab at first.
2. Add a `PaneNode` split tree with a single leaf as the initial layout.
3. Move file open/save logic into a dedicated file service once split panes start sharing buffers.