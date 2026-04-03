# Implementation Plan: Scratchpad (egui)

This plan outlines the architecture and progress of our custom, tabbed text editor written in Rust using the **egui** framework.

## 1. Project Goals

### Primary Goals
- **Modern Tabbed Interface**: Support multiple open files with an intuitive tab strip.
- **Custom Aesthetic**: A polished dark theme with custom-drawn caption buttons and unique UI elements.
- **Functional Parity**: Match standard Notepad features (New, Open, Save, Save As) with extended tab management.
- **High Performance**: Leverage immediate-mode GUI for a lag-free typing experience.
- **Cross-Platform Potential**: While developed on Windows, the use of `egui` and `rfd` allows for easy porting to other OSs.

## 2. Tech Stack

- **GUI Framework**: [**egui** / **eframe**](https://github.com/emilk/egui).
- **File Dialogs**: [**rfd**](https://github.com/PolyMeilex/rfd) for native system dialogs.
- **Assets**: Custom PNG assets for buttons and menus, loaded via the `image` crate.
- **Architecture**: Modular Rust structure for maintainability.

## 3. Modular Architecture (`src/app/`)

The application is split into specialized modules:
- **`mod.rs`**: Core application state (`ScratchpadApp`) and the main `eframe::App` implementation.
- **`tabs.rs`**: `TabState` struct and logic for individual buffers.
- **`chrome.rs`**: Reusable UI components, icon loading, and custom-drawn buttons.
- **`theme.rs`**: Centralized color palette and layout constants.

## 4. Completed Features

### Phase 1: Core UI & Layout
- [x] Frameless window with custom title bar.
- [x] Custom caption buttons (Minimize, Maximize/Restore, Close).
- [x] Integrated File Menu with popup.
- [x] High-contrast dark theme consistent across all components.

### Phase 2: Tab Management
- [x] Dynamic tab strip with horizontal scrolling.
- [x] **Integrated Close Buttons**: Using custom assets inside each tab.
- [x] **ID Safety**: Using `ui.push_id` to prevent clashes between multiple "Untitled" tabs.
- [x] Tab switching and "New Tab" functionality.

### Phase 3: File Operations & Logic
- [x] **Native Dialogs**: Open, Save, and Save As using `rfd`.
- [x] **Dirty State Tracking**: Visual `*` indicator and unsaved changes confirmation modal.
- [x] **Safe Exit Flow**: Unsaved changes confirmation now also guards app exit and OS-level close requests.
- [x] Status bar showing current file path and line count.

### Phase 4: UX & Polish
- [x] **Keyboard Shortcuts**: Ctrl+N (New), Ctrl+O (Open), Ctrl+S (Save), Ctrl+W (Close).
- [x] **Dynamic Font Sizing**: Ctrl + Scroll wheel to resize text in the editor.
- [x] Refactored codebase into a clean, modular structure.

## 5. Future Roadmap

### Phase 5: Advanced Editing (Planned)
- [ ] **Search & Replace**: A custom modal for finding and replacing text across the active tab.
- [x] **Word Wrap Toggle**: Option to toggle text wrapping in the editor.
- [ ] **Line Numbers**: Adding a gutter with line numbers to the left of the text area.

### Phase 6: Persistence & Settings
- [ ] **Session Persistence**: Save and restore open tabs on restart.
- [ ] **Configuration**: Allow users to customize default font size and theme colors via a config file.

## 6. Verification & Standards
- **Performance**: Ensure the UI remains responsive even with large files (>1MB).
- **Safety**: Robust error handling for file I/O operations.
- **Consistency**: Maintain the established architectural pattern of separating UI logic from state management.
