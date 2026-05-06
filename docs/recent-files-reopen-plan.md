# Recent Files Reopen Plan

## Goal

Add a lightweight way to reopen recently loaded files and recently closed file tabs without adding another always-visible control to the tab chrome. The current UI already keeps the primary action area very small: the top tab list shows Open File, Save As, and Search; the vertical tab list shows Open File, Save, and Search beside window controls; and Settings already has a Recent files toggle that currently reads like a placeholder. The plan should therefore extend the existing Open File affordance and tab context menu rather than creating a new toolbar button, side panel, or permanent recent-files strip.

## Current UI Constraints

- Top tab primary actions are fixed around three icon buttons in `src/app/ui/tab_strip/actions/primary.rs`.
- Vertical tab primary actions already wrap carefully in `src/app/ui/tab_strip/actions/vertical.rs`, so another permanent icon would make narrow side-tab layouts worse.
- The tab right-click menu already carries file lifecycle actions such as New Tab, Open File Here, Rename, Save, Encoding, Copy Path, Reveal In Explorer, tab-list controls, ordering, and close actions.
- Settings already includes `Recent files` under Opening in `src/app/ui/settings/opening.rs`; this should become the user-facing enable/disable switch for the feature.
- Existing context-menu guidance says right-click menus are secondary command surfaces and should stay small.

## Product Shape

Treat “recently loaded files” and “recently closed files” as related but separate histories. Recently loaded files are a persisted MRU list of file paths that successfully opened, regardless of whether the file is still open. Recently closed files are a shorter session-aware stack of file-backed tabs the user explicitly closed, optimized for “I closed that tab by accident.” The UI should call them `Open Recent` and `Reopen Closed File` so users do not have to understand the underlying distinction.

The primary entry point should be the existing Open File button, not a new button. A normal click keeps opening the file picker exactly as it does today. A secondary click, long press, or small hover caret on the same Open File control can open a compact recent-files popup. The same behavior should exist in both top and vertical tab-list layouts so users do not lose the feature when the tab list moves. The tooltip can mention recent files, but the default visible chrome should remain visually unchanged.

The tab right-click menu should get only one additional row in the file-actions group: `Open Recent >`. That submenu can contain the top few recently loaded files plus a final `More Recent Files...` action if the list is longer. `Reopen Closed File` should appear only when there is at least one recently closed file and should restore the most recent closed file immediately; a submenu is acceptable only after there are multiple closed candidates worth showing. This keeps the context menu from becoming a history browser while still making accidental close recovery discoverable where users already manage tabs.

## Data Model

Persist a bounded `recent_files` list alongside settings or a small dedicated recent-files store. Each entry should include canonical path, display name, last opened timestamp, last successful metadata snapshot if cheap, and optional last-open disposition. Deduplicate by normalized path, move reopened files to the front, cap the list at a modest number such as 25, and silently drop entries whose paths are no longer usable only after a failed reopen or explicit cleanup.

Track recently closed files separately as `recently_closed_tabs`. For the first implementation this can be a bounded in-memory plus persisted list of file paths and display names, capped around 10. If later the app wants full closed-tab restore, the entry can grow to include tab layout, active view, encoding, and dirty-session information, but the first pass should reopen clean file-backed buffers through the normal open path. Do not add unsaved untitled buffers to this list unless full closed-tab state restoration exists.

Use the existing `recent_files_enabled` setting as the feature gate. When disabled, the app should stop adding entries and hide recent-file UI affordances, but it does not need to delete existing history unless the user explicitly clears it. Settings can later add a `Clear Recent Files` action in the Opening section, but that should be a secondary settings action, not part of the main tab chrome.

## Reopen Behavior

Opening a recent file should use the same file loading pipeline as ordinary Open File so encoding detection, duplicate detection, background loading, settings-file handling, and open summaries remain consistent. If a recent file is already open, activate the existing tab rather than opening a duplicate. If the file is missing or fails to load, show a concise status message and either remove it immediately from the recent list or mark it stale until the next cleanup.

`Reopen Closed File` should prefer the most recently closed file-backed tab that is not already open. If it is already open, activate it and remove that closed entry. If the closed tab contained multiple files or split views, the first pass should either reopen the primary active file only or deliberately hide that entry until richer closed-tab restore exists; the plan should not pretend path-only MRU can faithfully restore a complex tab layout.

The feature should avoid surprising the user’s file placement preference. `Open Recent` should respect `FileOpenDisposition` for the default action, while `Open Recent Here` can be considered later for tile or tab context menus. From a tab context menu, opening a recent file should behave like `Open File`, and reopening a closed file should restore it as a normal tab rather than replacing the clicked tab.

## UI Integration

Phase 1 should add recent access to the Open File affordance and tab context menu only. The Open File button keeps its current icon and click behavior. Secondary interaction opens a small popup with a maximum of 7 rows: a short `Recently loaded` section, optionally a `Recently closed` section, and no explanatory body text. Rows should show file name first and path as subdued secondary text only if the popup design already supports two-line rows; otherwise use tooltip paths to keep the popup compact.

The tab context menu should not list many recent files at the root. Use one `Open Recent >` submenu in the file-actions group near `Open File Here`, with a disabled empty state only if the submenu is opened and there are no entries. Add `Reopen Closed File` as a single root row only when available, likely near close actions or after file actions; it should not appear when the stack is empty. This gives accidental-close recovery a clear home without making every tab menu look like a history panel.

The Settings Opening section should graduate the existing Recent files toggle from placeholder copy to real preference text. Keep it as a toggle card, and add a compact `Clear Recent Files` secondary action only if the settings design has an established secondary action style. Avoid a full recent-files management table in Settings until users need pinning, forgetting individual entries, or privacy controls.

## Implementation Sequence

1. Add recent-file storage and recording hooks around successful open paths, successful Save As path assignment, and explicit tab close for file-backed clean buffers.
2. Add app-level reopen methods that call the existing open pipeline and share duplicate handling with normal file open.
3. Add the Open File popup affordance in top and vertical primary actions without increasing the visible button count.
4. Add `Open Recent >` and conditional `Reopen Closed File` to the tab context menu with tight row limits.
5. Update the Settings Opening toggle copy and wire it to both recording and UI visibility.
6. Add tests for dedupe, ordering, cap size, disabled setting behavior, missing-file failure handling, already-open activation, and recent-closed filtering.

## Validation

Manual validation should cover top-tab and vertical-tab layouts at narrow widths, since those are the places UI overload is most likely. Verify that Open File still opens the picker on normal click, the recent popup does not shift toolbar layout, the tab context menu stays scannable, and recent-closed recovery appears only when it can do useful work. Also verify privacy and failure cases: disabling Recent files hides the UI, missing paths fail gracefully, and clearing history removes both recently loaded and recently closed entries.
