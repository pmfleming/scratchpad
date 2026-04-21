# Context Menu Command Plan

This document recommends the right-click command sets for the editor area and the tab area.

The goal is to keep the menus small, predictable, and mostly backed by the existing command system instead of creating one-off UI-only actions.

## Goals

- Define a practical right-click menu for the editor area.
- Define a practical right-click menu for the tab area.
- Prefer commands that already exist in `AppCommand`.
- Call out the missing commands that should be added before the menus are fully implemented.

## Current Constraints

- Context menus are still limited in the current product.
- The command palette is planned but not available yet.
- Some useful context-menu actions are available only through shortcuts or direct UI wiring, not `AppCommand`.
- Some file-system actions that are standard in tab menus do not exist yet.
- New files should expose `Save As` rather than pretending they already have a file-backed `Save` target.
- Existing file renaming is already tied to double-clicking the file name in the tab list, so rename does not need to be a primary context-menu action.

## Design Rules

### 1. Use context menus as a secondary command surface

Right-click should expose common actions that are already meaningful in the current context.

It should not become a dumping ground for every possible command.

### 2. Select the target before showing the menu

- Right-click on a tab should activate that tab before the menu opens.
- Right-click in an editor tile should target the tile under the pointer before the menu opens.

This keeps command behavior consistent with what the user sees as selected.

### 3. Reuse command dispatch where possible

Preferred flow:

1. context menu item
2. app-level command or app method
3. existing execution path

If a menu item cannot be expressed through the existing command surface, it should be treated as a gap and named explicitly.

### 4. Use mixed presentation styles inside the same menu

Not every context-menu command needs the same visual treatment.

Recommended pattern for the editor-area menu:

- fast clipboard and selection actions use compact icon buttons in a bottom action rail
- medium-frequency commands such as `Undo`, `Find`, and `Replace` stay as normal rows
- more structural commands such as split directions use a callout or submenu row with a trailing chevron

This keeps the menu short while still exposing more advanced actions.

### 5. Reserve row items for commands with text-heavy meaning

Commands that require interpretation should remain textual.

Examples:

- `Undo`
- `Find`
- `Replace`
- `Move Tile To New Tab`
- `Close Tile`

Commands that are already broadly recognizable can move into the icon rail.

Examples:

- Cut
- Copy
- Paste
- Select All

### 6. Match file actions to file state

The menu should not show the same save actions for every tab.

- untitled or not-yet-saved files should prefer `Save As`
- existing file-backed tabs should show `Save`
- existing file rename stays on the current tab-list double-click behavior instead of adding a second rename entry here

This keeps the menu aligned with current product behavior and avoids redundant rename affordances.

## Editor Area Menu

The editor-area menu should focus on editing, search, and tile-level actions.

### Recommended Menu Anatomy

Use a two-zone structure.

#### Main list area

This contains ordinary row items and callout rows.

Recommended rows:

- Undo
- Redo
- Find
- Replace
- Split >
- Move Tile To New Tab
- Close Tile

#### Bottom icon rail

This contains the quickest text-edit actions as icon-first buttons.

Recommended icon actions:

- Cut
- Copy
- Paste
- Select All

This matches the interaction pattern in the reference menu more closely than treating all editor commands as equal-weight rows.

### Recommended Primary Commands

These should appear in the first version of the editor-area menu.

| Menu Label | Command Path | Notes |
| --- | --- | --- |
| Undo | `AppCommand::UndoActiveBufferTextOperation` | Already exists. |
| Redo | `AppCommand::RedoActiveBufferTextOperation` | Already exists. |
| Cut | bottom icon rail, direct editor clipboard action | Likely editor-surface action, not `AppCommand`. |
| Copy | bottom icon rail, direct editor clipboard action | Likely editor-surface action, not `AppCommand`. |
| Paste | bottom icon rail, direct editor clipboard action | Likely editor-surface action, not `AppCommand`. |
| Select All | bottom icon rail, direct editor selection action | Likely editor-surface action, not `AppCommand`. |
| Find | `AppCommand::OpenSearch` | Already exists. |
| Replace | `AppCommand::OpenSearchAndReplace` | Already exists. |
| Split | callout row | Opens a directional submenu instead of listing four split rows inline. |
| Move Tile To New Tab | `AppCommand::PromoteViewToTab` | Only when the tile can be promoted. |
| Close Tile | `AppCommand::CloseView` | Only when the tab has more than one tile. |

### Split Callout

The split command should not appear as four top-level menu entries.

Instead use a single row:

- `Split >`

That row opens a callout or submenu with these directional actions:

| Callout Label | Command Path | Mapping |
| --- | --- | --- |
| Split Left | `AppCommand::SplitActiveView` | `SplitAxis::Vertical`, `new_view_first: true` |
| Split Right | `AppCommand::SplitActiveView` | `SplitAxis::Vertical`, `new_view_first: false` |
| Split Up | `AppCommand::SplitActiveView` | `SplitAxis::Horizontal`, `new_view_first: true` |
| Split Down | `AppCommand::SplitActiveView` | `SplitAxis::Horizontal`, `new_view_first: false` |

This is a better fit than placing `Split Left`, `Split Right`, `Split Up`, and `Split Down` directly in the root menu.

### Recommended Secondary Commands

These are useful, but can wait until the primary menu is working.

| Menu Label | Command Path | Notes |
| --- | --- | --- |
| Save | `AppCommand::SaveFile` | Show for existing file-backed tabs. |
| Save As | `AppCommand::SaveFileAs` | Show for untitled files or when the user wants a new path. |
| Open File Here | `AppCommand::OpenFileHere` | Good fit for the tile context and should be present in the first practical version. |
| Toggle Line Numbers | proposed new app command | Shortcut exists today, but no `AppCommand` is exposed. |

### Editor Area Commands To Avoid Initially

- Font-size commands
- Settings navigation
- low-frequency debug or diagnostics actions
- actions that depend on unsupported editor features such as multi-cursor or drag-drop text

These would increase menu size faster than they improve usability.

## Tab Area Menu

The tab-area menu should focus on file-tab lifecycle, file metadata, and bulk tab operations.

### File-State Rules

- For new untitled tabs, show `Save As`.
- For existing file-backed tabs, show `Save`.
- Do not prioritize rename in the tab context menu because existing file names are already changed by double-clicking the name in the tab list.
- Keep `Open File Here` visible because it is a strong tab-area action for replacing or reusing the current workspace context.

### Recommended Primary Commands

These should appear in the first version of the tab-area menu.

| Menu Label | Command Path | Notes |
| --- | --- | --- |
| New Tab | `AppCommand::NewTab` | Already exists. |
| Open File | `AppCommand::OpenFile` | Already exists. |
| Open File Here | `AppCommand::OpenFileHere` | Include in the first version of the tab-area menu. |
| Save | `AppCommand::SaveFile` | Show for existing file-backed tabs. |
| Save As | `AppCommand::SaveFileAs` | Show for new untitled tabs and optional redirected saves. |
| Close Tab | `AppCommand::RequestCloseTab` | Safer default than immediate close. |
| Split Files Into Tabs | `AppCommand::PromoteTabFilesToTabs` | Already exists and fits tab-level context. |

### Recommended Secondary Commands

These are strong follow-up items after the base menu lands.

| Menu Label | Command Path | Notes |
| --- | --- | --- |
| Close Other Tabs | proposed new app command | Standard tab affordance. |
| Close Tabs To The Right | proposed new app command | Standard tab affordance. |
| Close All Tabs | proposed new app command | Useful for workspace reset. |
| Copy Path | proposed new app command or direct file action | Standard for file-backed tabs. |
| Copy File Name | proposed new app command or direct file action | Low-cost utility action. |
| Reveal In Explorer | proposed new app command or direct file action | Standard Windows file-tab action. |
| Duplicate Tab | proposed new app command | Only if duplicating a tab is a supported concept. |

### Advanced Tab Commands

These should not be in the first menu unless there is already a strong UI need.

| Menu Label | Command Path | Notes |
| --- | --- | --- |
| Combine Into Selected Tab | `AppCommand::CombineTabIntoTab` | Better suited to drag/drop or a dedicated combine workflow. |
| Combine Multiple Tabs | `AppCommand::CombineTabsIntoTab` | Too advanced for a first-pass context menu. |
| Reorder Tab | `AppCommand::ReorderTab` | Drag already covers this better than a menu. |

## Suggested Menu Layout

### Editor Area

Recommended ordering:

1. Undo
2. Redo
3. separator
4. Find
5. Replace
6. separator
7. Split >
8. Move Tile To New Tab
9. Close Tile
10. separator
11. bottom icon rail: Cut, Copy, Paste, Select All

### Tab Area

Recommended ordering:

1. New Tab
2. Open File
3. Open File Here
4. separator
5. Save or Save As, depending on whether the tab is file-backed
6. separator
7. Close Tab
8. Split Files Into Tabs
9. separator
10. Copy Path
11. Reveal In Explorer
12. separator
13. Close Other Tabs
14. Close Tabs To The Right
15. Close All Tabs

## Command Gaps To Add

These are the most important gaps if the menus should feel complete.

### Editor Area Gaps

- `ToggleLineNumbers`
- app-level wrappers for Cut, Copy, Paste, and Select All if consistent command dispatch is desired

### Tab Area Gaps

- `CloseOtherTabs`
- `CloseTabsToTheRight`
- `CloseAllTabs`
- `CopyActiveTabPath`
- `CopyActiveTabFileName`
- `RevealActiveTabInExplorer`

## Implementation Recommendation

### Phase 1

- Add editor-area right-click menu using the existing active tile and active buffer context.
- Add tab-area right-click menu using the existing tab activation flow.
- Reuse current `AppCommand` variants wherever possible.
- For clipboard actions in the editor area, call the existing editor interaction layer directly if needed.
- Implement the editor clipboard actions as a dedicated bottom icon rail instead of ordinary menu rows.
- Implement split as a single row that opens a directional callout.

### Phase 2

- Add the missing tab bulk-close commands.
- Add file-path utility actions for tab menus.
- Add a line-number toggle command to the shared command layer.

### Phase 3

- Normalize context-menu actions and command-palette actions behind a shared command metadata layer.
- Make menu enablement explicit so disabled actions reflect current tile/tab state.

## Recommended First Slice

If only a narrow first implementation is desired, start with this exact set.

### Editor Area First Slice

- Undo
- Redo
- Find
- Replace
- Open File Here
- Save for existing files, or Save As for untitled files
- Split callout with Left, Right, Up, and Down
- Close Tile
- Bottom icon rail: Cut, Copy, Paste, Select All

### Tab Area First Slice

- New Tab
- Open File
- Open File Here
- Save for existing files, or Save As for untitled files
- Close Tab
- Split Files Into Tabs

This first slice covers the highest-frequency actions without requiring a large command-surface expansion, and it keeps the root editor menu compact by pushing clipboard actions into an icon rail and directional split actions into a callout.