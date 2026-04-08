# Scratchpad Plan

This document reflects the current state of the Scratchpad codebase.

## Project Snapshot

Scratchpad is a Windows-focused text editor built with `egui` / `eframe`.

The current application already includes:

- custom frameless window chrome
- tabbed editing with a visible strip plus overflow list
- drag-and-drop tab reordering across both views
- multi-pane, multi-buffer workspace tabs
- drag-to-combine across top-level tabs
- per-file tile promotion into new top-level tabs
- per-workspace promote-all for splitting one workspace into one tab per file
- Open Here for loading files into the current workspace tab with equal-share rebalancing
- encoding-aware file open and save flows
- formatting-artifact inspection for control-character-heavy content
- session persistence for tabs, pane layout, view settings, zoom, wrap, and logging preference
- runtime file logging for major user actions

## Implemented Architecture

### Application Layout

- `src/main.rs`: app startup, egui font setup, and logging initialization
- `src/app/app_state.rs`: top-level state container and app-facing helpers
- `src/app/commands.rs`: command handling for tab, view, and split operations
- `src/app/chrome/`: reusable chrome widgets and tab button rendering
- `src/app/logging.rs`: file logger plus panic hook

### Domain Layer

- `src/app/domain/buffer.rs`: buffer state, metadata, encoding information, artifact analysis
- `src/app/domain/tab.rs`: `WorkspaceTab`, view lifecycle, split/close logic
- `src/app/domain/panes.rs`: split tree structure and pane manipulation
- `src/app/domain/view.rs`: per-view state such as line numbers and control-char visibility
- `src/app/domain/tab_manager.rs`: shared tab-order source of truth and tab-level bookkeeping

### Services Layer

- `src/app/services/file_service.rs`: file IO, encoding detection, BOM handling
- `src/app/services/file_controller.rs`: open/save orchestration and status/log integration
- `src/app/services/session_manager.rs`: session save / restore orchestration
- `src/app/services/session_store/`: persisted session model and filesystem operations

### UI Layer

- `src/app/ui/tab_strip/`: visible tab strip, tab bar layout, and shared reorder integration
- `src/app/ui/tab_overflow.rs`: full tab list popup with shared drag/drop behavior
- `src/app/ui/tab_drag/`: drag state, drop resolution, marker painting, auto-scroll support
- `src/app/ui/editor_area/`: pane tree rendering and split/divider behavior
- `src/app/ui/editor_content/`: text editing, read-only artifact views, gutter rendering
- `src/app/ui/status_bar.rs`: status summary, encoding display, line counts, control-char toggle, runtime logging toggle
- `src/app/ui/dialogs.rs`: destructive-action confirmation flows

## Implemented Features

### Windowing and Chrome

- [x] Frameless custom window
- [x] Custom caption controls
- [x] Window drag and resize regions
- [x] Integrated dark theme and phosphor icons

### Tabs and Navigation

- [x] Multi-tab editing
- [x] Dirty markers and duplicate-name context labels
- [x] Horizontal tab strip with overflow popup
- [x] Full overflow list by default
- [x] Shared tab order across strip and overflow
- [x] Promote-all workspace action in both the visible strip and overflow list for tabs with 3 or more files
- [x] Drag-and-drop reorder:
  - [x] within the strip
  - [x] within the overflow list
  - [x] between strip and overflow
- [x] Drag-to-combine across top-level tabs

### Editing and Views

- [x] Multi-pane layout within a workspace tab
- [x] Multi-buffer workspace tabs
- [x] Split creation and split resizing
- [x] Close individual views
- [x] Promote one file's tile group into a new top-level tab
- [x] Workspace-tab-wide line-number visibility
- [x] Per-view control-character visibility
- [x] Zoom via keyboard shortcuts and Ctrl + mouse wheel
- [x] Word-wrap state stored in the app model

### File Handling

- [x] Open file via native dialogs
- [x] Open Here into the current workspace tab
- [x] Save and Save As via native dialogs
- [x] Duplicate-path detection when reopening files
- [x] Encoding detection and round-trip save support
- [x] BOM preservation
- [x] Large-file warning in the status bar

### Artifact Handling

- [x] Detect ANSI/control-character-heavy content
- [x] Read-only cleaned view for artifact-heavy files
- [x] Explicit view to reveal control characters
- [x] Status-bar surfacing of artifact state

### Persistence and Logging

- [x] Session persistence for open tabs
- [x] Session persistence for pane layouts and views
- [x] Session persistence for font size, wrap, and logging toggle
- [x] Runtime file logging for major commands and file operations
- [x] Panic hook integration

### Validation

- [x] Unit and integration tests for tabs, session storage, buffers, file IO, and drag helpers
- [x] Stress coverage for high tab counts

## Current Limitations

- Search UI is still a placeholder and not implemented.
- There is no context-menu or command-palette layer yet for tile/tab actions such as promote, combine, or workspace explode.
- README-level packaging, releases, and installer work are not set up.
- Logging is intentionally event-oriented; it does not capture every transient render-state change.

## Near-Term Roadmap

### Editing and Workspace Model

- [ ] Implement search and replace
- [ ] Add explicit wrap controls in the UI if wrap should become user-facing
- [ ] Add keyboard shortcuts or command-palette entries for tile promotion and workspace promote-all
- [ ] Extend Open Here and workspace rebalance rules so the initial split axis can respond to available viewport shape instead of always starting vertical

### UX and Discoverability

- [ ] Add clearer split commands and discoverable pane controls
- [ ] Improve overflow list configurability if hidden-only mode should become runtime-selectable
- [ ] Add a context menu or actions menu for tab and tile operations
- [ ] Document supported interactions directly in the app UI

### Persistence and Reliability

- [ ] Expand tests around session migration / incompatible manifests as the format evolves
- [ ] Add more targeted logging around drag state and other hard-to-debug interactions when needed

## Maintainability Plan

This pass is driven by the hotspot checker output from `scripts/hotspots.py` / `hotspots.html`.

Top checker hotspots at the start of the pass:

- `src/app/ui/tile_header/split.rs` with score `454.83`
- `src/app/app_state.rs` with score `385.87`
- `src/app/ui/tab_strip/mod.rs` with score `382.99`
- `src/app/chrome/tabs.rs` with score `380.25`
- `src/app/ui/tab_drag/state.rs` with score `373.51`
- `src/app/services/file_controller.rs` with score `324.11`
- `src/app/ui/tab_overflow.rs` with score `308.28`

Refactor priorities:

- [x] Reduce UI orchestration complexity in the tab strip and overflow popup by extracting row collection, popup lifecycle, and drag/drop helper paths.
- [x] Remove repeated editor layouter plumbing and repeated status-setting logic so common behavior has a single implementation point.
- [x] Simplify file open/save orchestration by introducing a batch-open summary and small save/open helpers instead of mixing aggregation, status, and logging inline.
- [x] Reduce tile-header control branching by separating visibility, split-preview, and close-button helpers.
- [ ] Break up `src/app/ui/tile_header/split.rs` into smaller geometry, preview-paint, and drag-state modules.
- [ ] Split `src/app/app_state.rs` into narrower state, status, and command-forwarding surfaces.
- [ ] Revisit `src/app/chrome/tabs.rs` and `src/app/ui/tab_drag/state.rs` with the same extraction strategy used here.

Validation during this pass:

- [x] `cargo check` before changes
- [x] `cargo check` after tab strip / overflow refactor
- [x] `cargo check` after file-controller / status / text-edit refactor
- [x] `cargo check` after tile-header refactor
- [x] `cargo test`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## Working Definition of Done

Scratchpad should remain a responsive, encoding-aware, session-persistent editor with a single shared tab-order model and predictable pane behavior. New work should preserve that structure instead of reintroducing duplicated tab state in the strip and overflow UI.
