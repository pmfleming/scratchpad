# Search / Replace Implementation Plan For Scratchpad

This document translates `docs/search-replace-plan.md` into a concrete implementation plan for the current Scratchpad codebase.

It now also reflects the editor-state foundation work that has already landed since the earlier draft of this implementation plan.

The existing search/replace plan is useful as a product-direction document, but it currently mixes together several different concepts:

- in-editor text search
- cross-buffer replacement
- tab lookup (`Find Tab`)
- future provider-model extensibility
- advanced features like regex, transactions, and multi-cursor promotion

Scratchpad is still not ready to land all of that in one pass, but it is in a much better position than it was when the first draft of this implementation plan was written.

This plan keeps the current UX intent, but restructures delivery into phases that match the architecture already in `src/app/`.

## 1. Original Goals We Should Preserve

The original `docs/search-replace-plan.md` is still the right product reference for the user-facing goals.

The implementation plan should continue to optimize for these outcomes:

- a compact unified search/replace strip rather than separate search and replace surfaces
- immediate in-editor feedback while typing
- next / previous match navigation
- match count and active match index
- replace current and replace all
- scope selection from the same UI
- a non-modal workflow that keeps the editor usable while search is open

Those goals remain valid. What changes in this document is how we land them in the current codebase.

## 2. Foundation Already Completed

The earlier version of this implementation plan treated three editor limitations as blockers:

- no document model
- no persisted caret / selection state
- no general undo / redo support

Those are no longer true in the same form.

Recent improvements now available in the codebase:

- `BufferState` owns a `TextDocument` instead of exposing a raw public text field.
- `TextDocument` implements `egui::TextBuffer`, which means the editor can still render and mutate it through `egui::TextEdit` without a parallel widget-only buffer.
- `TextDocument` now owns document-level undo history via `egui::util::undoer::Undoer<(CCursorRange, String)>`.
- `EditorViewState` now stores `cursor_range`, `pending_cursor_range`, and `search_highlights`.
- `src/app/ui/editor_content/text_edit.rs` now synchronizes `TextDocument` undo state into `egui::TextEditState` before render and captures both updated undo state and `cursor_range` after render.
- explicit editor undo depth is now defined as 100 undo levels per text document.

This changes the search/replace plan materially:

- active-buffer search highlighting is now a straightforward extension of existing view state
- navigation-to-match can now drive selection directly through `pending_cursor_range`
- single-document replace flows can preserve document undo history instead of inventing a second undo model

## 3. Current Codebase Reality

### Existing seams we should build on

- `src/app/ui/editor_content/text_edit.rs` already owns the custom `TextEdit` layouter and captures the latest `Galley` into `RenderedLayout`.
- `src/app/ui/editor_content/text_edit.rs` now also owns the bridge between `TextDocument`, `egui::TextEditState`, cursor state, and document undo history.
- `src/app/ui/editor_area/mod.rs` owns the active workspace render path and is the right place to mount a search strip above the pane tree.
- `src/app/app_state.rs` is the top-level owner of cross-workspace UI state and should own the active search session.
- `src/app/commands.rs` and `src/app/shortcuts.rs` are already the command/shortcut entry points.
- `src/app/ui/tab_strip/actions.rs` already exposes a search button, but it is still a placeholder.
- `WorkspaceTab`, `BufferState`, and `EditorViewState` already distinguish between:
  - the active view/buffer
  - multiple buffers inside one workspace tab
  - multiple tabs across the app

### Constraints that matter

- Search still has no app-owned state, commands, or UI module.
- Search highlighting is not yet rendered, even though the view state now has a place to store it.
- Cross-buffer replace-all still cannot reasonably satisfy a global one-step `Ctrl+Z` guarantee. Undo now exists per `TextDocument`, not as a cross-document transaction log.
- `Find Tab` is still a different feature from text search and should not be coupled to replace workflows.

## 4. Scope Reconciliation

To keep the feature shippable, split the work into two tracks.

### Track A: in-editor search / replace

This is the feature that should ship first.

First-pass scope:

- open search with `Ctrl + F`
- open search + replace with `Ctrl + H`
- show a unified strip inside the active editor area
- search the active buffer immediately while typing
- navigate next / previous matches
- show match count and active match index
- replace current match
- replace all matches in the selected scope
- support three text scopes:
  - active buffer
  - active workspace tab
  - all open tabs

Explicitly out of first pass:

- regex
- project-wide search on disk
- search in unopened files
- multi-cursor promotion (`Ctrl + D`)
- cross-document single-step undo for replace-all
- anchoring the search strip to the toolbar button geometry

Clarification on undo expectations:

- single-document replace current should preserve normal editor undo/redo behavior
- single-document replace all should be designed so one `Ctrl + Z` can revert the operation if we route it as one document mutation boundary
- multi-document replace all should not promise one global undo step in the first pass

### Track B: tab lookup (`Find Tab`)

Treat `Find Tab` as a follow-up feature. It belongs in tab-strip / overflow UI, not in the same data path as text replacement.

Reason:

- it searches tab metadata, not buffer content
- it has no meaningful replace flow
- it introduces a second result model that would complicate the first release

## 5. Proposed Architecture

### App-level search session

Add a dedicated search state owned by `ScratchpadApp`.

Recommended shape:

```rust
pub(crate) struct SearchState {
    pub open: bool,
    pub replace_open: bool,
    pub query: String,
    pub replacement: String,
    pub scope: SearchScope,
    pub match_case: bool,
    pub active_match_index: Option<usize>,
    pub results: Vec<SearchMatch>,
    pub pending_focus: SearchFocusTarget,
    pub source: SearchSourceContext,
}

pub(crate) enum SearchScope {
    ActiveBuffer,
    ActiveWorkspaceTab,
    AllOpenTabs,
}
```

`SearchSourceContext` should capture which tab/view/buffer was active when search opened so navigation and strip-close behavior can restore editor focus predictably.

Also add explicit search UI focus intent so the strip can move keyboard focus between:

- find input
- replace input
- editor view

### Search engine module

Add a pure text-search module under `src/app/services/`, for example:

- `src/app/services/search.rs`

Responsibilities:

- resolve targets from the selected scope
- compute plain-text matches
- support case-sensitive and case-insensitive search
- return match ranges in character offsets, not bytes
- provide replace helpers for:
  - replace current
  - replace all in one buffer
  - replace all across multiple buffers

Keep the first engine plain and deterministic. Do not introduce a provider trait or regex engine in the first pass.

### Search target model

Use a small internal target struct instead of reaching into UI code directly:

```rust
struct SearchTarget {
    tab_index: usize,
    view_id: Option<ViewId>,
    buffer_id: BufferId,
    text: String,
}
```

Notes:

- `ActiveBuffer` should resolve to the active view's current buffer.
- `ActiveWorkspaceTab` should resolve all unique buffers inside the active `WorkspaceTab`.
- `AllOpenTabs` should resolve all unique buffers across the app.
- buffer deduplication must be by `BufferId`, not by label text.

Because `BufferState` now owns a `TextDocument`, this target should read from `buffer.text()` and write back through buffer/document helpers rather than managing independent copies for mutation.

### UI ownership

Add a new UI module for the strip, for example:

- `src/app/ui/search_replace.rs`

Render it from `src/app/ui/editor_area/mod.rs` above the pane-tree render path.

Why this placement is correct for the first implementation:

- it works for both top and vertical tab layouts
- it avoids fragile screen-space anchoring to the toolbar button
- it keeps the strip visually attached to the active editor workspace
- it avoids mixing search UI concerns into tab-strip layout code

The search toolbar button in `src/app/ui/tab_strip/actions.rs` should simply dispatch the open-search command.

## 6. Editor Integration

### Capture and restore caret / selection

Search navigation and replace-current need editor-level caret control.

This is now partially solved.

Plan:

- use `EditorViewState.cursor_range` as the persisted editor selection snapshot
- use `EditorViewState.pending_cursor_range` for programmatic match navigation and replace-current repositioning
- keep all raw `egui::TextEditState` interaction inside `src/app/ui/editor_content/text_edit.rs`

This should be wrapped behind helper functions inside `text_edit.rs` so the rest of the app does not depend on `egui` text-state details directly.

### Match highlighting

The current custom layouter is the best place to implement highlights.

Plan:

- extend the layouter input so it can receive `EditorViewState.search_highlights` when the rendered buffer matches the active search target
- build a `LayoutJob` with:
  - normal text formatting for non-match regions
  - passive highlight formatting for all matches
  - active highlight formatting for the selected match

This keeps the highlight path in one place:

- no overlay painting
- no manual text measurement outside the layouter
- no duplication between wrapped and unwrapped modes

### Scroll-to-match

Do not try to scroll by geometry math in the first pass.

Instead:

- when navigating to a match, set `pending_cursor_range` on the active view to that match range
- let `egui::TextEdit` keep the caret visible

If this proves insufficient, add a follow-up helper that uses `RenderedLayout` to compute the row position and explicitly scroll the surrounding `ScrollArea`.

## 7. Undo / Replace Model

This section changes materially because the editor now has document-level undo history.

### 7.1 What is now possible

- normal text editing already supports `Ctrl + Z`, `Ctrl + Y`, and `Ctrl + Shift + Z`
- undo depth is explicitly capped at 100 states per `TextDocument`
- search replacement can now build on the same document-level undo history instead of bypassing it

### 7.2 What is still not solved

- there is still no app-wide transaction layer spanning multiple `TextDocument` instances
- an `AllOpenTabs` replace-all cannot honestly promise one global undo step in the first pass

### 7.3 Replacement policy

- replace current: must preserve document undo history naturally
- replace all in one document: should be implemented as one coherent document mutation boundary where practical
- replace all across multiple documents: should preserve per-document undo, but not promise cross-document atomic undo

This keeps the implementation honest relative to the original goals while still benefiting from the new document model.

## 8. Command and Shortcut Plan

### New commands

Extend `AppCommand` with search-oriented actions such as:

- `OpenSearch`
- `OpenSearchAndReplace`
- `CloseSearch`
- `NextSearchMatch`
- `PreviousSearchMatch`
- `ReplaceCurrentMatch`
- `ReplaceAllMatches`

Do not put raw text-edit keystrokes into `AppCommand`. Query changes should remain direct state mutation from the search strip UI.

### Shortcut wiring

Add to `src/app/shortcuts.rs`:

- `Ctrl + F` -> open search
- `Ctrl + H` -> open search with replace expanded
- `Enter` while the search field is focused -> next match
- `Shift + Enter` while the search field is focused -> previous match
- `Esc` -> close search if open, otherwise preserve current behavior

Shortcut handling needs one extra rule:

- when search is open and one of its text fields has focus, search-local shortcuts should win before global tab/file shortcuts

Because editor undo/redo already lives in the focused `TextEdit`, search-local shortcuts must not consume `Ctrl + Z`, `Ctrl + Y`, or `Ctrl + Shift + Z` unless a future search widget intentionally needs them.

## 9. Replacement Semantics

### Replace current

Algorithm:

1. validate that the active match still exists for the current query
2. mutate only the target buffer's `TextDocument`
3. refresh buffer metadata (`line_count`, artifact summary)
4. mark the owning workspace tab dirty
5. recompute results immediately
6. move active selection to the next sensible match

### Replace all

For one buffer, replace in descending range order.

For multi-buffer scopes:

1. group matches by `BufferId`
2. for each buffer, apply descending-order replacements
3. refresh metadata once per mutated buffer
4. mark the owning tab(s) dirty
5. emit one summary status message

This keeps indices stable without introducing a transaction abstraction that the rest of the editor does not yet use.

### Dirty-state integration

After replacement, reuse the same post-edit behaviors already used by editor typing where possible:

- dirty flag updates
- artifact metadata refresh
- status message refresh
- settings TOML refresh checks when the edited buffer is the settings file

This is important because search replace must not become a second, slightly different edit path.

## 10. File-Level Change Plan

Expected first-pass file additions:

- `src/app/services/search.rs`
- `src/app/ui/search_replace.rs`

Expected first-pass file edits:

- `src/app/app_state.rs`
- `src/app/commands.rs`
- `src/app/commands/dispatch.rs`
- `src/app/domain/buffer.rs`
- `src/app/shortcuts.rs`
- `src/app/domain/view.rs`
- `src/app/ui/editor_area/mod.rs`
- `src/app/ui/editor_content/text_edit.rs`
- `src/app/ui/editor_content/mod.rs`
- `src/app/ui/tab_strip/actions.rs`
- `src/app/ui/status_bar.rs`

Optional supporting edits:

- `src/app/ui/mod.rs`
- `src/app/services/mod.rs`
- `README.md`
- `PLAN.md`

Foundation edits already completed before search implementation:

- `src/app/domain/buffer.rs`
- `src/app/domain/view.rs`
- `src/app/ui/editor_content/text_edit.rs`
- affected test files and supporting buffer call sites

## 11. Delivery Phases

### Phase 0: Foundation Completed

Already done:

- document-backed text storage via `TextDocument`
- document-level undo state
- persisted view cursor state
- pending cursor targeting
- per-view search highlight storage

This phase is complete and should be treated as the prerequisite layer for the rest of the plan.

### Phase 1: Search state and strip skeleton

- add `SearchState` to `ScratchpadApp`
- add open/close commands and shortcuts
- render a non-functional search strip in `editor_area`
- wire the toolbar search button to open the strip instead of showing a warning

Definition of done:

- `Ctrl + F` opens the strip
- `Ctrl + H` opens the strip with replace visible
- `Esc` closes the strip and restores editor focus

### Phase 2: Active-buffer search

- implement plain-text match engine for one buffer
- live-update results while typing
- show total count and active index
- navigate next/previous
- highlight passive and active matches in the active buffer

Definition of done:

- searching in the active buffer works reliably in wrapped and unwrapped modes
- navigation keeps the active match visible

### Phase 3: Replacement in active buffer

- implement replace current
- implement replace all for active buffer
- reuse existing dirty-state and status-update flows

Definition of done:

- replacing content mutates the underlying buffer correctly
- line counts and artifact status stay accurate
- editor undo / redo still works after replacement

### Phase 4: Cross-buffer scopes

- add active-workspace-tab scope
- add all-open-tabs scope
- deduplicate buffers correctly
- show replacement summary in the status bar

Definition of done:

- replace-all can affect multiple buffers without duplicate edits
- split views sharing one buffer do not produce duplicate replacements

### Phase 5: Polish and spec catch-up

- optional toolbar-button anchoring/popup placement
- optional case toggle UI polish
- optional preserved recent queries
- decide whether `Find Tab` should be a separate feature or folded into a command palette later

## 12. Test Plan

### Service tests

Add focused tests for the search engine covering:

- empty query
- no matches
- multiple matches
- case-sensitive vs case-insensitive search
- Unicode and multibyte characters
- descending-order replace-all correctness
- mixed-scope deduplication by `BufferId`

Also add tests for document-aware replacement boundaries:

- replace current preserves undo / redo in a single document
- replace all in one document preserves undo / redo behavior predictably

### App / command tests

Add tests covering:

- `Ctrl + F` / `Ctrl + H` open state
- `Esc` close behavior
- next/previous match selection
- replace-current on active buffer
- replace-all across split buffers in one workspace tab
- replace-all across multiple tabs
- editing the settings file through replace still triggers settings refresh flow correctly
- cursor range is restored when navigating to a match

### UI behavior tests

At minimum, unit-test the non-render logic for:

- scope resolution
- active-match selection after result recompute
- summary status text generation

## 13. Risks and Mitigations

### Risk: `egui::TextEdit` state APIs are easy to misuse

Mitigation:

- isolate caret/selection read-write logic inside `text_edit.rs`
- keep the rest of the app on simple app-owned structs

### Risk: highlight rendering becomes slow on very large files

Mitigation:

- keep first pass plain-text only
- short-circuit highlighting when the query is empty
- measure large-file behavior before adding regex or project-wide search

### Risk: search and normal editing diverge into separate mutation paths

Mitigation:

- route replacement through shared post-edit helpers
- keep buffer metadata refresh in one place

### Risk: replacement clears or corrupts document undo history

Mitigation:

- treat `TextDocument` as the write boundary for search replacement
- test replace-current and replace-all against undo / redo explicitly
- avoid replacement helpers that bypass the existing editor/document undo path unless they intentionally create one new undo boundary

### Risk: the original plan's feature list keeps expanding mid-implementation

Mitigation:

- treat Track A as the ship target
- keep `Find Tab`, regex, and provider abstractions out of the first merge

## 14. Recommended Merge Strategy

Use small reviewable pull requests rather than one large feature branch:

1. search state + commands + shortcuts + strip shell
2. active-buffer search + highlight rendering
3. active-buffer replace with undo-preservation tests
4. cross-buffer scopes
5. polish and follow-up UX

That keeps regressions localized in a codebase that is already carrying custom chrome, tab drag/drop, split panes, settings refresh, and session persistence.