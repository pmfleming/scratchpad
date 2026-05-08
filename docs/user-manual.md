# Scratchpad User Manual

Scratchpad is a Windows text editor built as a safe-by-design Notepad replacement for everyday text work.

It is designed to stay responsive, restore work safely, handle awkward encodings and control-character artifacts visibly, and support multi-file workspaces without becoming a coding-first editor.

## Getting Started

The fastest way to get moving is:

1. Press `Ctrl + O` to open one or more files as tabs.
2. Press `Ctrl + Shift + O` to open files into the current workspace as tiles.
3. Press `Ctrl + Shift + Arrow` to split the active tile.
4. Press `Ctrl + T` to promote the active tile into its own tab.
5. Press `Ctrl + F` to search.
6. Press `F1` to open this manual.

The top toolbar also provides Open File, Save As, and Search buttons. When the tab list is placed on the left or right, those actions move into the vertical toolbar with Open File, Save, Search, and the window controls.

## Core Concepts

### Workspace Tabs

Each top-level tab is a workspace. A workspace can hold one file or several tiled file views.

If several files belong to the same task, keep them together in one workspace. If a workspace grows too large, promote one tile or promote all files back into separate tabs.

### Tiles, Views, and Buffers

A tile is one editor view inside a workspace.

A buffer is the underlying document. Several tiles can show the same buffer, which lets you keep multiple views of one file open in the same workspace.

Tiles can be activated, split, resized, closed, or promoted into their own tab.

### Active Focus

Most commands apply to the active editor tile. Click a tile, select a search result, use `F6`, or use the tab strip to change the active target.

### Open Here

Open Here adds incoming files to the current workspace rather than opening each file as a separate top-level tab. It is available from `Ctrl + Shift + O`, the tab context menu, and startup switches such as `/here` and `/addto`.

### Settings Surface

Settings is a normal app surface that can appear as a tab slot. Press `Ctrl + ,`, click the status-bar gear, or click its tab slot if it is already open.

The settings file itself is TOML and can also be opened as a normal text file from the Settings page.

## Opening Files and Starting Scratchpad

### In the App

- `Ctrl + O`: open one or more files according to the file-open setting.
- `Ctrl + Shift + O`: open one or more files into the active workspace.
- `Ctrl + N`: create a new untitled tab.
- `F1`: open this manual.
- Open File button: open files from the toolbar.
- New Tab `+` button: create a new untitled tab from the horizontal or vertical tab list.
- Tab context menu > New Tab: create a new tab.
- Tab context menu > Open File Here: add files to that workspace.

If a file is already open, Scratchpad activates the existing view instead of opening a duplicate. If Open Here targets a file that is open in another tab, Scratchpad moves or combines that tab into the active workspace.

### Command Line

Scratchpad supports:

```text
scratchpad.exe [switches] [files...]
```

Available switches:

- `/clean`: start with one fresh untitled tab and skip session restore.
- `/here`: add incoming files into the active workspace tab.
- `/addto`: alias for `/addto:active`.
- `/addto:active`: add incoming files into the active workspace tab.
- `/addto:index:N`: add incoming files into the Nth tab, using a 1-based index.
- `/files:"a","b"`: pass a comma-delimited quoted file list in one argument.
- `/help` or `/?`: show usage text.
- `/version`: print the app version and exit.

`/clean` cannot be combined with `/addto:index:N`, because there is no restored tab index to target.

## Saving and File Safety

### Save Commands

- `Ctrl + S`: save the active file.
- Toolbar Save As button: choose a path and save the active file.
- Tab context menu > Save: save the active file for that tab.
- Tab context menu > Save All: save all open files.
- Encoding dialog > Save: save the active file using the selected encoding.

Untitled files open a Save As dialog when saved. If the file name has no extension, Scratchpad suggests `.txt`.

### Save Conflicts

Before saving, Scratchpad refreshes the active file's disk state.

If the file changed on disk, the save conflict dialog offers:

- Overwrite: write the current buffer back to disk.
- Reload: discard local buffer state and reload from disk.
- Save As Copy: save the current buffer to a new file.
- Cancel: dismiss the prompt.

If the file is missing on disk, the dialog offers:

- Recreate the file at its original path.
- Discard this missing file tab.

### Unsaved Changes

Closing a dirty tab or closing the last tile that owns a dirty buffer prompts:

- Save changes
- Discard changes
- Cancel

Bulk close actions such as Close Others, Close Right/Down, and Close All skip tabs with unsaved changes and report how many were skipped. Close Saved closes only clean tabs.

### Encoding and Line Endings on Save

Scratchpad preserves the active file's detected encoding, supported BOM state, and line-ending metadata where possible. If the current text cannot be represented in the chosen encoding, Scratchpad warns before writing.

Inserted and pasted line breaks are normalized to the document's preferred line ending. Mixed line endings are tracked and preserved unless an explicit future normalization command changes them.

## Editing Text

### Typing and Insertion

- Type normally to insert text.
- IME preedit and commit input are supported.
- `Enter`: insert the document's preferred line ending.
- `Tab`: insert a tab character.
- `Shift + Tab`: outdent the current line by removing leading spaces or one leading tab when possible.
- Paste text with standard paste input, the context-menu Paste button, or `Shift + Insert`.

### Selection and Mouse

- Click: place the caret.
- `Shift + Click`: extend the existing selection.
- Drag: select text.
- Double-click: select a word.
- Triple-click: select a visual line.
- Right-click: open the editor context menu. If the click is outside the current selection, the caret moves to the clicked position first. If it is inside the selection, the selection is preserved.
- Scroll wheel and scrollbars: scroll the active editor.

### Caret Movement

- Arrow keys: move by character or line.
- `Ctrl + Left` / `Ctrl + Right`: move by word.
- `Alt + Left` / `Alt + Right`: move by word.
- `Home`: move to the current line start.
- `End`: move to the current line content end.
- `Ctrl + Home`: move to the start of the document.
- `Ctrl + End`: move to the end of the document.
- `Page Up` / `Page Down`: move by a page of document lines.
- Add `Shift` to movement keys to extend the selection.
- `Ctrl + A`: select all text in the active editor.

### Delete, Cut, Copy, and Paste

- `Backspace`: delete backward or delete the current selection.
- `Delete`: delete forward or delete the current selection.
- `Ctrl + Backspace`: delete backward by word.
- `Alt + Backspace`: classic undo shortcut.
- `Ctrl + Delete` / `Alt + Delete`: delete forward by word.
- Standard Copy/Cut/Paste events are supported.
- `Ctrl + Insert`: copy the current selection.
- `Shift + Insert`: request paste.
- Editor context menu rail: Cut, Copy, Paste, Select All.
- Editor context menu > Delete: delete the current selection.

### Undo and Redo

- `Ctrl + Z`: undo the last text operation in the focused document.
- `Ctrl + Y`: redo the last text operation in the focused document.
- `Alt + Backspace`: classic undo shortcut.
- Editor context menu > Undo / Redo.
- Search dialog undo/redo buttons.
- History dialog rows can move the document to a selected undo or redo point.

Undo history is per file and can be tuned in Settings > Advanced > Memory Assigned to Undo Operations.

## Search and Replace

Open search with:

- `Ctrl + F`
- Toolbar Search button
- Editor context menu > Find

Open search with replace focused using:

- `Ctrl + H`
- Editor context menu > Replace

The search window is movable. Close it with `Esc` while focused, the close button in the search header, or the toolbar Search button when search is already open.

### Search Fields

- Find field `Enter`: next match.
- Find field `Shift + Enter`: previous match.
- Replace field `Enter`: replace current match.
- `Ctrl + Enter`: replace current match.
- `Alt + Enter`: replace all matches in the current scope.
- `Esc`: close search.

### Search Options

Search supports:

- Selection-only scope
- Active file scope
- Current workspace-tab scope
- All-open-tabs scope
- Plain text mode
- Regex mode
- Case-sensitive matching
- Whole-word matching

If text is selected when search opens, Scratchpad defaults to selection-only scope. Otherwise it defaults to the active file.

### Result List

- Click a file result pill to focus that file at its first match.
- Click a caret beside a file result to expand or collapse the file's match rows.
- Click a match row to activate that exact match.
- The active match is highlighted in both the editor and the result list.

Search only covers text already open in Scratchpad. It does not search unopened files or folders.

### Replace Behavior

Replace is enabled only when search results are current and valid.

- Replace Current replaces the active match and then advances within the active buffer.
- Replace All replaces every match in the selected scope.
- Replace All across multiple buffers requires running Replace All twice. The first run reports the number of matches and buffers; the second confirms.
- Regex replacement uses the active regex program's replacement expansion.

If results become stale because the buffer changed, replacement is blocked until search refreshes.

## Tabs and Workspace Management

### Activate and Select Tabs

- Click a tab to activate it.
- Click the Settings tab to return to Settings.
- `Shift + Click` a tab: select a range of tab slots.
- `Ctrl + Click` a tab: toggle that tab slot in the selection.
- Click a tab in the overflow menu to activate it.
- Right-click a tab or empty tab-list space for the tab context menu.

### Rename Tabs and Files

- Double-click a workspace tab to rename it.
- Tab context menu > Rename also starts rename.
- `Enter`: commit the rename.
- `Esc`: cancel the rename.
- Losing focus commits the rename if the name is valid.

For saved files, rename also renames the file on disk. Names are normalized as file names, not paths; if no extension is supplied, `.txt` is added. The settings file cannot be renamed from the tab strip.

### Reorder, Group, and Combine Tabs

- Drag a tab to reorder it.
- Select multiple tabs with `Shift + Click` or `Ctrl + Click`, then drag one selected tab to move the group.
- Drag a tab onto the center of another workspace tab to combine them into one tiled workspace.
- Drag a selected group onto a workspace tab to combine the group into that workspace.
- When tabs overflow, use the overflow button to select, close, promote, or drag tabs. While dragging, hovering over the overflow button opens the overflow list after a short delay.

The Settings tab can be selected and moved visually, but workspace combining applies only to workspace tabs.

### Promote Tabs and Tiles

- `Ctrl + T`: promote the active tile into its own tab.
- `Ctrl + Shift + T`: promote all files in the active workspace into separate tabs.
- Tab promote button: promote all files in a multi-file workspace.
- Tile header Promote Tile button: promote that tile.
- Editor context menu > Promote Tile.

### Close Tabs and Tiles

- `Ctrl + W`: close the active tab, or close Settings when Settings is open.
- `Ctrl + Shift + W`: close the active tile when the workspace has more than one tile.
- Tab close button: request close for that tab.
- Tile close button: close that tile.
- Editor context menu > Close Tile.
- Window close button: persist settings and session, then close Scratchpad.

When only one tile remains in a workspace, closing the tab and closing the tile are no longer the same action. `Ctrl + Shift + W` only closes a tile when there is more than one tile.

### Tab Context Menu

Right-click a tab, the overflow button, or empty tab-list space to open tab commands.

File actions:

- New Tab
- Open File Here
- Rename
- Save
- Save All

Tab-list actions:

- Hide Tab List or Pin Tab List
- Place tab list at Top, Bottom, Left, or Right
- Order Tabs
- Choose tab ordering: Custom Order, File Name, File Size, File Age, Recent Edit

Location actions:

- Encoding
- Copy Path
- Reveal In Explorer

Close actions:

- Close
- Close Others
- Close Right for horizontal tab lists
- Close Down for vertical tab lists
- Close Saved
- Close All

## Tiles, Splits, and Layout

### Activate Tiles

- Click inside a tile to activate it.
- Right-click an inactive tile to activate it and open the context menu.
- `F6`: move to the next tile in layout order.
- `Shift + F6`: move to the previous tile.

### Split Tiles

Keyboard:

- `Ctrl + Shift + Left`: split the active tile and place the new view to the left.
- `Ctrl + Shift + Right`: split and place the new view to the right.
- `Ctrl + Shift + Up`: split and place the new view above.
- `Ctrl + Shift + Down`: split and place the new view below.

Mouse:

- Hover a tile to show tile controls.
- Click the tile split control for the default split action.
- Drag the tile split control left, right, up, or down to preview and create a split in that direction.
- Editor context menu > Split creates the default split.
- Editor context menu > Split submenu offers Split Left, Split Right, Split Up, and Split Down.

### Resize Splits

Drag the divider between tiles to resize the split. Vertical dividers resize left/right space; horizontal dividers resize top/bottom space.

## Editor Context Menu

Right-click the editor to open:

- Undo
- Redo
- Delete
- History
- Unicode
- Find
- Replace
- Split
- Promote Tile
- Close Tile
- Cut
- Copy
- Paste
- Select All

### Unicode Menu

The Unicode submenu includes:

- Left to Right / Right to Left reading order toggle.
- Control Chars toggle, when control characters are available or already shown.
- Insert Control submenu.
- Reconversion, currently unavailable.

Insert Control includes:

- LRM: left-to-right mark
- RLM: right-to-left mark
- ZWJ: zero-width joiner
- ZWNJ: zero-width non-joiner
- LRE, RLE, LRO, RLO, PDF: bidirectional formatting controls
- NADS, NODS, ASS, ISS, AAFS, IAFS: legacy shaping and digit controls
- RS: record separator
- US: unit separator

## Status Bar

The status bar shows:

- file path or `Untitled`
- line count
- cursor line and column
- selection character count
- encoding
- line-ending style
- disk freshness warnings
- non-compliant character warnings
- status and error messages

Interactions:

- Double-click the path to copy it.
- Double-click the line count to toggle line numbers.
- Click the encoding label to open encoding actions.
- Click the control-character icon to show or hide control characters when available.
- Click the history icon to open text history.
- Click the gear icon to open Settings.

The status bar can be hidden or shown from Settings > Tab Position > Status bar.

## Encoding and Artifact-Heavy Files

Scratchpad is designed to cope with files that contain:

- mixed or unusual encodings
- byte order marks
- ANSI escape sequences
- control characters
- carriage-return output artifacts
- backspace-driven overprint text
- characters that may not be representable in the chosen save encoding

### Detection

When opening a file, Scratchpad checks for a BOM first. If there is no BOM, it uses heuristic detection and stores the resolved encoding as document metadata.

The status bar shows the active file's encoding. Non-default encodings and BOM state are highlighted.

### Encoding Dialog

Open encoding actions from:

- status bar encoding label
- tab context menu > Encoding

The dialog shows the active file, a common-encoding selector, and two actions:

- Reopen: reload the file from disk using the selected encoding. This requires a saved path and no unsaved edits.
- Save: write the active file using the selected encoding. Untitled files open Save As.

The compatibility warning is intentional: choosing the wrong encoding can permanently corrupt characters or lose character mapping data.

## Text History

Open History from:

- editor context menu > History
- status bar history icon

History shows operation-based text edits. It includes normal typing, deletes, cuts, pastes, and search replacements.

Controls:

- Timeline: show all text changes in recent order.
- By file: group text changes by file.
- Follow undo toggle: when enabled, applying a history row also follows focus to the affected file.
- Clear all text history.
- Close button.

Interactions:

- Click a history row that has been applied to undo back to that change.
- Click a dimmed undone row to redo that change.
- In By file view, click a file header or caret to expand or collapse that file's history.
- The `Now` line marks the current undo position.

## Settings

Open Settings with:

- `Ctrl + ,`
- status-bar gear
- clicking the Settings tab when it is already open

Close Settings with:

- `Esc`
- `Ctrl + W`
- Settings tab close button

Settings cards can be expanded or collapsed by clicking their headers.

### Text Formatting

Settings > Text Formatting includes:

- Font family
- Font size
- Editor gutter width
- Preview panel
- Word wrap

### Appearance

Settings > Appearance includes:

- Theme mode: system, light, dark, or custom when colors are overridden
- Text color
- Background color
- Search highlight color
- Preview panel

### Opening

Settings > Opening includes:

- Opening files: open in a new tab or in the current tab
- Startup behavior: continue previous session, or start fresh and discard unsaved session changes
- Recent files toggle

### Tab Position

Settings > Tab Position includes:

- Tab list position: Top, Bottom, Left, Right
- New tab placement: Start, End, Before selection, After selection
- Auto-hide tab list
- Auto-hide delay
- Status bar visibility

### Advanced

Settings > Advanced includes:

- Settings file card: open the TOML settings file.
- Memory Assigned to Undo Operations: per-file, total, and persisted-session undo budgets.
- Reset to defaults.

## Session Restore and Startup Conflicts

Scratchpad can restore the previous session, including open tabs, workspace layout, font size, word wrap, and safely persisted session state.

If a restored session conflicts with an updated disk version, Scratchpad shows a restore conflict dialog:

- Keep Session Version
- Use Disk Version
- Dismiss

Session restore behavior is controlled from Settings > Opening > When Scratchpad starts.

## Window and Chrome

Window controls:

- Minimize button: minimize Scratchpad.
- Maximize/Restore button: toggle maximized state.
- Close button: persist settings and session, then close.
- Drag empty title/tab-strip space to move the window.
- Double-click empty title/tab-strip space to maximize or restore.
- When tabs are left or right, hover near the top edge to reveal the top drag button; drag it to move the window or double-click it to maximize or restore.
- Drag the window edges or corners to resize when not maximized.

View controls:

- `Ctrl + +` or `Ctrl + =`: increase editor font size.
- `Ctrl + -`: decrease editor font size.
- `Ctrl + Mouse Wheel` or the platform zoom gesture over the editor: adjust editor font size.
- `Shift + Mouse Wheel` over the editor: adjust editor font size.
- `Ctrl + 0`: toggle line numbers for the current workspace tab.

## Keyboard Shortcut Reference

Global and app-level shortcuts:

- `F1`: open the user manual.
- `F6`: move to the next tile.
- `Shift + F6`: move to the previous tile.
- `Ctrl + N`: new tab.
- `Ctrl + O`: open file.
- `Ctrl + Shift + O`: open file here as tile(s).
- `Ctrl + ,`: open Settings.
- `Esc`: close Settings or search when focused/open.
- `Ctrl + W`: close active tab or close Settings.
- `Ctrl + S`: save active file.
- `Ctrl + F`: open search.
- `Ctrl + H`: open search and focus replace.
- `Ctrl + T`: promote active tile to its own tab.
- `Ctrl + Shift + T`: promote all files in the active workspace to separate tabs.
- `Ctrl + Shift + W`: close active tile.
- `Ctrl + Shift + Arrow`: split active tile.
- `Ctrl + +` / `Ctrl + =`: increase font size.
- `Ctrl + -`: decrease font size.
- `Ctrl + 0`: toggle line numbers.

Editor shortcuts:

- `Ctrl + A`: select all.
- `Ctrl + Z`: undo.
- `Alt + Backspace`: classic undo.
- `Ctrl + Y`: redo.
- `Enter`: insert line ending.
- `Tab`: insert tab.
- `Shift + Tab`: outdent current line.
- `Backspace`: delete backward or delete selection.
- `Delete`: delete forward or delete selection.
- `Ctrl + Backspace`: delete backward by word.
- `Ctrl + Delete` / `Alt + Delete`: delete forward by word.
- `Ctrl + Insert`: copy selection.
- `Shift + Insert`: paste.
- Arrow keys, `Home`, `End`, `Page Up`, `Page Down`: move the caret.
- Add `Shift` to movement keys to extend selection.
- Add `Ctrl` or `Alt` to horizontal arrows for word movement.

Search shortcuts:

- Find field `Enter`: next match.
- Find field `Shift + Enter`: previous match.
- Replace field `Enter`: replace current match.
- `Ctrl + Enter`: replace current match.
- `Alt + Enter`: replace all matches.
- `Esc`: close search.

Rename shortcuts:

- Double-click tab: begin rename.
- Rename field `Enter`: commit.
- Rename field `Esc`: cancel.

## The User Manual File

Press `F1` to open this manual.

The manual is a normal Markdown file named `user-manual.md`. Scratchpad does not open it in a protected read-only mode, so you can edit it, save it, keep it in a tab, move it into a tiled workspace, copy its path, or copy it elsewhere.

If the shipped file is updated on disk, Scratchpad opens the updated version the next time you use the manual shortcut.

## Current Limits

- Search only covers files already open in Scratchpad.
- Context menus cover the main command surface, but a full command palette is still planned.
- Recent files can be enabled in Settings, but the broader recent-file UI may not be available in all builds.
- Windows packaging is currently zip-based.
- Scratchpad intentionally focuses on plain text editing rather than language-aware coding workflows.
