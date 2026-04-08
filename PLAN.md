# Scratchpad Plan

This document reflects the current state of the Scratchpad codebase.

## Project Snapshot

Scratchpad is a Windows-focused text editor built with `egui` / `eframe`.

The current application already includes:

- custom frameless window chrome
- tabbed editing with a visible strip plus overflow list
- drag-and-drop tab reordering across both views
- multi-pane editing inside a workspace tab
- encoding-aware file open and save flows
- formatting-artifact inspection for control-character-heavy content
- session persistence for tabs, pane layout, view settings, zoom, wrap, and logging preference
- runtime file logging for major user actions

## Implemented Architecture

### Application Layout

- `src/main.rs`: app startup, egui font setup, and logging initialization
- `src/app/app_state.rs`: top-level state container and app-facing helpers
- `src/app/commands.rs`: command handling for tab, view, and split operations
- `src/app/chrome.rs`: reusable chrome widgets and tab button rendering
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
- [x] Drag-and-drop reorder:
  - [x] within the strip
  - [x] within the overflow list
  - [x] between strip and overflow

### Editing and Views

- [x] Multi-pane layout within a workspace tab
- [x] Split creation and split resizing
- [x] Close individual views
- [x] Per-view line-number visibility
- [x] Per-view control-character visibility
- [x] Zoom via keyboard shortcuts and Ctrl + mouse wheel
- [x] Word-wrap state stored in the app model

### File Handling

- [x] Open file via native dialogs
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
- Multi-pane layout currently gives multiple views into the same buffer within a workspace tab; true multi-buffer workspaces are not finished.
- README-level packaging, releases, and installer work are not set up.
- Logging is intentionally event-oriented; it does not capture every transient render-state change.

## Near-Term Roadmap

### Editing and Workspace Model

- [ ] Implement search and replace
- [ ] Add explicit wrap controls in the UI if wrap should become user-facing
- [ ] Support true multi-buffer workspaces, not just multi-view same-buffer panes

### UX and Discoverability

- [ ] Add clearer split commands and discoverable pane controls
- [ ] Improve overflow list configurability if hidden-only mode should become runtime-selectable
- [ ] Document supported interactions directly in the app UI

### Persistence and Reliability

- [ ] Expand tests around session migration / incompatible manifests as the format evolves
- [ ] Add more targeted logging around drag state and other hard-to-debug interactions when needed

## Working Definition of Done

Scratchpad should remain a responsive, encoding-aware, session-persistent editor with a single shared tab-order model and predictable pane behavior. New work should preserve that structure instead of reintroducing duplicated tab state in the strip and overflow UI.