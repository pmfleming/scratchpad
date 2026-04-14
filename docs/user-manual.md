# Scratchpad User Manual

Scratchpad is a Windows-focused text editor for working with tabs, tiled editor views, multiple files in one workspace, and artifact-heavy text files.

## What It Does

- Open files into separate tabs or into the current workspace layout
- Split the current workspace into multiple editor tiles
- Promote one tile or one file group into its own tab
- Save with encoding and BOM preservation
- Restore the previous session layout
- Inspect text files that contain control characters or terminal artifacts
- Review document-local undo and a broader transaction history

## Core Concepts

### Tabs

Each top-level tab is a workspace. A workspace can contain one file or multiple files arranged as tiled views.

### Tiles

Each tile is an editor view. Tiles can be split, resized, closed, and promoted into their own tab.

### Open Here

`Open Here` adds one or more files into the current workspace instead of opening each file in a separate top-level tab.

### Transaction Log

Simple undo and redo operate on the focused document only.

The transaction log is broader. It tracks:

- text-edit transactions
- file-open operations
- tab and tile operations
- workspace restructuring operations

Undoing from the transaction log reverts the selected transaction and any newer transactions.

## Keyboard Shortcuts

- `Ctrl + N`: new tab
- `Ctrl + O`: open file
- `Ctrl + Shift + O`: open file here as tile(s)
- `Ctrl + ,`: open settings
- `Ctrl + S`: save active file
- `Ctrl + Z`: undo editor text changes for the focused document
- `Ctrl + Y`: redo editor text changes for the focused document
- `Ctrl + Shift + Z`: open the transaction log
- `Ctrl + W`: close active tab
- `Ctrl + Shift + W`: close active tile
- `Ctrl + Shift + Arrow`: split active tile
- `Ctrl + +` / `Ctrl + =`, `Ctrl + -`, `Ctrl + Mouse Wheel`: zoom editor font
- `Ctrl + 0`: toggle line numbers for the current workspace tab
- `Escape`: close Settings or the transaction log when focused/open

## Status Bar

The status bar shows:

- current file path
- line count
- encoding
- artifact warnings
- logging state
- transaction log access via `TXN`

## Settings

Settings are stored in TOML and currently include:

- font size
- word wrap
- runtime logging
- editor font
- startup/session behavior
- file-open disposition
- tab-list presentation options

## Working With Artifact-Heavy Files

Scratchpad can detect files containing ANSI sequences, carriage-return-only formatting, backspaces, and other control characters.

When artifacts are present, you can:

- inspect the cleaned reading view
- switch back to the raw control-character view
- keep editing the underlying document text

## Saving and Encodings

Scratchpad preserves:

- detected text encoding
- BOM presence
- file path identity for duplicate-open checks

## Known Gaps

- Search is not implemented yet.
- Replace is not implemented yet.
- Context menus and command palette actions are not implemented yet.
- Packaging is still zip-based rather than installer-based.
