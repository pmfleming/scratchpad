# Scratchpad User Manual

Scratchpad is a Windows-first plain-text editor for everyday text work. It is
designed for notes, logs, reports, copied terminal output, encoded files, and
temporary scratch work rather than coding projects.

The app keeps work local, restores sessions, warns about risky file-format
choices, and lets one tab hold either a single document or a tiled workspace of
several file views.

## Start Here

Use these commands first:

- `Ctrl + N`: create a new untitled tab.
- `Ctrl + O`: open files using the current opening preference.
- `Ctrl + Shift + O`: open files into the active workspace.
- `Ctrl + S`: save the active file.
- `Ctrl + Shift + S`: save the active file as another path.
- `Ctrl + F`: search.
- `Ctrl + H`: search with replace focused.
- `Ctrl + ,`: open Settings.
- `F1`: open this manual.

The toolbar also exposes common actions such as new tab, open, save, and search.
When the tab list is on the left or right, the toolbar becomes vertical.

## How Scratchpad Is Organized

### Tabs

A top-level tab is a workspace. A workspace may contain one editor tile or many
tiles. Tabs can be reordered, selected as a group, renamed, closed, or combined
with other workspace tabs.

The Settings surface can also appear as a tab slot. It behaves like an app page,
not a normal text file, although the TOML settings file can be opened from
Settings when you want to edit it directly.

### Tiles, Views, and Buffers

A tile is one editor view inside a workspace. A buffer is the underlying
document. Several tiles can point at the same buffer, which is useful when you
want two views of one long file.

Most commands act on the active tile. Click inside a tile, choose a search
result, or press `F6` / `Shift + F6` to move focus between tiles.

### Open Here

Open Here adds files to the active workspace instead of opening each file as a
separate tab. Use it when several documents belong together.

Open Here is available from `Ctrl + Shift + O`, the tab context menu, recent
file actions when opening into the current tab, and startup switches such as
`/here` and `/addto`.

## Opening and Startup

Inside the app:

- `Ctrl + O`: open one or more files.
- `Ctrl + Shift + O`: open one or more files into the active workspace.
- Tab context menu > Open File: open through the tab menu.
- Tab context menu > Open File Here: add files to that workspace.
- Recent files submenu: reopen recently closed files when recent files are
  enabled.

If a file is already open, Scratchpad activates the existing document instead of
creating an accidental duplicate. When Open Here targets an already-open file,
Scratchpad moves or combines the existing tab into the target workspace.

Command-line form:

```text
scratchpad.exe [switches] [files...]
```

Supported switches:

- `/clean`: start with a fresh untitled tab and skip session restore.
- `/here`: add incoming files to the active workspace.
- `/addto`: alias for `/addto:active`.
- `/addto:active`: add incoming files to the active workspace.
- `/addto:index:N`: add incoming files to the Nth restored tab, using a 1-based
  index.
- `/files:"a","b"`: pass a comma-delimited quoted file list in one argument.
- `/help` or `/?`: print help text.
- `/version`: print the app version.

Examples:

```text
scratchpad.exe "C:\notes\a.txt" "C:\notes\b.txt"
scratchpad.exe /clean "C:\notes\a.txt"
scratchpad.exe /here "C:\notes\a.txt"
scratchpad.exe /addto:active /files:"C:\a.txt","C:\b.txt"
```

`/clean` cannot be combined with `/addto:index:N` because there is no restored
tab index to target.

## Editing Text

Normal typing, selection, clipboard, IME input, and mouse editing work as
expected.

Core editing commands:

- `Ctrl + A`: select all text in the active editor.
- `Ctrl + Z`: undo the last text operation.
- `Ctrl + Y`: redo the last undone text operation.
- `Alt + Backspace`: classic undo shortcut.
- `Backspace`: delete backward or delete the selection.
- `Delete`: delete forward or delete the selection.
- `Ctrl + Backspace`: delete the previous word.
- `Ctrl + Delete` / `Alt + Delete`: delete the next word.
- `Tab`: insert a tab.
- `Shift + Tab`: outdent the current line when possible.
- `Ctrl + Insert`: copy the current selection.
- `Shift + Insert`: paste.

Caret movement:

- Arrow keys: move by character or line.
- `Ctrl + Left` / `Ctrl + Right`: move by word.
- `Alt + Left` / `Alt + Right`: move by word.
- `Home`: move to the current line start.
- `End`: move to the current line content end.
- `Ctrl + Home`: move to the start of the document.
- `Ctrl + End`: move to the end of the document.
- `Page Up` / `Page Down`: move by a page.
- Add `Shift` to movement keys to extend the selection.

Mouse behavior:

- Click: place the caret.
- `Shift + Click`: extend the current selection.
- Drag: select text.
- Double-click: select a word.
- Triple-click: select a visual line.
- Right-click: open the editor context menu.
- Mouse wheel and scrollbars: scroll the active editor.

## Saving and File Safety

Save commands:

- `Ctrl + S`: save the active file.
- `Ctrl + Shift + S`: save the active file as another path.
- Tab context menu > Save: save that tab's active file.
- Tab context menu > Save All: save every open file.
- Encoding dialog > Save: save using the selected encoding.

Untitled files open Save As. If a new file name has no extension, Scratchpad
suggests `.txt`.

Before saving, Scratchpad refreshes the file's disk state. If the file changed
outside Scratchpad, the save-conflict dialog lets you overwrite, reload from
disk, save a copy, or cancel. If the file is missing, you can recreate it or
discard the missing-file tab.

On Linux, ordinary files are replaced atomically while retaining their Unix
permissions and extended attributes. Saving through a symbolic link updates its
target without replacing the link. A file with multiple hard links is updated
in place so every linked name keeps the same content and inode. Scratchpad makes
a private, hidden recovery copy beside the file before such an in-place save and
removes it after the new content is safely synchronized. If the save fails, the
recovery copy remains and its path is included in the error message.

When closing dirty work, Scratchpad asks whether to save, discard, or cancel.
Bulk close commands skip tabs with unsaved changes and report how many were
skipped. Close Saved closes only clean tabs.

## Encoding, Newlines, and Artifacts

Scratchpad is built for plain text that may not be clean UTF-8.

It detects byte order marks first, then falls back to heuristic encoding
detection. The active file's encoding, BOM state, and line-ending style are
shown in the status bar. Non-default or risky states are highlighted.

Scratchpad tracks:

- UTF-8 and common legacy encodings.
- UTF-16 little-endian and big-endian files.
- BOM state where supported.
- CRLF, LF, CR, and mixed line endings.
- Characters that cannot be represented by the selected save encoding.
- Control characters and terminal-style artifacts such as ANSI escapes,
  carriage-return output, and overprint patterns.

Open encoding actions from:

- `Ctrl + Shift + E`
- the status-bar encoding label
- tab context menu > Encoding

The Encoding dialog offers:

- Reopen: reload the file from disk using the selected encoding. This requires a
  saved path and no unsaved edits.
- Save: write the active file using the selected encoding. Untitled files open
  Save As.

Choosing the wrong encoding can permanently change characters on disk, so the
warning in that dialog is intentional.

Display controls:

- `Ctrl + Alt + C`: toggle control-character display for the active buffer.
- `Ctrl + Alt + R`: toggle left-to-right / right-to-left reading order.
- Editor context menu > Unicode: access reading order, control characters, and
  inserted control marks.

## Search and Replace

Open search with:

- `Ctrl + F`
- toolbar Search
- editor context menu > Find

Open search with replace focused using:

- `Ctrl + H`
- editor context menu > Replace

Search supports:

- selection-only scope
- active-file scope
- current-workspace scope
- all-open-tabs scope
- plain-text search
- regex search
- case-sensitive matching
- whole-word matching

When search opens with text selected, Scratchpad starts in selection scope.
Otherwise it starts in active-file scope.

Search field commands:

- Find field `Enter`: next match.
- Find field `Shift + Enter`: previous match.
- Replace field `Enter`: replace current match.
- `Ctrl + Enter`: replace current match.
- `Alt + Enter`: replace all matches in the current scope.
- `Esc`: close search when search is focused.

Search results are grouped by file. Click a file result to focus its first
match, expand a file to inspect individual rows, or click a row to jump to that
exact match.

Replace is enabled only while results are current. If a buffer changes and
results become stale, refresh search before replacing. Replace All across
multiple files uses a confirmation step so broad edits are harder to trigger by
accident.

Search covers files already open in Scratchpad. It does not search unopened
folders.

## Tabs and Workspaces

Activate and select tabs:

- Click a tab to activate it.
- `Shift + Click`: select a range of tab slots.
- `Ctrl + Click`: toggle a tab slot in the selection.
- Click a tab in overflow to activate it.
- Right-click a tab, the overflow button, or empty tab-list space for tab
  commands.

Rename:

- `F2`: rename the active workspace tab.
- Double-click a workspace tab to rename it.
- Tab context menu > Rename also starts rename.
- `Enter`: commit the new name.
- `Esc`: cancel the rename.

Renaming a saved single-file tab also renames the file on disk. Scratchpad
normalizes the entered name as a file name, not a path. If no extension is
supplied, `.txt` is added. The Settings tab cannot be renamed from the tab
strip.

Reorder and combine:

- Drag a tab to reorder it.
- Select multiple tabs and drag one selected tab to move the group.
- Drag a tab onto the center of another workspace tab to combine them.
- Drag a selected group onto a workspace tab to combine the group.
- Use the overflow list when there are more tabs than fit in the visible strip.

Promote:

- `Ctrl + T`: promote the active tile into its own tab.
- `Ctrl + Shift + T`: promote every file in the active workspace into separate
  tabs.
- Tile header > Promote Tile: promote that tile.
- Editor context menu > Promote Tile: promote that tile.
- Tab promote button: promote files from a multi-file workspace.

Close:

- `Ctrl + W`: close the active tab, or close Settings when Settings is open.
- `Ctrl + Shift + W`: close the active tile when the workspace has more than
  one tile.
- Tab close button: close that tab.
- Tile close button: close that tile.
- Editor context menu > Close Tile: close the active tile.
- Window close button: persist settings and session, then close Scratchpad.

Tab context menu groups:

- File actions: New Tab, Open File, Open File Here, recent files, Rename, Save,
  Save All.
- Tab list actions: hide or pin the tab list, place tabs Top/Bottom/Left/Right,
  and choose tab ordering.
- Location actions: Encoding, Copy Path, Reveal In Explorer on Windows or Open Containing Folder on Linux.
- Close actions: Close, Close Others, Close Right or Close Down, Close Saved,
  Close All.

Useful tab/list shortcuts:

- `Ctrl + Shift + C`: copy the active file path.
- `Ctrl + Shift + R`: reveal the active file in Explorer on Windows or open its containing folder on Linux.
- `Ctrl + Alt + B`: show or hide the tab list.
- `Ctrl + Shift + B`: toggle auto-hide for the tab list.

## Tiles and Splits

Tiles let one workspace show several views at once.

Navigate:

- Click inside a tile to activate it.
- Right-click an inactive tile to activate it and open its context menu.
- `F6`: move to the next tile.
- `Shift + F6`: move to the previous tile.

Split:

- `Ctrl + Shift + Left`: split the active tile and place the new view left.
- `Ctrl + Shift + Right`: split and place the new view right.
- `Ctrl + Shift + Up`: split and place the new view above.
- `Ctrl + Shift + Down`: split and place the new view below.
- Tile split control: click for the default split, or drag in a direction to
  preview and create that split.
- Editor context menu > Split: split right by default.
- Editor context menu > Split submenu: choose Left, Right, Up, or Down.

Resize:

- Drag the divider between tiles.
- Vertical dividers resize left/right space.
- Horizontal dividers resize top/bottom space.

## Editor Context Menu

Right-click the editor for:

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

The Unicode submenu includes reading-order controls, the control-character
toggle, and common invisible control marks such as LRM, RLM, ZWJ, ZWNJ,
directional formatting marks, record separator, and unit separator.

## Status Bar

The status bar summarizes the active file and current editor state:

- file path or `Untitled`
- line count
- cursor line and column
- selection character count
- encoding and BOM state
- line-ending style
- disk freshness warnings
- encoding compliance warnings
- status and error messages

Interactions:

- Double-click the path to copy it.
- Double-click the line count to toggle line numbers.
- Click the encoding label to open encoding actions.
- Click the control-character indicator to toggle control-character display.
- Click the history icon to open text history.
- Click the gear icon to open Settings.

Use `Ctrl + Shift + M` to open the status-message history dialog. Use Settings
to show or hide the status bar.

## Text History

Open text history from:

- `Ctrl + Shift + H`
- editor context menu > History
- status-bar history icon

History shows operation-based text edits such as typing, deletes, cuts, pastes,
and search replacements.

The dialog includes:

- Timeline: recent text changes in order.
- By file: grouped text changes.
- Follow undo: move focus to the file affected by the chosen history row.
- Clear all text history.

Click an applied history row to undo back to that point. Click a dimmed undone
row to redo forward. The `Now` line marks the current undo position.

Undo history is per document and can be tuned in Settings > Advanced > Memory
Assigned to Undo Operations.

## Settings

Open Settings with:

- `Ctrl + ,`
- the status-bar gear
- the Settings tab slot when it is visible

Close Settings with:

- `Esc`
- `Ctrl + W`
- the Settings tab close button

Settings are grouped into expandable cards:

- Text Formatting: font family, font size, gutter width, preview, and word wrap.
- Appearance: theme mode, text color, background color, search highlight color,
  and preview.
- Opening: file-open behavior, startup/session behavior, and recent files.
- Tab Position: tab-list placement, new-tab placement, auto-hide behavior, and
  status-bar visibility.
- Advanced: settings file, undo memory budgets, and reset to defaults.

The settings file is TOML. Opening it from Settings creates a normal editable
text tab.

## Session Restore

Scratchpad can restore your previous session, including open tabs, tiled
workspace layout, font size, word wrap, and persisted unsaved state.

Startup behavior is controlled in Settings > Opening. Choose whether Scratchpad
continues the previous session or starts fresh.

If a restored file no longer matches disk, Scratchpad shows a restore-conflict
dialog with options to keep the session version, use the disk version, or
dismiss the prompt.

## Window and View Controls

Window controls:

- Minimize: minimize Scratchpad.
- Maximize/Restore: toggle maximized state.
- Close: persist settings and session, then close.
- Drag empty title/tab-strip space to move the window.
- Double-click empty title/tab-strip space to maximize or restore.
- Drag window edges or corners to resize when not maximized.

When tabs are left or right, hover near the top edge to reveal the top drag
button. Drag it to move the window or double-click it to maximize or restore.

View controls:

- `Ctrl + +` / `Ctrl + =`: increase editor font size.
- `Ctrl + -`: decrease editor font size.
- `Ctrl + Mouse Wheel`: adjust editor font size.
- `Shift + Mouse Wheel`: adjust editor font size.
- `Ctrl + 0`: toggle line numbers for the active workspace tab.

## Shortcut Reference

App shortcuts:

- `F1`: open this manual.
- `F2`: rename the active workspace tab.
- `F6`: move to the next tile.
- `Shift + F6`: move to the previous tile.
- `Esc`: close Settings or search when focused/open.
- `Ctrl + N`: new tab.
- `Ctrl + O`: open file.
- `Ctrl + Shift + O`: open file here.
- `Ctrl + S`: save.
- `Ctrl + Shift + S`: save as.
- `Ctrl + W`: close active tab or Settings.
- `Ctrl + ,`: open Settings.

Search, history, and diagnostics:

- `Ctrl + F`: find.
- `Ctrl + H`: replace.
- `Ctrl + Shift + H`: text history.
- `Ctrl + Shift + M`: status-message history.

Tabs, files, and paths:

- `Ctrl + T`: promote active tile.
- `Ctrl + Shift + T`: promote all files in the workspace.
- `Ctrl + Shift + W`: close active tile.
- `Ctrl + Shift + E`: encoding dialog.
- `Ctrl + Shift + C`: copy active file path.
- `Ctrl + Shift + R`: reveal active file in Explorer on Windows or open its containing folder on Linux.
- `Ctrl + Alt + B`: show or hide the tab list.
- `Ctrl + Shift + B`: toggle tab-list auto-hide.

Display and layout:

- `Ctrl + Shift + Arrow`: split the active tile.
- `Ctrl + Alt + C`: toggle control-character display.
- `Ctrl + Alt + R`: toggle reading order.
- `Ctrl + +` / `Ctrl + =`: increase font size.
- `Ctrl + -`: decrease font size.
- `Ctrl + 0`: toggle line numbers.

Editor shortcuts:

- `Ctrl + A`: select all.
- `Ctrl + Z`: undo.
- `Ctrl + Y`: redo.
- `Alt + Backspace`: undo.
- `Ctrl + Backspace`: delete previous word.
- `Ctrl + Delete` / `Alt + Delete`: delete next word.
- `Ctrl + Insert`: copy.
- `Shift + Insert`: paste.
- `Shift + Tab`: outdent.

Search field shortcuts:

- `Enter`: next match or replace current, depending on focused field.
- `Shift + Enter`: previous match from the find field.
- `Ctrl + Enter`: replace current.
- `Alt + Enter`: replace all.

## Current Limits

- Search only covers open files.
- Folder-wide search for unopened files is not available.
- A full command palette is still planned.
- Some context-menu coverage is still growing.
- Recent files can be enabled in Settings, but the broader recent-file surface
  is still evolving.
- Scratchpad intentionally stays focused on plain text rather than
  language-aware coding workflows.

## About This Manual

This manual is a normal Markdown file named `user-manual.md`. Press `F1` to open
it in Scratchpad. It is not protected or read-only, so you can edit it, save it,
move it into a tiled workspace, or copy it elsewhere like any other text file.
