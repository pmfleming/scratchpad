# Scratchpad Plan

This document summarizes the current state and next steps for Scratchpad, a Windows-focused text editor built with `egui` / `eframe`.

## Current State

Implemented:

- Custom frameless chrome with caption controls, window drag/resize regions, dark theme, and phosphor icons
- Shared tab order across the visible strip, overflow popup, and Settings tab
- Drag/drop tab reordering within and between the strip and overflow list
- Multi-pane, multi-buffer workspace tabs with split creation, split resizing, tile close, line-number toggles, and zoom
- Drag-to-combine across top-level tabs
- Open Here, per-file tile promotion, and workspace promote-all for tabs with 3 or more files
- Native open/save/save-as flows with dirty confirmation, duplicate-path checks, encoding detection, BOM preservation, and large-file status warnings
- Artifact-heavy content detection with read-only cleaned view and explicit control-character reveal
- TOML-backed settings for font, wrap, logging, and editor font
- Session persistence for tabs, pane layouts, views, and session metadata
- Runtime logging and panic hook integration
- Tests for tabs, session storage, buffers, file IO, drag helpers, and high tab counts

Current limitations:

- Search UI is still a placeholder.
- Context menus and command palette actions are not implemented.
- Installer packaging is not set up; release distribution is a Windows `.zip`.
- Logging is event-oriented and does not capture every transient render-state change.

## Architecture Map

- `src/main.rs`: startup, egui font setup, logging initialization
- `src/app/app_state.rs`: top-level state and app-facing helpers
- `src/app/commands.rs`: tab, view, and split command handling
- `src/app/chrome/`: window chrome and tab button rendering
- `src/app/logging.rs`: file logger and panic hook
- `src/app/domain/`: buffers, workspace tabs, pane trees, views, and shared tab manager
- `src/app/services/`: file IO/controller, session persistence, settings persistence
- `src/app/ui/`: tab strip, overflow popup, drag state, editor area/content, settings, dialogs, status bar

## Near-Term Roadmap

- Implement search and replace.
- Add user-facing wrap controls if wrapping should become configurable in the UI.
- Add shortcuts or command-palette entries for tile promotion and workspace promote-all.
- Let Open Here / workspace rebalancing choose the initial split axis from viewport shape.
- Add clearer split commands, pane controls, and tab/tile action menus.
- Make overflow list behavior configurable if hidden-only mode should become selectable.
- Expand session migration and incompatible-manifest tests as the format evolves.
- Add targeted drag-state logging when debugging needs it.

## Measurement

Standard tools:

- `scripts/hotspots.py`: complexity and maintainability JSON
- `scripts/slowspots.py`: benchmark-driven speed and degradation JSON
- `scripts/clone_alert.py`: token-based clone groups for duplication drift review
- `scripts/map.py`: dependency/interrelatedness JSON enriched with hotspot and slowspot data
- `scripts/ci.ps1`: standard local and CI entry point for formatting, linting, tests, hotspot review, slowspot review, and clone review

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

Remaining priorities:

- Split `src/app/ui/tile_header/split.rs` into geometry, preview-paint, and drag-state modules.
- Split `src/app/app_state.rs` into narrower state, status, and command-forwarding surfaces.
- Revisit `src/app/chrome/tabs.rs` and `src/app/ui/tab_drag/state.rs` with the same extraction strategy.

Validation already completed during this pass:

- `cargo check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `scripts/hotspots.py`
- `scripts/slowspots.py`

## Definition of Done

Scratchpad should remain a responsive, encoding-aware editor with TOML-backed settings, session persistence for workspace state, one shared tab-order model, and predictable pane behavior. New work should preserve that structure instead of reintroducing duplicated tab state in the strip and overflow UI.
