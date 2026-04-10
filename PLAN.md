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
- a dedicated Settings surface that behaves like a real tab in the shared tab model
- encoding-aware file open and save flows
- formatting-artifact inspection for control-character-heavy content
- TOML-backed settings persistence for font, wrap, logging, and editor font selection
- session persistence for tabs, pane layout, and session/view metadata
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
- `src/app/services/settings_store.rs`: TOML-backed settings load/save, legacy YAML migration, and default handling

### UI Layer

- `src/app/ui/tab_strip/`: visible tab strip, tab bar layout, and shared reorder integration
- `src/app/ui/tab_overflow.rs`: full tab list popup with shared drag/drop behavior
- `src/app/ui/tab_drag/`: drag state, drop resolution, marker painting, auto-scroll support
- `src/app/ui/editor_area/`: pane tree rendering and split/divider behavior
- `src/app/ui/editor_content/`: text editing, read-only artifact views, gutter rendering
- `src/app/ui/settings.rs`: settings page with font, diagnostics, and settings sections
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
- [x] Settings page as a shared-order tab with strip + overflow presence
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
- [x] TOML-backed settings independent of session restore

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
- [x] TOML persistence for font size, wrap, logging, and editor font
- [x] Runtime file logging for major commands and file operations
- [x] Panic hook integration

### Validation

- [x] Unit and integration tests for tabs, session storage, buffers, file IO, and drag helpers
- [x] Stress coverage for high tab counts
- [x] Standardized complexity measurement via `scripts/hotspots.py`
- [x] Standardized performance measurement via `scripts/slowspots.py`
- [x] Standardized architecture/interrelatedness mapping via `scripts/map.py`

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

## Standard Measurement Methods

The project uses three standardized measurement tools:

- `scripts/hotspots.py`: complexity and maintainability data emitted as JSON
- `scripts/slowspots.py`: benchmark-driven speed and degradation data emitted as JSON
- `scripts/map.py`: dependency/interrelatedness data emitted as JSON, enriched with hotspot and slowspot data

These should be treated as the default ways to measure:

- complexity
- speed
- interrelatedness

`scripts/ci.ps1` is the standard local and CI entry point and runs the supported checks together.

The Python tools intentionally do not generate HTML. Viewer work should consume the three JSON files and render the presentation layer separately, for example with a Java/React tabbed interface for hotspots, slowspots, and the map.

The repo includes a lightweight static viewer in `viewer/` as the first presentation layer over those JSON contracts. It is intentionally separate from the Python scripts so it can later be replaced by, or migrated into, a Java/React UI without changing the measurement tools.

## Maintainability Plan

This pass is driven primarily by the hotspot checker output from `scripts/hotspots.py`.

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
- [x] `scripts/hotspots.py`
- [x] `scripts/slowspots.py`

## Working Definition of Done

Scratchpad should remain a responsive, encoding-aware editor with TOML-backed settings, session persistence for workspace state, a single shared tab-order model, and predictable pane behavior. New work should preserve that structure instead of reintroducing duplicated tab state in the strip and overflow UI.
