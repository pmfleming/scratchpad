# Implementation Plan: Scratchpad (egui)

This plan outlines the architecture and progress of our custom, tabbed text editor written in Rust using the **egui** framework.

## 1. Project Goals

### Primary Goals
- **Modern Tabbed Interface**: Support multiple open files with an intuitive tab strip.
- **Custom Aesthetic**: A polished dark theme with Phosphorus icons and perfectly aligned controls.
- **Functional Parity**: Match standard Notepad features (New, Open, Save, Save As) with extended tab management.
- **High Performance**: Leverage immediate-mode GUI for a lag-free typing experience.
- **Multi-Pane Editing**: Support splitting the workspace into multiple editor views.

## 2. Tech Stack

- **GUI Framework**: [**egui** / **eframe**](https://github.com/emilk/egui).
- **Icons**: [**egui-phosphor**](https://crates.io/crates/egui-phosphor) for modern, font-based iconography.
- **File Dialogs**: [**rfd**](https://github.com/PolyMeilex/rfd) for native system dialogs.
- **Architecture**: Domain-driven modular structure.

## 3. Modular Architecture (`src/app/`)

The application is split into specialized modules for better maintainability:
- **`domain/`**: Pure business logic (Buffers, WorkspaceTabs).
- **`services/`**: Infrastructure and persistence (SessionStore).
- **`ui/`**: Specialized rendering components (TabStrip, EditorArea, Dialogs).
- **`app_state.rs`**: Central state and command handling.
- **`chrome.rs`**: Reusable UI primitives and window decoration.

## 4. Completed Features

### Phase 1: Core UI & Layout
- [x] Frameless window with custom title bar.
- [x] Custom caption buttons (Minimize, Maximize/Restore, Close).
- [x] High-contrast dark theme consistent across all components.
- [x] **Phosphor Integration**: All icons replaced with scalable font-based icons.

### Phase 2: Tab Management
- [x] Dynamic tab strip with horizontal scrolling.
- [x] **Overflow Dropdown**: Consistent styling with the main tab strip.
- [x] **ID Safety**: Global ID management using tab indices to prevent clashes.
- [x] Tab switching and "New Tab" functionality.
- [x] **Consistent Naming**: Centralized display name logic with dirty markers and duplication context.

### Phase 3: File Operations & Logic
- [x] **Native Dialogs**: Open, Save, and Save As using `rfd`.
- [x] **Dirty State Tracking**: Visual `*` indicator and unsaved changes confirmation modal.
- [x] **Safe Exit Flow**: Unsaved changes confirmation now also guards app exit and OS-level close requests.
- [x] Status bar showing current file path and line count.

### Phase 4: UX & Polish
- [x] **Keyboard Shortcuts**: Ctrl+N (New), Ctrl+O (Open), Ctrl+S (Save), Ctrl+W (Close).
- [x] **Dynamic Font Sizing**: Ctrl + Scroll wheel to resize text in the editor.
- [x] **Stress Tested**: Verified performance with 1,000+ tabs and random closing orders.

## 5. Future Roadmap

### Phase 5: Multi-Pane Architecture (Current Focus)
- [ ] **EditorViewState**: Decouple view state (scroll, cursor) from buffer state.
- [ ] **Split Tree**: Implement a binary split tree for horizontal and vertical workspace divisions.
- [ ] **Pane Management**: Commands to split, join, and resize panes.
- [ ] **Multi-Buffer Views**: Allow different buffers to be visible simultaneously in one tab.

### Phase 6: Advanced Editing
- [ ] **Search & Replace**: Integrated find and replace overlay.
- [ ] **Line Numbers**: Adding a gutter with line numbers to the left of the text area.
- [ ] **Tab Drag and Drop**: Allow users to reorder tabs by dragging.

### Phase 7: Persistence & Settings
- [x] **Session Persistence**: Save and restore open tabs, font size, and wrap settings.
- [ ] **Layout Persistence**: Save and restore the multi-pane split configuration.
- [ ] **Configuration**: Persistent user settings for default themes and font choices.

## 6. Verification & Standards
- **Performance**: Ensure the UI remains responsive even with 1,000+ tabs.
- **Safety**: Robust error handling for file I/O operations.
- **Consistency**: Centralized UI tokens and domain-driven state separation.
