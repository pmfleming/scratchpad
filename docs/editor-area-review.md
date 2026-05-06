# Editor Area, Scrolling & Selection Review

Review of Scratchpad's editor area, native text editor, scrolling, and selection model.

Date: 2026-05-06

This rewrite intentionally treats Scratchpad as a **Windows-first plain-text editor**. It is not a code editor plan. Code-editor affordances such as syntax-aware navigation, minimap, folding, multi-cursor editing, Vim/Emacs chords, and smart-indent Home behavior are not baseline requirements unless they also serve ordinary plain-text editing.

No code was modified for this review.

---

## 1. Product Standard

Scratchpad should feel like a standard Windows multiline text editor with stronger capacity handling.

The editor should prefer a **single behavior path**:

- One editor model for small and large files.
- One selection model across files, tabs, and split tiles.
- One scrolling model across mouse wheel, scrollbar, keyboard navigation, reveal, and drag-selection autoscroll.
- One rendering path whose internals can choose the cheapest representation for the current capacity case.

"Single path" does not mean doing the same amount of work for every file. It means the user-visible semantics stay identical while the implementation picks the best data path for large files, many tabs, and many tiles.

The first compatibility target is Windows. Keyboard and mouse interactions should match Windows text editing conventions first, then add Scratchpad-specific shortcuts for non-standard app features such as tabs, tiles, history, encoding, and search.

---

## 2. Research Baseline

External references used for this rewrite:

- [Keyboard shortcuts in Windows](https://support.microsoft.com/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec): default Windows text editing shortcuts, including arrows, word movement, page movement, selection extension, clipboard, undo/redo, find, replace, and Tab indentation.
- [About Win32 edit controls](https://learn.microsoft.com/en-us/windows/win32/controls/about-edit-controls): Windows edit controls expose a blinking caret, keyboard/mouse text entry, movement, selection, multiline behavior, horizontal/vertical scrolling, and Unicode support.
- [Edit control text operations](https://learn.microsoft.com/en-us/windows/win32/controls/edit-controls-text-operations): standard selection ranges, replacement, clipboard operations, and the built-in edit-control context menu with Undo, Cut, Copy, Paste, Delete, and Select All.
- [Keyboard accelerators for Windows apps](https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators): accelerators should be consistent across Windows apps, scoped where appropriate, discoverable through menu labels/tooltips, and especially complete for common commands.
- [Input Method Editors](https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editors): apps with text input should test end-to-end IME entry and avoid occluding candidate windows or touch keyboard UI.

Those sources point to a simple rule: standard text-control behavior is the compatibility floor. Scratchpad-specific features should fit around that floor, not overwrite it.

---

## 3. Current Architecture Read

The previous review described a good shape:

1. **Pane tree -> tiles.** The editor area walks a split/leaf pane tree. Leaves render editor tiles with header chrome, body content, context menu, and tile actions collected for later application.

2. **Editor content frame.** The content layer combines gutter, native editor, and fallback artifact display. It chooses the active view from tab state and keeps focus styling separate from text layout.

3. **Native text editor.** The native editor builds a visible text layout, processes input, paints text/caret/selection, and stores per-view state.

4. **Scrolling.** A per-view `ScrollManager` owns the application-level scroll truth, while egui's scroll state is the pixel/UI bridge. Scroll requests flow through explicit intents.

This is directionally right for the product. The important architectural requirement is to keep the editor as a single plain-text editor path, not a collection of special paths for "large file mode", "split mode", "search mode", or "selection mode".

---

## 4. Windows Text Editing Contract

These are baseline expectations for the editor when focus is inside text.

### Cursor Movement

| Interaction | Expected Windows/plain-text behavior |
| --- | --- |
| Left / Right | Move one character or grapheme. Collapse an existing selection to the corresponding edge when Shift is not held. |
| Ctrl+Left / Ctrl+Right | Move by word boundary. |
| Up / Down | Move by visual line while preserving preferred x position. Must continue beyond the visible slice. |
| Home / End | Move to beginning/end of current line. For Scratchpad's plain-text baseline, use literal line edges, not code-editor smart indent. |
| Ctrl+Home / Ctrl+End | Move to beginning/end of document. |
| PageUp / PageDown | Move the caret by roughly one viewport page, not merely to the current visible slice edge. |
| Ctrl+Up / Ctrl+Down | Move by paragraph if supported; otherwise leave unbound until a paragraph model is defined. |

### Selection

| Interaction | Expected Windows/plain-text behavior |
| --- | --- |
| Shift+movement | Extend selection from the anchored end. |
| Ctrl+Shift+Left / Right | Extend by word. |
| Shift+Home / End | Extend to line start/end. |
| Ctrl+Shift+Home / End | Extend to document start/end. |
| Shift+PageUp / PageDown | Extend by page. |
| Mouse click | Move insertion point. |
| Shift+click | Extend selection to clicked position. |
| Drag | Select continuously; autoscroll when dragging outside the viewport. |
| Double click | Select word. |
| Triple click | Optional line-selection convenience; useful if already present, but not the core Windows compatibility baseline. |

### Editing And Clipboard

| Interaction | Expected Windows/plain-text behavior |
| --- | --- |
| Backspace / Delete | Delete left/right character, or replace current selection. |
| Ctrl+Backspace / Ctrl+Delete | Delete previous/next word. |
| Enter | Insert newline in the editor. |
| Tab | Insert a tab/indent in the editor while text has focus. App focus traversal belongs outside text focus or behind explicit UI commands. |
| Ctrl+A | Select all text in the focused editor. |
| Ctrl+C / Ctrl+Insert | Copy selected text. |
| Ctrl+X | Cut selected text. |
| Ctrl+V / Shift+Insert | Paste clipboard text. |
| Ctrl+Shift+V | Paste as plain text if the clipboard can contain rich formats. Since Scratchpad is plain text, normal paste may already satisfy this. |
| Ctrl+Z / Ctrl+Y | Undo / redo. |
| Alt+Backspace | Optional undo compatibility with classic edit controls. |

Formatting shortcuts listed by Windows for rich text, such as Ctrl+B, Ctrl+I, and Ctrl+U, should not become editor commands in a plain-text editor. Either leave them unused while text is focused or reserve them only for explicit non-editor surfaces where they do not violate text expectations.

### Context Menu

The editor context menu should include the standard edit-control actions where applicable:

- Undo
- Cut
- Copy
- Paste
- Delete
- Select All

Scratchpad-specific actions can be present, but they should not displace the standard editing group. Right-click behavior should respect the selection: right-clicking inside an existing selection should operate on that selection; right-clicking elsewhere should move the caret or select the clicked context according to the app's chosen Windows-like rule.

### App Accelerators Around The Editor

App-level shortcuts should follow Windows conventions and should be scoped so they do not steal text editing behavior:

- Ctrl+F: Find.
- Ctrl+H: Replace.
- F3 / Shift+F3: Next/previous search result.
- Ctrl+S: Save.
- Ctrl+O: Open.
- Ctrl+N or Ctrl+T: New tab/new document, but choose deliberately and show it in menus/tooltips.
- Ctrl+W: Close current tab/document.
- F6 / Shift+F6: Move focus between panes/regions. This is preferable to code-editor-specific directional pane chords for the baseline.

---

## 5. Scrolling Review

The existing anchor-based design is the right foundation.

Strengths:

- A logical/piece anchor is more robust than raw pixel offsets when edits happen above the viewport.
- A single `ScrollIntent` pipeline is easier to reason about than scattered offset mutations.
- Reveal, wheel, scrollbar, page movement, and drag-autoscroll all fit the same model.
- Edge-autoscroll during drag-selection is a standard multiline editor behavior and belongs in the core path.

Compatibility requirements:

- Mouse wheel should scroll the viewport without moving the caret.
- Keyboard navigation should move the caret and reveal it only as much as needed.
- PageUp/PageDown should move the caret by a page, not just scroll the viewport and not stop at the visible-layout boundary.
- Horizontal scroll should exist when word wrap is off. With word wrap on, horizontal scroll should normally be unnecessary.
- Scrollbar dragging should update the viewport without corrupting the caret/selection anchor.
- Programmatic reveal should distinguish typing from jumps: typing should keep the caret visible with minimal movement; explicit jumps/search results can center or use a stronger alignment.

Primary issue:

**Vertical and page cursor movement must be computed against the full document, not only the current visible galley.** The earlier review identified that visible-slice layout can constrain Up/Down/Page movement at viewport boundaries. For a standard text editor this is a correctness issue, not just a performance issue. Holding ArrowDown at the bottom of the viewport should continue smoothly through the document. PageDown should move by roughly one viewport page.

Implementation direction:

- Keep the single scroll manager and intent path.
- Add or reuse full-document line/row mapping for cursor movement.
- Use visible galleys for painting and hit testing, but do not make them the only source of truth for navigation.
- Preserve the anchor-based scroll model so capacity behavior stays stable for large files.

---

## 6. Selection Review

The existing `CursorRange` shape, with primary and secondary endpoints, is a good plain-text editor model.

Strengths:

- Single range selection is the correct baseline.
- Shift extension by keeping the fixed endpoint is standard.
- Word selection using the underlying text model rather than just the visible slice is the right capacity-aware choice.
- Drag-selection autoscroll belongs in the core editor path.

Compatibility requirements:

- Selection should be char/grapheme correct. Word movement and deletion should respect Unicode text, not only ASCII.
- Double-click word selection should cross visible-slice boundaries.
- Selection painting should be stable while dragging and should not force a full editor-mode change.
- Copy/cut should use the exact selected plain text.
- Replace-selection should be the single path for typed text, paste, delete, and IME commit.

Important gap:

**IME preedit/composition support remains a real plain-text editor requirement.** This is not a code-editor feature. Windows text input must work for users entering East Asian and other composed text. Scratchpad should show composition text and keep the IME candidate window aligned with the caret, or at minimum explicitly test and document the current behavior before calling the editor complete.

Non-goals for the baseline:

- Multi-cursor.
- Column/block selection.
- Add-next-occurrence.
- Syntax-aware word objects.

Those can be future optional features, but they should not drive the editor architecture now.

---

## 7. Capacity Review

The capacity goal is not "add a large-file mode". The goal is that the normal editor path automatically picks the cheapest internal representation.

Keep:

- Piece-tree-backed text storage.
- Anchor-based scroll state.
- Visible-slice layout.
- Per-view scroll state.
- Deferred intents for reveal and scroll updates.
- Bounded caches.

Improve:

1. **Navigation mapping.** Full-document vertical/page navigation needs a capacity-safe row/line mapping so cursor movement remains standard without laying out the entire file.

2. **Layout cache split.** The previous review noted that selection and search highlights can churn the visible galley cache. For plain text, structure and decoration should be separated where possible: cache text layout by content/font/wrap/viewport, then paint selection/search overlays without making every selection drag a structural cache miss.

3. **Inactive tile cost.** Many tiles and many tabs should not imply many full editor layouts per frame. Inactive/offscreen views should preserve scroll/cursor state but avoid unnecessary galley rebuilds.

4. **Search highlight targeting.** Update only views/buffers that actually carry search highlights. Do not walk every view on every search state change if the data model can target less work.

5. **Fragmentation pressure.** If visible-slice extraction repeatedly allocates because the piece tree is fragmented, surface a coalescing/rebalance hint through the existing data structure rather than introducing a separate render mode.

6. **Display snapshot reuse.** Display row metadata derived from the galley should be cached with the structural layout when possible.

The desired end state: the same editor behavior remains fast for a tiny note, a large text file, many tabs, and split tiles.

---

## 8. Revised Findings

### High Priority

1. **Full-document vertical and page navigation.** Fix Up/Down/PageUp/PageDown so they behave like a standard Windows multiline text editor across visible-slice boundaries.

2. **Windows shortcut parity audit.** Verify the editor-focus shortcut matrix against Windows text editing conventions: movement, selection, deletion, clipboard, undo/redo, find/replace, save/open, and pane traversal.

3. **IME preedit/composition.** Support or explicitly verify composition lifecycle, candidate window placement, and committed text insertion.

4. **Standard editor context menu.** Ensure Undo/Cut/Copy/Paste/Delete/Select All are present and correctly enabled before Scratchpad-specific commands.

5. **Tests for the text editor contract.** Add tests for cursor movement at slice boundaries, Shift extension, word deletion, page movement, anchor recovery, reveal behavior, and drag-autoscroll.

### Medium Priority

6. **Separate structural layout cache from decoration.** Reduce selection/search drag churn without changing behavior.

7. **Consolidate scroll truth.** Avoid duplicated `user_scrolled` state unless both layers have clearly different meanings.

8. **Encapsulate scroll state mutation.** Keep all scroll changes flowing through intent/manager APIs; UI pixel state should be a bridge, not another authority.

9. **Reduce inactive tile work.** Preserve state for many tabs/tiles while doing heavy layout only where needed.

10. **F6 / Shift+F6 pane traversal.** If split tiles are central to the app, provide standard Windows focus traversal rather than code-editor-specific pane chords.

### Low Priority

11. **Alt+Backspace undo compatibility.** Classic edit controls support it; nice to have.

12. **Triple-click line selection.** Keep if it works well, but do not treat it as a baseline blocker.

13. **Expose or remove scroll-past-EOF setting.** If the implementation already has a parameter, either make it an intentional setting or simplify it.

14. **Document non-goals.** Make it explicit that multi-cursor, minimap, syntax highlighting, folding, and smart-indent navigation are not current goals.

---

## 9. Items Removed From The Old Review

These were code-editor shaped and should not drive the plain-text editor roadmap:

- Smart Home as a top quick win. Literal Home/End line-edge behavior is the Windows plain-text baseline. Smart indent can be an optional setting later, not a default priority.
- Multi-cursor / column selection as a "gap". This is a non-goal unless explicitly requested.
- Minimap, sticky line, virtualized code gutter, and syntax-oriented affordances as expected features.
- Vim-like pane navigation suggestions. Use F6/Shift+F6 first for Windows compatibility.
- Rich-format copy concern. Plain text copy is correct for Scratchpad unless a future feature introduces rich rendering semantics.

---

## 10. Prioritized Improvement List

| # | Category | Item | Effort |
| --- | --- | --- | --- |
| 1 | Correctness / UX | Full-document vertical and page navigation beyond visible-slice boundaries | M |
| 2 | Compatibility | Windows text shortcut parity audit and fixes | M |
| 3 | Functionality | IME preedit/composition and candidate placement | M |
| 4 | UX | Standard edit context menu group | S |
| 5 | Correctness | Tests for movement, selection, reveal, anchor recovery, and autoscroll | M |
| 6 | Capacity | Split structural layout cache from selection/search decoration | M |
| 7 | Capacity | Reduce inactive tile/tab layout work | S |
| 8 | Architecture | Consolidate scroll truth and encapsulate `ScrollState` mutation | S |
| 9 | Accessibility / UX | F6 / Shift+F6 region and tile focus traversal | S |
| 10 | Capacity | Search highlight targeting for only affected views | S |
| 11 | Capacity | Fragmentation/coalescing hint for repeated visible-slice allocation | S |
| 12 | UX | Audit reveal alignment: typing minimal, jumps/search stronger | XS |
| 13 | Compatibility | Alt+Backspace undo, if not already handled | XS |
| 14 | Documentation | State code-editor features as non-goals | XS |

XS = under an hour; S = under a day; M = multi-day.

---

## 11. Best First Work

1. **Fix full-document vertical/page navigation.** This is the largest standard-editor correctness issue and the one most likely to be felt immediately.

2. **Write the Windows shortcut parity matrix as tests.** It turns the compatibility goal into something enforceable and prevents accidental code-editor shortcuts from stealing standard text behavior.

3. **Add IME composition support or a measured compatibility note.** This is part of Windows text input, not an advanced feature.

4. **Make the context menu standard first.** Users expect the edit-control menu vocabulary before app-specific actions.

These keep Scratchpad on the intended path: a fast, capacity-aware plain-text editor that behaves like a Windows editor before it behaves like anything else.
