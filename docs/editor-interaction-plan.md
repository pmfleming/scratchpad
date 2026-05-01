This plan defines how Scratchpad's editor should behave from the user's point of view, with an emphasis on keyboard and mouse interactions.

It has two jobs:

1. serve as an implementation plan for the editor interaction model
2. serve as an interaction checklist so we do not overlook expected editor behaviors while the piece-tree editor keeps evolving

The source of truth for the "currently implemented" sections is the code, not the manual. The most relevant files today are:

- `src/app/ui/editor_content/native_editor/mod.rs`
- `src/app/ui/editor_content/native_editor/cursor.rs`
- `src/app/ui/editor_content/native_editor/editing.rs`
- `src/app/shortcuts.rs`
- `src/app/ui/search_replace/controls.rs`
- `src/app/ui/editor_area/tile.rs`
- `src/app/ui/editor_area/divider.rs`
- `src/app/ui/tile_header/mod.rs`

## Scope

This document covers:

- text editing interactions inside the active editor
- selection and caret behavior
- clipboard, undo, redo, and search-related editing flows
- mouse behavior in the editor surface
- tile-level interactions that directly affect editing workflow
- keyboard shortcuts that act on the active editor or active tile

This document does not try to fully spec:

- command palette behavior
- full settings-surface interaction design
- top-level tab overflow behavior in detail
- future multi-cursor or modal-editing systems

## Current Product State

Scratchpad currently has a native editor path built on top of piece-tree storage and a custom per-view cursor model.

That means we should not describe the editor as "whatever `egui::TextEdit` does." We now own the interaction contract directly.

### Currently Implemented Text Editing

The active editor currently supports:

- single-click caret placement
- `Shift + Click` selection extension
- click-drag selection
- plain text insertion
- `Enter` inserting the document's preferred line ending
- `Tab` inserting a literal tab character
- `Backspace` and `Delete`
- `Ctrl` or `Alt` plus `Backspace` or `Delete` for word-wise deletion
- arrow-key navigation
- `Ctrl` or `Alt` plus `Left` or `Right` for word-wise navigation
- `Home` and `End`
- command-modifier `Up` and `Down` for document start and end
- command-modifier `Home` and `End` for document start and end
- `Shift` plus movement keys for selection extension
- `Ctrl + A` select all
- copy, cut, and paste through normal OS clipboard events
- `Ctrl + Z` undo
- `Ctrl + Y` redo
- IME commit text insertion

Important current details:

- word navigation and word deletion are whitespace-based, not punctuation-aware
- vertical movement is row-based through the laid-out galley, which means it follows visual rows
- selection collapse on plain `Left` and `Right` is implemented
- `Shift + Tab` does not currently outdent
- double-click word selection is not currently implemented
- triple-click line selection is not currently implemented
- page-wise navigation is not currently implemented
- drag-and-drop text move/copy is not currently implemented
- context-menu editing is still limited

### Currently Implemented Editor-Adjacent Shortcuts

These shortcuts already exist and the plan should treat them as reserved unless we intentionally change them:

- `F1`: open user manual
- `Ctrl + F`: open search focused on the find field
- `Ctrl + H`: open search focused on the replace field
- `Ctrl + ,`: open settings
- `Esc`: close settings or search when applicable
- `Ctrl + N`: new tab
- `Ctrl + O`: open file
- `Ctrl + Shift + O`: open file into the current workspace
- `Ctrl + S`: save active file
- `Ctrl + +` or `Ctrl + =`: increase editor font size
- `Ctrl + -`: decrease editor font size
- `Ctrl + Mouse Wheel`: zoom editor font while pointer is over the editor workspace
- `Ctrl + 0`: toggle line numbers
- `Ctrl + W`: close active tab
- `Ctrl + T`: promote active tile to its own tab
- `Ctrl + Shift + T`: promote all files in the active workspace into tabs
- `Ctrl + Shift + W`: close active tile
- `Ctrl + Shift + Arrow`: split active tile

### Currently Implemented Search-Strip Keys

When the find field has focus:

- `Enter`: next match
- `Shift + Enter`: previous match
- `Esc`: close search

When the replace field has focus:

- `Enter`: replace current match
- `Esc`: close search

### Currently Implemented Mouse and Tile Behavior

The workspace editing surface currently supports:

- clicking a tile body to activate that tile
- hover-only tile header controls
- dragging the tile split handle to create a split
- dragging split dividers to resize panes
- clicking tile close and promote controls

## Goals

1. Make Scratchpad feel like a serious keyboard-first editor without making mouse use awkward.
2. Make text-edit behavior predictable across normal text files, large files, and artifact-heavy files.
3. Keep focus, selection, and search behavior coherent across tiled views.
4. Separate what is a text-edit action from what is a workspace action.
5. Avoid shortcut collisions with commands that already exist.
6. Make the interaction model specific enough that it can be tested.

## Interaction Principles

### 1. Focus Must Be Explicit

At any given moment, one of these should be clearly true:

- the active editor tile owns keyboard editing
- the search strip owns keyboard editing
- settings owns keyboard editing
- tab rename owns keyboard editing

The plan should avoid "mystery focus" cases where text-edit keys are consumed by the wrong surface.

### 2. Text Editing And Workspace Control Must Stay Distinct

Text-edit keys should operate inside the focused editor.

Workspace keys should operate on the active tile or workspace.

Examples:

- `Backspace` in the editor deletes text
- `Ctrl + Shift + W` closes the active tile
- `Esc` should prefer dismissing the active transient surface before altering editor state

### 3. Selection Semantics Must Be Stable

All selection-producing actions should follow one shared contract:

- plain movement collapses selection unless the behavior explicitly extends or preserves it
- `Shift` extends selection from the existing anchor
- selection ranges remain character-based in piece-tree coordinates
- passive views may render the active buffer selection, but the focused view owns the live caret

### 4. Visual-Row And Logical-Line Behavior Must Be Deliberate

Scratchpad now has both:

- logical lines in the document model
- visual rows in the laid-out viewport

Every navigation command must be explicit about which one it uses.

## Canonical Interaction Contract

### A. Text Input

Target behavior:

- printable text inserts at the caret or replaces the selection
- `Enter` inserts the file's preferred line ending
- `Tab` inserts a tab when no indentation command is defined for the current context
- paste inserts normalized text and replaces the active selection if one exists

Implementation note:

- this is already mostly implemented and should be preserved

### B. Caret Movement

Target behavior:

- `Left` and `Right`: character movement
- `Ctrl + Left` and `Ctrl + Right`: word movement
- `Up` and `Down`: visual-row movement
- `Home` and `End`: visual-row start and end
- `Ctrl + Home` and `Ctrl + End`: document start and end
- `Page Up` and `Page Down`: viewport-sized movement while preserving horizontal intent

Current gap:

- `Page Up` and `Page Down` are still missing

### C. Selection

Target behavior:

- `Shift` plus any caret movement extends selection
- click places caret
- `Shift + Click` extends selection
- drag creates or updates a selection
- double-click selects word
- triple-click selects line
- `Ctrl + A` selects the whole document

Current gaps:

- double-click word selection
- triple-click line selection
- line selection from gutter interactions

### D. Word Semantics

Target behavior:

- word movement, word selection, and word deletion should all use the same boundary rules
- punctuation handling should be deliberate rather than accidental

Current gap:

- current word semantics are whitespace-only and should be upgraded to a shared boundary helper

### E. Deletion

Target behavior:

- `Backspace` deletes left
- `Delete` deletes right
- selection deletion replaces or removes the selected range
- word-wise delete uses the same boundary rules as word movement

Implementation note:

- baseline behavior already exists

### F. Clipboard

Target behavior:

- copy copies selection only
- cut copies then deletes selection only
- paste replaces selection if present
- copy with an empty selection should be explicitly decided rather than left ambiguous

Open question:

- should future behavior support line copy/cut when no selection exists, or stay conservative?

### G. Undo And Redo

Target behavior:

- editor text edits use operation-based undo/redo
- search-driven replacements should restore both text and selection coherently
- workspace actions are not part of undo/redo history

Shortcut rule:

- keep `Ctrl + Z` for text undo
- keep `Ctrl + Y` for text redo
- do not bind `Ctrl + Shift + Z` to undo or redo

### H. Search And Replace Focus

Target behavior:

- `Ctrl + F` focuses find
- `Ctrl + H` focuses replace
- find-field `Enter` navigates results
- replace-field `Enter` executes replace-current
- `Esc` closes search and returns focus to the active editor

This area is already mostly defined and should be treated as part of the editor interaction contract, not as a separate afterthought.

### I. Mouse Behavior

Target behavior:

- hover shows text cursor over editable text
- single click places caret and focuses the editor
- drag selects
- `Shift + Click` extends selection
- double-click selects word
- triple-click selects line
- wheel scroll scrolls the editor viewport
- `Ctrl + Mouse Wheel` zooms only when pointer is over the editor workspace

Future-only behaviors that should stay explicitly out of scope until chosen:

- alt-click multi-cursor
- drag-and-drop text move/copy
- rectangular selection

### J. Tile And Pane Behavior

Target behavior:

- clicking an inactive tile activates it
- tile chrome remains secondary to editing, not visually dominant
- split creation and split resize should not steal normal text-selection gestures inside the editor body
- active-tile focus handoff should be immediate after split or tile activation

## Interaction Checklist

This checklist is the "did we overlook anything?" section. Every item should eventually be marked as one of:

- shipped
- intentionally deferred
- rejected

### Keyboard Editing Checklist

- printable character insertion
- IME commit insertion
- `Enter`
- `Tab`
- `Shift + Tab`
- `Backspace`
- `Delete`
- word-wise backspace/delete
- `Left` and `Right`
- word-wise `Left` and `Right`
- `Up` and `Down`
- `Home` and `End`
- document start and end
- `Page Up` and `Page Down`
- select all
- undo
- redo
- copy
- cut
- paste

### Selection Checklist

- click to place caret
- `Shift + Click`
- drag selection
- double-click word selection
- triple-click line selection
- selection collapse behavior on plain movement
- selection preservation across search navigation
- selection restore after undo and redo

### Mouse And Viewport Checklist

- text cursor on hover
- wheel scroll
- `Ctrl + Mouse Wheel` zoom
- viewport scroll-to-caret after movement
- viewport scroll-to-match after search navigation
- visible-range rendering correctness while selection spans offscreen content

### Search Checklist

- open search from editor
- open replace from editor
- focus restore after closing search
- next and previous match keys
- replace-current key
- replace-all trigger behavior
- selection-only search interaction with a live editor selection

### Tile And Workspace Checklist

- click inactive tile to activate
- split active tile shortcut
- split handle drag
- divider drag resize
- close active tile shortcut
- close active tab shortcut
- promote tile shortcut
- line-number toggle
- zoom controls

### Edge-Case Checklist

- empty document behavior
- single-line file behavior
- very large file behavior
- wrapped-line movement behavior
- combining-character movement and deletion behavior
- clipboard and IME behavior with Unicode content
- search highlight behavior after edits
- undo/redo behavior after replace-all

## Implementation Plan

### Phase 1: Freeze The Baseline Contract

Goal:

- document the current shipped behavior accurately
- add targeted tests for the current native editor behavior we want to preserve

Deliverables:

- this plan
- tests for current caret movement, selection extension, word-wise deletion, and shortcut conflicts

### Phase 2: Fill Core Editing Gaps

Goal:

- bring the editor up to a strong baseline text-editor contract

Recommended work:

- add `Page Up` and `Page Down`
- add double-click word selection
- add triple-click line selection
- add `Shift + Tab` behavior for selected lines or define that it remains literal
- unify word-boundary rules across movement and deletion

### Phase 3: Clarify Focus And Search Handoffs

Goal:

- make focus transitions predictable between editor, search, rename, and settings

Recommended work:

- codify `Esc` priority rules
- codify when focus returns to the editor after search actions
- add tests for search-open, search-close, replace-current, and post-action focus retention

### Phase 4: Solidify Mouse And Viewport Semantics

Goal:

- make the visible-window model and editor interaction model agree

Recommended work:

- ensure click-hit-testing and selection work correctly with active viewport slicing
- define row-based versus logical-line semantics explicitly in tests
- add viewport-aware cursor movement tests around wrapped content

### Phase 5: Workspace And Tile Polish

Goal:

- make tile-level interactions feel complete without undermining text editing

Recommended work:

- verify split, resize, activate, and close interactions against the active-editor focus model
- verify zoom and line-number toggles stay scoped correctly
- document any tab-strip behaviors that should count as part of the editor workflow

## Recommended Immediate Next Steps

1. Treat this document as the interaction source of truth for editor work.
2. Add tests for the currently shipped native-editor behaviors before expanding the interaction surface further.
3. Implement the missing baseline items in this order:
   `Page Up/Page Down`, double-click word select, triple-click line select, shared word-boundary rules.
4. Keep `Ctrl + Y` as the only redo shortcut.
5. Update the user manual only after the code and this plan agree.
