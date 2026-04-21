# Scratchpad Plan

This document summarizes the current state and next steps for Scratchpad, a safe-by-design, crash-resistant Notepad replacement for Windows built in Rust with `egui` / `eframe`.

## Current State

Implemented:

- Custom frameless chrome with caption controls, window drag/resize regions, dark theme, and phosphor icons
- Shared tab order across the visible strip, overflow popup, Settings surface, and drag/drop flows
- Drag/drop tab reordering within and between the strip and overflow list
- Multi-pane workspace tabs with split creation, split resizing, tile close, line-number toggles, and zoom
- Drag-to-combine across top-level tabs plus Open Here composition into the active workspace
- Per-file tile promotion and workspace promote-all for tabs with 3 or more files
- Search and search/replace across selection, active file, current workspace tab, and all open tabs, with plain-text and regex modes
- Native open/save/save-as flows with dirty confirmation, duplicate-path checks, encoding detection, BOM preservation, and large-file status warnings
- Artifact-heavy content detection with cleaned/raw inspection modes and explicit control-character reveal
- Document-level undo/redo per `TextDocument`
- A separate transaction log for text-edit history and workspace-level operations
- Tab and editor context menus for common workspace and tile actions
- TOML-backed settings for font, wrap, logging, editor font, startup/session behavior, file-open disposition, and tab-list preferences
- Session persistence for tabs, pane layouts, views, encodings, and session metadata
- Runtime logging and panic hook integration
- Measured performance and maintainability workflows covering hotspots, search speed, capacity limits, resource profiles, flamegraphs, and clone drift
- Tests for search, piece-tree behavior, native-editor behavior, tabs, session storage, buffers, file IO, drag helpers, transaction history, startup behavior, and high tab counts

Current limitations:

- Command palette actions are not implemented.
- Search only covers text already open in Scratchpad; there is no unopened-file or folder search.
- Some tab and tile actions still need clearer menu coverage and polish.
- Installer packaging is not set up; release distribution is a Windows `.zip`.
- Logging is event-oriented and does not capture every transient render-state change.

## Architecture Map

- `src/main.rs`: startup, egui font setup, logging initialization
- `src/app/app_state.rs`: top-level state and app-facing helpers
- `src/app/app_state/`: startup state, settings state, display-tab ordering, and settings refresh handling
- `src/app/commands.rs`: tab, view, and split command handling
- `src/app/commands/`: dispatch and tab/view transfer helpers
- `src/app/chrome/`: window chrome and tab button rendering
- `src/app/logging.rs`: file logger and panic hook
- `src/app/domain/`: buffers, workspace tabs, pane trees, views, shared tab manager, and layout/promotion helpers
- `src/app/services/`: file IO/controller, session persistence, settings persistence, and store helpers
- `src/app/startup/`: startup argument parsing
- `src/app/transactions.rs`: transaction log model and grouping logic for text-edit history
- `src/app/ui/`: tab strip, overflow popup, drag state, editor area/content, settings, dialogs, status bar, and tile header controls
- `viewer/`: static viewer for analysis artifacts
- `tests/`: integration coverage for app, files, session restore, startup, and tab behavior

## Near-Term Roadmap

- Refine search/replace UX, result presentation, and edge-case handling across split and duplicate-view workspaces.
- Detect and handle when an open/restored file is older than the latest on-disk version, especially after external edits while Scratchpad was closed.
- Add user-facing wrap controls if wrapping should become configurable beyond settings-driven defaults.
- Add shortcuts or command-palette entries for tile promotion and workspace promote-all.
- Let Open Here / workspace rebalancing choose the initial split axis from viewport shape.
- Add clearer split commands, pane controls, and broader tab/tile action menus.
- Make overflow list behavior configurable if hidden-only mode should become selectable.
- Expand session migration and incompatible-manifest tests as the format evolves.
- Refine transaction-log grouping and presentation for non-typing edits, replacements, and mixed insert/delete sequences.
- Improve Windows packaging beyond the current `.zip` release flow.
- Add targeted drag-state logging when debugging needs it.

## Measurement

Standard tools:

- `scripts/hotspots.py`: complexity and maintainability JSON
- `scripts/slowspots.py`: benchmark-driven speed and degradation JSON
- `scripts/search_speed.py`: search-scaling JSON for full-completion and first-response latency
- `scripts/capacity_report.py`: capacity-threshold JSON for file size, tabs, splits, and paste ceilings
- `scripts/resource_profiles.py`: allocation, working-set, page-fault, and session-cost JSON
- `scripts/generate_flamegraphs.py`: flamegraph index generation for dedicated profile binaries
- `scripts/speed_efficiency_report.py`: combined performance triage across latency, flamegraphs, and capacity signals
- `scripts/clone_alert.py`: token-based clone groups for duplication drift review
- `scripts/map.py`: dependency/interrelatedness JSON enriched with hotspot and slowspot data
- `scripts/ci.ps1`: standard local and CI entry point for formatting, linting, tests, hotspot review, slowspot review, and clone review
- `scripts/open-overview.ps1`: local launcher for the static viewer

The Python tools intentionally do not generate HTML. The static viewer in `viewer/` consumes their JSON contracts from `target/analysis/` and can later be replaced by a Java/React UI without changing the measurement layer.

## Maintainability Plan

This pass is driven by `scripts/hotspots.py`.

Top original hotspots:

- `src/app/ui/tile_header/split.rs`: `454.83`
- `src/app/app_state.rs`: `385.87`
- `src/app/ui/tab_strip/mod.rs`: `382.99`
- `src/app/chrome/tabs.rs`: `380.25`
- `src/app/ui/tab_drag/state.rs`: `373.51`
- `src/app/services/file_controller.rs`: `324.11`
- `src/app/ui/tab_overflow.rs`: `308.28`

Completed refactors:

- Extracted tab-strip and overflow popup row collection, popup lifecycle, and drag/drop helper paths
- Removed repeated editor layouter plumbing and status-setting logic
- Simplified file open/save orchestration with batch-open summary and small save/open helpers
- Reduced tile-header control branching by separating visibility, split-preview, and close-button helpers
- Split tile-header split behavior into geometry, preview, and drag modules
- Broke app/settings/session behavior into narrower helper modules under `app_state/` and `services/session_store/`
- Split piece-tree behavior into focused support, edit, and slice modules
- Reduced search-service branching and duplication while preserving plain-text and regex behavior
- Broke native-editor and legacy text-edit code into smaller interaction, highlighting, and windowing helpers
- Refined search runtime target collection and replacement planning to preserve current-tab behavior across duplicate buffers

Remaining priorities:

- Split `src/app/app_state.rs` into narrower state, status, and command-forwarding surfaces.
- Revisit `src/app/chrome/tabs.rs` and `src/app/ui/tab_drag/state.rs` with the same extraction strategy.
- Revisit `src/app/ui/tab_strip/context_menu.rs` to keep menu growth from recreating the same complexity pattern.
- Keep transaction-log logic tidy as history presentation evolves.

Validation already completed during this pass:

- `cargo check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `scripts/hotspots.py`
- `scripts/slowspots.py`

## Definition of Done

Scratchpad should remain a responsive, encoding-aware editor with TOML-backed settings, document-local undo/redo, transaction-history visibility, session persistence for workspace state, one shared tab-order model, and predictable pane behavior. New work should preserve that structure instead of reintroducing duplicated tab state in the strip and overflow UI.
