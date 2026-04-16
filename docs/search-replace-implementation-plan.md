# Search / Replace Implementation Plan For Scratchpad

This plan replaces the broader draft.
here is another modification
It is scoped to one implementation run and to the code that already exists today. The goal is to ship a solid active-buffer search and replace feature without promising cross-buffer behavior that the current editor and undo model do not support yet.

## 1. Ship Target

Ship a compact search / replace strip that works on the active buffer only.

First-run feature set:

- search strip near search button
- whole-word matching
- plain-text search in the active buffer, active workspace tab scope & all-open-tabs scope
- `Ctrl + F` opens search (with replace visible)
- next / previous match navigation
- live match count with active match index
- case-sensitive toggle
- replace current match
- replace all matches in the active buffer
- `Esc` closes the strip and returns focus to the editor
- toolbar search buttons open the strip instead of showing the current placeholder warning

## 2. Explicitly Deferred

These are out of scope for this run:

- search on disk or unopened files
- regex
- query history
- floating or anchored popup geometry
- `Find Tab`
- global multi-document undo for replace-all

That narrower scope is deliberate. The current codebase already has enough editor state and selection plumbing to ship active-buffer search cleanly, but not enough proven undo-safe mutation plumbing to justify cross-buffer replace in the same pass.

## 3. Current Codebase Reality

The revised plan should be built around the code that already exists:

- `src/app/ui/tab_strip/actions.rs` still routes search buttons to `app.set_warning_status("Search is not implemented yet.")`
- `EditorViewState` already stores:
  - `cursor_range`
  - `pending_cursor_range`
  - `search_highlights`
- `sync_text_edit_state_before_render` in `src/app/ui/editor_content/text_edit.rs` already applies `pending_cursor_range` back into `egui::TextEditState`
- the text layouter in `src/app/ui/editor_content/text_edit.rs` currently renders one text format only, so search highlighting is not implemented yet
- `apply_editor_change` in `src/app/ui/editor_area/mod.rs` is the current active-buffer post-edit finalization path
- that finalization path already covers:
  - metadata refresh
  - dirty-state updates
  - artifact warning/status updates
  - transaction logging
  - session dirty tracking
  - `note_settings_toml_edit`
- `TextDocument::replace_text` and `BufferState::replace_text` clear the document undoer, which makes them invalid write paths for search replacement
- per-document undo exists, but there is no existing non-widget replacement helper that already proves undo preservation for search-driven edits

Those constraints should drive the implementation. The plan should not assume a future abstraction layer or a future global undo model.

## 4. Design Rules

1. Search is an editor feature, not a tab-strip feature.
2. Ccope picker is needed.
3. All search ranges use character offsets. Search should also be aware that it may be searching the same text represneted in different encoding. (it should be able to find the text no matter which encoding is used)
4. Replacement must use an undo-aware range-edit path, never whole-buffer replacement helpers.
5. Replacement must reuse the same active-buffer finalization path as ordinary typing.
6. The implementation should stay concrete and local to the current editor architecture instead of adding provider traits or future-facing abstractions.

## 5. Proposed Shape

### 5.1 App-owned search state

Add search state directly to `ScratchpadApp`.

Recommended shape:

```rust
pub(crate) struct SearchState {
    pub open: bool,
    pub replace_open: bool,
    pub query: String,
    pub replacement: String,
    pub match_case: bool,
    pub active_match_index: Option<usize>,
    pub matches: Vec<SearchMatch>,
    pub buffer_id: Option<BufferId>,
    pub focus_target: SearchFocusTarget,
}

pub(crate) struct SearchMatch {
    pub range: std::ops::Range<usize>,
}

pub(crate) enum SearchFocusTarget {
    FindInput,
    ReplaceInput,
    Editor,
}
```

Notes:

- `buffer_id` is enough to invalidate stale results when the user switches tabs or views
- no scope enum is needed in this run
- no provider layer is needed in this run

### 5.2 Search service

Add `src/app/services/search.rs` as a small, read-only helper module.

Responsibilities:

- `find_matches(text, query, match_case) -> Vec<SearchMatch>`
- `next_match_index(matches, current) -> Option<usize>`
- `previous_match_index(matches, current) -> Option<usize>`

This module should not mutate buffers. It only computes matches and navigation results.

### 5.3 Search UI

Add `src/app/ui/search_replace.rs` and render it from `src/app/ui/editor_area/mod.rs` above the pane tree.

The strip should contain:

- find input
- replace input when replace mode is open
- previous button
- next button
- match count label
- case-sensitive toggle
- replace current button
- replace all button
- close button

That placement fits the current UI structure:

- it works for top tabs and side tabs
- it stays attached to the active editing surface
- it avoids unrelated anchored-popup work

## 6. Editor Integration

### 6.1 Match computation

Recompute matches whenever any of these change:

- search query
- case-sensitive toggle
- active buffer
- active-buffer text after a replacement

Rules:

- empty query clears matches and highlights
- if the old active match still exists at the same range after recompute, keep it
- otherwise choose the first match when matches exist

### 6.2 Navigation

Use the existing `pending_cursor_range` path.

When the active match changes:

- set the active view's `pending_cursor_range`
- update `search_highlights.active_range_index`
- let `TextEdit` restore the selection on the next render

No custom scrolling helper is required for first ship.

### 6.3 Highlight rendering

Extend the layouter path in `src/app/ui/editor_content/text_edit.rs` so it can build a `LayoutJob` with:

- normal text
- passive search match styling
- active search match styling

Do not use overlay painting for first ship. The current text-edit bridge is already the right seam for highlight rendering because it owns the layouter and captures the latest rendered layout there.

## 7. Replacement Path

This is the main place where the previous plan was too optimistic. The current code does not yet have a proven non-widget replacement path that preserves document undo.

### 7.1 Required extraction

Extract the active-buffer finalization logic out of `apply_editor_change` into a reusable helper on `ScratchpadApp`.

Target shape:

- one helper that finalizes the active buffer after a text mutation
- normal typing calls it
- search replacement calls it

That helper should continue to cover:

- `refresh_text_metadata`
- dirty-state updates
- artifact warning/status updates
- transaction logging
- session dirty tracking
- `note_settings_toml_edit`

This is enough for first ship. A multi-buffer finalization abstraction is not needed until cross-buffer replace is actually in scope.

### 7.2 Required write helper

Add an undo-aware active-buffer replacement helper in `src/app/domain/buffer.rs` or the closest active-buffer edit seam.

Requirements:

- operate on character ranges
- replace one range or many ranges in descending order
- never call `replace_text`
- explicitly preserve or rebuild the document undo state so `Ctrl + Z` still works after replacement
- update the resulting selection so the next render can target the correct match

Important constraint:

- do not assume direct `insert_text` and `delete_char_range` calls are already enough for undo preservation

The implementation order should be:

1. add the helper
2. test undo behavior directly
3. only then wire replace-current and replace-all

### 7.3 Replacement semantics

`Replace Current`:

1. validate that the active match index is still valid
2. replace that one range
3. finalize the active buffer through the shared helper
4. recompute matches
5. move to the next sensible match

`Replace All`:

1. collect current matches for the active buffer
2. apply replacements in descending range order
3. finalize the active buffer through the shared helper
4. recompute matches
5. show a short status summary

## 8. Commands And Shortcuts

Extend `AppCommand` with search commands such as:

- `OpenSearch`
- `OpenSearchAndReplace`
- `CloseSearch`
- `NextSearchMatch`
- `PreviousSearchMatch`
- `ReplaceCurrentMatch`
- `ReplaceAllMatches`

Shortcut plan:

- `Ctrl + F` opens search and focuses the find input
- `Ctrl + H` opens replace and focuses the find input
- `Enter` in the find input selects the next match
- `Shift + Enter` in the find input selects the previous match
- `Esc` closes the strip and returns focus to the editor

Important current constraint:

- global shortcut handling is mostly focus-agnostic
- `Enter`, `Shift + Enter`, and `Esc` should be handled by the search-strip widgets locally instead of assuming the global shortcut layer already models input focus well enough

## 9. File Plan

New files:

- `src/app/services/search.rs`
- `src/app/ui/search_replace.rs`

Expected edits:

- `src/app/app_state.rs`
- `src/app/commands.rs`
- `src/app/commands/dispatch.rs`
- `src/app/shortcuts.rs`
- `src/app/domain/buffer.rs`
- `src/app/domain/view.rs`
- `src/app/ui/editor_area/mod.rs`
- `src/app/ui/editor_content/text_edit.rs`
- `src/app/ui/tab_strip/actions.rs`
- `src/app/ui/mod.rs`
- `src/app/services/mod.rs`

Likely docs to update after implementation:

- `README.md`
- `PLAN.md`
- `docs/user-manual.md`

## 10. Single-Run Implementation Order

This is the order that fits one development pass without inventing extra architecture:

1. add `SearchState`, commands, and shortcut entry points
2. add the search strip UI and wire the toolbar search buttons to open it
3. add the plain-text search service and active-buffer recompute logic
4. wire active-match navigation through `pending_cursor_range`
5. extend the text layouter to render passive and active highlights
6. extract the active-buffer post-edit finalization helper from `apply_editor_change`
7. add the undo-aware active-buffer replacement helper and test it in isolation
8. wire `Replace Current` and `Replace All` for the active buffer
9. add status text, focus polish, and docs

If step 7 cannot preserve expected document undo behavior cleanly, stop there and ship search-only first. That is a better result than landing replace behavior that silently regresses undo.

## 11. Test Plan

### Service tests

Add tests for:

- empty query
- no matches
- repeated matches
- case-sensitive vs case-insensitive behavior
- Unicode text
- next / previous wrap behavior

### Buffer/edit tests

Add unit tests for the new replacement helper:

- replace one range
- replace many ranges in descending order
- overlapping ranges are rejected or never produced
- undo restores the previous text
- resulting selection/cursor state is predictable after replacement

### App tests

Add integration-style tests for:

- `Ctrl + F` opens the strip
- `Ctrl + H` opens replace mode
- `Esc` closes the strip
- next / previous navigation updates the active selection
- replace current changes only the active buffer
- replace all updates all matches in the active buffer
- replacing inside `settings.toml` still triggers the existing settings-refresh follow-up

### UI logic tests

Add focused tests for:

- active-match selection after recompute
- search state reset when the active buffer changes
- highlight state clearing when the query becomes empty

## 12. Definition Of Done

This plan is complete when all of the following are true:

- search and replace are available from `Ctrl + F`, `Ctrl + H`, and the toolbar search button
- the strip works on the active buffer only, and does that reliably
- highlights and match navigation stay synchronized with the editor selection
- replace operations do not use undo-clearing whole-buffer helpers
- replace operations preserve expected document undo behavior
- normal typing and search replacement share the same active-buffer finalization path
- replacements inside `settings.toml` still participate in the existing settings-refresh flow
- the user manual and plan docs reflect the shipped scope

## 13. Deferred Follow-Up

After the active-buffer feature ships and proves stable, the next logical extensions are:

- active workspace tab scope
- all-open-tabs scope
- better replace-all summaries
- regex and whole-word options
- explicit scroll-to-match polish if the default `TextEdit` behavior is not enough

That order matters. Cross-buffer scope should come after the active-buffer replacement path is proven safe for undo, selection sync, and settings refresh.
