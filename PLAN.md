# Implementation Plan: Rust Simple Text Editor (Tabbed & Native)

This plan outlines the creation of a simple, isolated text editor in Rust that uses **native OS components** and supports a **tabbed interface**, similar to a modern take on the classic Windows Notepad.

## 1. Goals

### Primary Goals
- Build a desktop text editor using **native Windows (Win32) controls** for an authentic OS feel.
- Support a **tabbed interface** allowing multiple files to be open simultaneously.
- Match classic Notepad workflows: New, Open, Edit, Save, Save As, but extended for tabs.
- Maintain project isolation at the IDE and toolchain level.
- Produce a native Windows executable.

### Non-Goals for v1
- Rich text formatting or syntax highlighting.
- Split views or complex docking.
- Printing support.
- Multi-encoding support beyond UTF-8 (initially).

## 2. Environment & Isolation Strategy

### IDE Isolation (VS Code)
- **Extensions**: Use `.vscode/extensions.json` to recommend project-specific extensions (Rust-analyzer, etc.).
- **Settings**: Use `.vscode/settings.json` for project-specific editor behavior.
- **Isolation**: Extensions will be configured to load/activate specifically for this workspace.

### OS/Toolchain Isolation
- **Rust Toolchain**: Pin the version using `rust-toolchain.toml`.
- **Dependencies**: Managed via `Cargo.toml`, kept local to the project `target` folder.
- **Development**: Since we are using native Win32 components (`native-windows-gui`), development will happen directly on the Windows host to ensure full access to OS APIs and GUI debugging.

## 3. Tech Stack Selection

For a "simple text editor" that uses **OS components as much as possible**:
- **GUI Framework**: [**native-windows-gui** (NWG)](https://github.com/gabdube/native-windows-gui).
    - **Why**: Wraps the actual Windows Win32 API. It uses real OS controls (Tabs, Menus, RichEdit/Edit boxes) rather than rendering its own. This ensures a 100% native look, feel, and performance.
- **Tab Management**: NWG's `TabContainer` and `TabPage` controls.
- **File I/O**: Standard library `std::fs`.
- **Dialogs**: NWG's built-in `FileDialog` (standard Windows Open/Save dialogs).

## 4. Architecture

### Core Types
- **TabState**
  - Text buffer (associated with a native `RichTextBox` or `TextBox`).
  - File path (Optional).
  - Dirty flag (modified state).
- **EditorApp**
  - Collection of `TabState`.
  - Active tab index.
  - Native window and menu handles.

### State Rules
- Each tab maintains its own undo/redo history (handled by the native control).
- Closing a dirty tab prompts for confirmation.
- The window title reflects the active tab's filename.

## 5. Implementation Phases

### Phase 1: Native Window & Tabs
- Initialize Rust project: `cargo init`.
- Set up a basic NWG window with a `TabContainer` filling the client area.
- Implement "New Tab" logic: Programmatically add a `TabPage` with a native multiline text control.
- Verify basic input and tab switching.

### Phase 2: Native Menus & File Operations
- Add a top Menu Bar (Native Windows Menu):
    - **File**: New Tab, Open, Save, Save As, Close Tab, Exit.
- Implement File Open/Save using the native `FileDialog`.
- Map file contents to the active tab's text control.

### Phase 3: Tab Management UX
- Add "Close" buttons or a context menu for tabs.
- Handle "unsaved changes" warnings per tab.
- Update the application title bar when switching tabs or modifying text.
- Implement keyboard shortcuts: `Ctrl+T` (New Tab), `Ctrl+W` (Close Tab), `Ctrl+Tab` (Next Tab).

### Phase 4: Notepad-Style Features
- **Status Bar**: Native Windows status bar showing Line/Column and "UTF-8".
- **Word Wrap**: Toggle word wrap on the native text controls via the "Format" menu.
- **Search/Replace**: (Optional for v1) Use native find/replace dialogs.

## 6. Verification & Testing

- **Native Look**: Confirm all buttons, menus, and scrollbars match the Windows OS theme.
- **Tab Stress Test**: Open 10+ tabs and verify stability and performance.
- **File Integrity**: Verify UTF-8 files are read and written correctly without data loss.
- **Isolation**: Verify that opening a different VS Code window does not load this project's specific extensions/settings.

## 7. GitHub Integration
- Initialize Git: `git init`.
- Configure `.gitignore` for Rust and VS Code.
- Create GitHub repo and push the initial native scaffold.
