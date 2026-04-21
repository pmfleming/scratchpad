# Scratchpad User Manual

Scratchpad is a Windows text editor built as a safe-by-design Notepad replacement for general text work.

It is designed to stay responsive, handle awkward encodings and control-character artifacts safely, and avoid drifting into a coding-first editor.

## Getting Started

Scratchpad works like a normal text editor when you only need one file, but it becomes more useful when you need to compare files, keep several views open in one workspace, or inspect files with unusual encodings or control-character artifacts.

The fastest way to get moving is:

1. Press `Ctrl + O` to open one or more files into tabs.
2. Press `Ctrl + Shift + O` to add files into the current workspace instead.
3. Press `Ctrl + Shift + Arrow` to split the active tile.
4. Press `Ctrl + T` to move the active tile into its own tab.
5. Press `F1` to open this manual.

## Core Concepts

### Workspace Tabs

Each top-level tab is a workspace. A workspace can hold one file or several files.

If a workspace contains several files, Scratchpad can keep them together inside one tab as multiple tiles instead of forcing every file into its own top-level tab.

### Tiles

A tile is one editor view inside a workspace.

Tiles can be:

- split
- resized
- closed
- promoted into their own tab

Several tiles can point at the same underlying file buffer. That lets you keep more than one view of the same file open inside one workspace.

### Open Here

`Open Here` adds files into the active workspace instead of opening each file in a separate tab.

Use this when one task needs one combined workspace.

### Settings Surface

Scratchpad includes a Settings surface for editor and workspace behavior.

The settings file itself is still a normal text file. You can open it, edit it, and save it like any other document.

### Transaction Log

Normal undo and redo affect the focused document.

The transaction log tracks broader app actions such as opening files, splitting views, promoting tiles, and other workspace-level changes. Use it when the thing you want to undo is larger than a simple text edit.

## Everyday Tasks

### Open Files

- `Ctrl + O`: open file into tabs
- `Ctrl + Shift + O`: open file into the current workspace
- `Ctrl + N`: create a new untitled tab

If the same file is already open, Scratchpad activates the existing tab instead of opening a duplicate.

### Split a Workspace

Press `Ctrl + Shift + Arrow` with the active tile focused.

- `Up` and `Down` create horizontal splits
- `Left` and `Right` create vertical splits

The new tile starts as another view in the same workspace.

### Promote a Tile or a Whole Workspace

- `Ctrl + T`: promote the active tile to its own tab
- `Ctrl + Shift + T`: promote all files in the active workspace into separate tabs

This is useful when a combined workspace starts to grow and you want to break it back into top-level tabs.

### Close Things

- `Ctrl + Shift + W`: close the active tile
- `Ctrl + W`: close the active tab

When only one tile remains in a workspace, closing the tab and closing the tile are no longer the same action.

### Save Changes

- `Ctrl + S`: save the active file

Scratchpad preserves the file's detected encoding and BOM state when saving.

### Search and Replace

- `Ctrl + F`: open the search strip with find and replace fields visible
- `Ctrl + H`: move focus directly to the replace field
- `Enter` in the find field: next match
- `Shift + Enter` in the find field: previous match
- `Esc`: close search and return focus to the active editor

The search strip supports:

- selection-only scope when text is selected
- active file scope
- current workspace-tab scope
- all-open-tabs scope
- plain-text and regex modes
- case-sensitive matching
- whole-word matching
- replace current match
- replace all matches in the current scope when replacement is allowed

Search operates on the decoded text already loaded into each open buffer. That means the same search query can match across open files even when those files were loaded from different encodings.

## Editing Behavior

### Undo and Redo

- `Ctrl + Z`: undo text edits in the focused document
- `Ctrl + Y`: redo text edits in the focused document
- `Ctrl + Shift + Z`: open the transaction log

The text editor undo stack and the transaction log are related, but they are not the same thing.

### Zoom and Layout Aids

- `Ctrl + +` or `Ctrl + =`: increase editor font size
- `Ctrl + -`: decrease editor font size
- `Ctrl + Mouse Wheel`: zoom editor font
- `Ctrl + 0`: toggle line numbers for the current workspace

These controls change presentation without changing file contents.

## Encodings and Artifact-Heavy Files

Scratchpad is designed to cope with text files that contain:

- mixed or unusual encodings
- byte order marks
- ANSI escape sequences
- control characters
- carriage-return style output artifacts
- backspace-driven overprint text

When Scratchpad detects these conditions, it keeps the underlying file editable while surfacing warnings and cleaned views where appropriate.

## Status Bar

The status bar reports the current document state, including:

- file path
- line count
- encoding
- artifact warnings
- logging state
- transaction log access via `TXN`

If Scratchpad notices an on-disk conflict, stale file, or decoding issue, the status area is one of the first places to check.

## Settings

Press `Ctrl + ,` to open Settings.

Current settings include:

- font size
- word wrap
- runtime logging
- editor font preset
- theme mode
- editor colors
- startup and session behavior
- file-open disposition
- tab list position, width, and auto-hide behavior
- recent file behavior

Scratchpad stores settings in TOML.

## Session Restore

By default, Scratchpad restores the previous session so you can continue where you left off.

That includes open tabs, workspace structure, and other session state that can be persisted safely.

## The User Manual File

Press `F1` to open this manual.

The manual is just a normal Markdown file named `user-manual.md`.

Scratchpad does not treat it as a protected or special read-only document. You can:

- edit it
- save it
- keep it open in a tab
- move it into a tiled workspace
- copy it elsewhere

If you update the shipped file on disk, Scratchpad will open your updated version the next time you use the manual shortcut.

## Keyboard Shortcuts

- `F1`: open the user manual
- `Ctrl + N`: new tab
- `Ctrl + O`: open file
- `Ctrl + Shift + O`: open file here as tile(s)
- `Ctrl + ,`: open settings
- `Ctrl + S`: save active file
- `Ctrl + F`: open search with replace visible
- `Ctrl + H`: focus the replace field in the search strip
- `Ctrl + Z`: undo editor text changes for the focused document
- `Ctrl + Y`: redo editor text changes for the focused document
- `Ctrl + Shift + Z`: open the transaction log
- `Ctrl + T`: promote active tile to its own tab
- `Ctrl + Shift + T`: promote all files in the active workspace to tabs
- `Ctrl + W`: close active tab
- `Ctrl + Shift + W`: close active tile
- `Ctrl + Shift + Arrow`: split active tile
- `Ctrl + +` / `Ctrl + =`, `Ctrl + -`, `Ctrl + Mouse Wheel`: zoom editor font
- `Ctrl + 0`: toggle line numbers for the current workspace tab
- `Escape`: close Settings or the transaction log when focused or open

## Current Limits

- Search only covers text already open in Scratchpad; it does not search unopened files or folders on disk.
- Context menus exist, but command coverage is still narrower than a full command palette.
- A command palette is planned but not available yet.
- Windows packaging is currently zip-based.
- Scratchpad is intentionally focused on text editing rather than language-aware coding workflows.
