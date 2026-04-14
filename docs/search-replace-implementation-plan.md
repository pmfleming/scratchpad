# Search / Replace Implementation Plan For Scratchpad

This plan replaces the earlier draft and is written against the project as it exists today.

It is intentionally implementation-oriented. The goal is to give us a realistic path to shipping search and replace in the current codebase without promising behavior the editor cannot honestly support yet.

## 1. Current Project Reality

Scratchpad now has a much stronger editing foundation than it did when the original search/replace planning started.

Important current facts:

- `BufferState` owns a `TextDocument`.
- `TextDocument` is the editable text source used by `egui::TextEdit`.
- each `TextDocument` owns its own undo history
- undo/redo is per document, not global
- `EditorViewState` already stores:
  - `cursor_range`
  - `pending_cursor_range`
  - `search_highlights`
- the editor render bridge in `src/app/ui/editor_content/text_edit.rs` already synchronizes:
  - `TextDocument`
  - `egui::TextEditState`
  - selection/caret state
  - per-document undo state
- workspace tabs may contain:
  - one active buffer
  - additional buffers in the same workspace tab
  - multiple split views pointing at those buffers
- the app also now has a separate workspace transaction log, but normal text undo remains document-local

These facts change the plan substantially:

- active-buffer search is now straightforward
- match navigation can drive selection through `pending_cursor_range`
- replace flows must respect per-document undo instead of inventing a fake global undo model
- cross-buffer replace-all must be treated as a multi-document operation, not one atomic undo transaction

## 2. Product Scope We Should Ship

### First ship target

Ship a compact in-editor search / replace strip that supports:

- `Ctrl + F` to open search
- `Ctrl + H` to open search with replace visible
- plain-text search in the active buffer
- next / previous match navigation
- match count and active match index
- replace current
- replace all in:
  - active buffer
  - active workspace tab
  - all open tabs
- case-sensitive toggle
- non-modal workflow so the editor remains usable while search is open

### Explicitly out of first pass

- regex
- search on disk
- unopened-file/project search
- `Find Tab`
- multi-cursor promotion
- query history
- toolbar-anchored popup geometry
- global one-step undo for multi-document replace-all

## 3. Guiding Rules

### Rule 1: Search is an editor feature, not a tab-strip feature

The toolbar search button can open it, but the UI belongs in the editor area.

### Rule 2: Replacement must use document-aware edit paths

Search/replace cannot be allowed to mutate text through helpers that clear undo history.

### Rule 3: Post-edit effects must be shared with normal typing

Replacement must trigger the same metadata, dirty-state, warning, session, and settings-refresh behavior as ordinary editor edits.

### Rule 4: Multi-document replace-all is not atomic undo

We should preserve per-document undo honestly, not promise cross-document atomic undo we do not have.

## 4. Proposed Architecture

### 4.1 App-owned search state

Add app-owned search state to `ScratchpadApp`.

Recommended structure:

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
    pub focus_target: SearchFocusTarget,
    pub source: SearchSourceContext,
}

pub(crate) enum SearchScope {
    ActiveBuffer,
    ActiveWorkspaceTab,
    AllOpenTabs,
}
```

`SearchSourceContext` should remember where search was opened from so closing the strip can restore focus predictably.

`SearchFocusTarget` should explicitly model focus movement between:

- find input
- replace input
- editor

### 4.2 Search service module

Add a plain search engine in:

- `src/app/services/search.rs`

Responsibilities:

- resolve search scope into concrete targets
- compute plain-text matches
- support case-sensitive and case-insensitive search
- operate in character offsets
- group matches by `BufferId`
- provide replace helpers for:
  - replace current
  - replace all in one buffer
  - replace all across buffers

Do not introduce provider traits or regex abstraction in the first pass.

### 4.3 Search target model

Use an internal target model so UI code does not search by poking directly through widgets:

```rust
struct SearchTarget {
    tab_index: usize,
    buffer_id: BufferId,
    view_ids: Vec<ViewId>,
    text: String,
}
```

Notes:

- `ActiveBuffer` resolves the active view's current buffer
- `ActiveWorkspaceTab` resolves unique buffers inside the active workspace tab
- `AllOpenTabs` resolves unique buffers across all tabs
- deduplication must be by `BufferId`

## 5. UI Placement

Add:

- `src/app/ui/search_replace.rs`

Render the strip from:

- `src/app/ui/editor_area/mod.rs`

The strip should render above the pane tree inside the editor workspace.

Why this is still the right first placement:

- it works for top tabs and side tabs
- it avoids brittle floating/anchored geometry
- it keeps the feature attached to the active editing surface
- it avoids coupling search behavior to tab chrome

The search button in:

- `src/app/ui/tab_strip/actions.rs`

should dispatch open-search instead of showing the current placeholder warning.

## 6. Match Representation

The current code already has a place to store highlights:

- `EditorViewState.search_highlights`

But the project should now define the unit clearly.

Required decision:

- all search results and highlight ranges in app/service code should use character offsets

That means:

- `SearchMatch` should store character-based ranges
- replacement should operate from character-based ranges
- any conversion needed for layout or rendering must happen at the edge

Recommended shape:

```rust
pub(crate) struct SearchMatch {
    pub tab_index: usize,
    pub buffer_id: BufferId,
    pub range: std::ops::Range<usize>, // character offsets
}
```

This needs to be explicit because the current codebase contains both string operations and layout/rendering code, and Unicode safety will otherwise be easy to break.

## 7. Editor Integration

### 7.1 Selection and navigation

Use the existing editor-view state:

- `cursor_range` for persisted current selection snapshot
- `pending_cursor_range` for programmatic match targeting

Plan:

- when search results update, compute the active match
- when navigating next/previous, set `pending_cursor_range` on the relevant active view
- let `egui::TextEdit` bring the caret into view naturally

Keep all raw `TextEditState` interaction inside:

- `src/app/ui/editor_content/text_edit.rs`

### 7.2 Highlight rendering

The current layouter in:

- `src/app/ui/editor_content/text_edit.rs`

is the right place to render match highlights.

Plan:

- extend the layouter path to accept current search highlights for the rendered view/buffer
- build a `LayoutJob` with:
  - normal text format
  - passive match format
  - active match format

We should not use overlay painting for text highlights in the first pass.

### 7.3 Scroll-to-match

Do not start with custom geometry math.

First pass:

- set `pending_cursor_range`
- rely on `TextEdit` visibility behavior

If needed later, add explicit scroll helpers using `RenderedLayout`.

## 8. Undo Model

### 8.1 What we can honestly support

- replace current in one document should preserve normal document undo/redo
- replace all in one document should ideally behave as one coherent undo boundary
- replace all across multiple documents should preserve per-document undo only

### 8.2 What must not happen

The current whole-document helper:

- `TextDocument::replace_text`

clears undo history. That makes it unsafe for undo-preserving search replacement.

So the implementation plan must require:

- do not use whole-document replacement helpers that reset the undoer for replace-current
- do not use them for single-document replace-all if we want undo preservation

Before implementation starts, we should introduce explicit document mutation helpers for search replacement that preserve or intentionally set the desired undo boundary.

### 8.3 Practical policy

- replace current:
  - mutate through document-aware edit operations
  - preserve undo naturally
- replace all in one buffer:
  - mutate through one coherent document-aware path
  - aim for one undo step
- replace all across buffers:
  - mutate each buffer through that same document-aware path
  - do not promise one global undo step

## 9. Shared Post-Edit Finalization

This is the biggest implementation requirement the old draft did not pin down clearly enough.

Today, normal typing finalization is effectively handled from:

- `src/app/ui/editor_area/mod.rs`

That logic currently updates:

- dirty state
- artifact metadata
- warning/status state
- transaction logging
- session dirty state
- settings TOML refresh checks

Search/replace must not invent a second version of that logic.

So before or during replacement work, extract a shared helper on `ScratchpadApp` for:

- finalizing a buffer mutation
- finalizing one or more mutated buffers

That shared path should accept enough context to handle:

- active-buffer edits
- non-active buffer edits
- multi-buffer replace-all
- settings file edits
- transaction logging for replacements

Without this extraction, the plan will drift into inconsistent edit semantics.

## 10. Commands and Shortcuts

### New commands

Extend `AppCommand` with search commands such as:

- `OpenSearch`
- `OpenSearchAndReplace`
- `CloseSearch`
- `NextSearchMatch`
- `PreviousSearchMatch`
- `ReplaceCurrentMatch`
- `ReplaceAllMatches`

Do not route raw query typing through `AppCommand`.

### Shortcut plan

Add to:

- `src/app/shortcuts.rs`

First-pass bindings:

- `Ctrl + F` opens search
- `Ctrl + H` opens search with replace visible
- `Esc` closes search when search is open

Navigation bindings:

- `Enter` in find input moves to next match
- `Shift + Enter` in find input moves to previous match

Important current constraint:

`src/app/shortcuts.rs` is currently global and focus-agnostic.

So the implementation needs one explicit choice:

- either search input widgets own `Enter`/`Shift+Enter`/`Esc` locally
- or we add a small focus-aware shortcut layer for search

Do not start implementation assuming current global shortcut handling is already enough.

## 11. Replacement Semantics

### Replace current

Algorithm:

1. validate the active match still exists for the current query
2. mutate only the target document
3. finalize that buffer through shared post-edit logic
4. recompute results
5. select the next sensible match

### Replace all

For one buffer:

- apply replacements in descending range order

For multi-buffer scopes:

1. group matches by `BufferId`
2. apply descending-order replacements per buffer
3. finalize each mutated buffer through the shared post-edit path
4. recompute results
5. show one summary status message

Descending order remains the right first-pass approach because it keeps match indices stable without introducing more complex transactional editing machinery.

## 12. File Change Plan

### New files

- `src/app/services/search.rs`
- `src/app/ui/search_replace.rs`

### Expected edits

- `src/app/app_state.rs`
- `src/app/commands.rs`
- `src/app/commands/dispatch.rs`
- `src/app/shortcuts.rs`
- `src/app/domain/view.rs`
- `src/app/ui/editor_area/mod.rs`
- `src/app/ui/editor_content/text_edit.rs`
- `src/app/ui/editor_content/mod.rs`
- `src/app/ui/tab_strip/actions.rs`
- `src/app/ui/mod.rs`
- `src/app/services/mod.rs`

### Likely additional edits

- `src/app/domain/buffer.rs`
  - only if needed for explicit document-aware replacement helpers
- transaction/status integration files if replacement should be logged distinctly
- docs:
  - `README.md`
  - `PLAN.md`
  - `docs/user-manual.md`

## 13. Delivery Phases

### Phase 1: Search state and strip shell

- add `SearchState` to `ScratchpadApp`
- add open/close commands
- add `Ctrl + F` / `Ctrl + H`
- render strip shell in editor area
- wire toolbar search button

Definition of done:

- search strip opens and closes reliably
- focus returns to editor on close

### Phase 2: Active-buffer search

- implement plain-text matching for the active buffer
- live-update results while typing
- show count and active index
- next/previous navigation
- render highlights

Definition of done:

- search works in wrapped and unwrapped modes
- active match selection stays stable

### Phase 3: Active-buffer replace

- implement replace current
- implement replace all for active buffer
- route mutation through undo-safe document helpers
- route finalization through shared post-edit helper

Definition of done:

- replace operations preserve expected document undo/redo behavior
- dirty state and metadata remain correct

### Phase 4: Cross-buffer scopes

- add active-workspace-tab scope
- add all-open-tabs scope
- deduplicate by `BufferId`
- show replacement summary

Definition of done:

- split views of the same buffer do not double-apply edits
- multi-buffer replace-all works predictably

### Phase 5: Polish

- UI polish
- case toggle polish
- performance review on larger files
- decide later whether `Find Tab` stays separate or folds into another feature

## 14. Test Plan

### Search service tests

Add tests for:

- empty query
- no matches
- multiple matches
- case-sensitive and case-insensitive behavior
- Unicode text
- descending replace-all correctness
- deduplication by `BufferId`

### Undo-aware replacement tests

Add explicit tests for:

- replace current preserves single-document undo/redo
- replace all in one document preserves predictable undo/redo behavior
- multi-document replace-all preserves per-document undo

### App/command tests

Add tests for:

- `Ctrl + F` / `Ctrl + H` open behavior
- close behavior
- next/previous navigation
- replace current on active buffer
- replace all in active workspace tab
- replace all across tabs
- settings file replacement still triggers settings refresh flow correctly

### UI logic tests

At minimum, unit-test:

- scope resolution
- active-match recompute rules
- summary status text
- focus-target transitions

## 15. Risks

### Risk: undo gets broken by the wrong mutation helper

Mitigation:

- make the allowed replacement write path explicit before Phase 3 starts
- test undo/redo behavior directly

### Risk: search and normal typing diverge

Mitigation:

- extract shared post-edit finalization
- do not duplicate dirty/settings/status logic in search code

### Risk: Unicode range bugs

Mitigation:

- define character-offset ranges as the app/service contract
- confine conversions to the rendering edge

### Risk: focus/shortcut handling becomes brittle

Mitigation:

- explicitly choose widget-local or focus-aware shortcut routing
- do not assume the current global shortcut layer is sufficient

### Risk: highlight rendering becomes expensive on large files

Mitigation:

- keep the first pass plain-text only
- skip highlight work for empty query
- measure before adding regex or project search

## 16. Recommended Merge Strategy

Use small reviewable PRs:

1. search state + commands + strip shell
2. active-buffer search + highlight rendering
3. active-buffer replace + undo-safe mutation path + shared post-edit finalization
4. cross-buffer scopes
5. polish

That is the right granularity for the project as it exists today: custom chrome, split panes, combined workspace tabs, settings-file refresh behavior, per-document undo, and transaction-log integration all make a single giant feature branch riskier than it looks.
