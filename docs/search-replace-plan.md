# Unified Search/Replace Plan

This plan replaces the earlier placeholder search document with an implementation plan that matches Scratchpad's current architecture: multi-pane workspaces, view-local UI state, and editor interactions built on `egui`.

The goal is a responsive in-editor search experience with:

- live search as the query changes
- one unified search/replace surface
- replace current and replace all
- multi-cursor search/replace for repeated next/previous matches
- search scopes for current context, current tab, and all open tabs
- minimal duplicated state and minimal duplicated search logic

## 1. Product Goals

- Add a fast live-search flow inside one shared search/replace view.
- Keep search non-modal and anchored to the active editor experience.
- Support single-match replacement and whole-document replacement.
- Support "select next match" style multi-cursor editing for repeated search hits.
- Support scope switching between current context, current tab, and all open tabs.
- Share one search engine and one match model across find, replace, highlight, navigation, and multi-cursor expansion.
- Fit the current multi-pane model without introducing disconnected search UIs per scope.

## 2. Scope

In scope for this pass:

- `Ctrl + F` opens search for the active view.
- `Ctrl + H` opens replace for the active view.
- Live highlight updates while typing.
- One combined search/replace strip rather than separate find and replace surfaces.
- Next/previous match navigation.
- Match count and active match index.
- Replace current.
- Replace all in the active search scope.
- Add next match to selection.
- Add previous match to selection.
- Replace across all active multi-cursor selections derived from search.
- Scope switching for:
  - current context
  - current tab
  - all open tabs

Out of scope for this pass:

- project-wide search
- regex search
- search across unopened files
- preserve-all-historical-searches UX
- structural search
- replace preview diff UI

## 3. UX Direction

### Search Surface

Use a compact unified search/replace strip anchored at the top of the active editor area, not a modal dialog.

- It should appear inside the active tile/editor region.
- It should not displace the tab strip or status bar.
- It should keep keyboard focus in the search field when opened.

### Unified Search/Replace View

Search and replace should always be part of the same surface.

- The primary query field is always visible.
- The replace field is part of the same strip, not a separate mode or second popup.
- The UI can collapse or de-emphasize replace controls when not in use, but it should remain one component with one state object.
- Scope selection should sit in this same strip so query, replace text, navigation, and scope are managed together.

### Live Search Behavior

- Highlights update on every query change.
- The nearest current match becomes active when practical.
- Empty query clears highlights and match navigation state.
- No-match state should be visually obvious but not disruptive.

### Search Scope Behavior

Support three scopes from the same UI:

- `Current Context`
- `Current Tab`
- `All Open Tabs`

Recommended semantics:

- `Current Context`: search only within the active editor context. For the first implementation this should mean the active view's current buffer and current editing context, with navigation centered around the current primary cursor/selection.
- `Current Tab`: search across the active workspace tab. If the workspace tab can contain multiple open buffers/views, this scope should include those buffers, not just the currently focused tile.
- `All Open Tabs`: search across every currently open workspace tab and their visible/open buffers.

The exact boundaries should be defined once in code and then reused consistently for match counts, navigation, and replacement actions.

### Multi-Cursor Search Behavior

The search flow should support repeated match selection without inventing a second workflow:

- `Find Next` moves the active match.
- `Add Next Match` adds the next search hit as another selection/cursor.
- `Add Previous Match` does the same in reverse.
- `Replace` acts on the active match when only one match is active.
- `Replace All Selected` acts on all search-driven cursors when multi-cursor mode is active.

This should behave like "search-powered multi-cursor", not like a completely separate feature.

## 4. Architecture Direction

### Search Session Ownership

The UI state for the search strip should be shared enough to feel like one tool, but the match context still needs to respect Scratchpad's multi-pane model.

Recommended split:

- one active `SearchSession` owned at the app/workspace level for the visible search/replace strip
- scope resolution delegated through the active workspace/view context
- per-view rendering helpers consume the active search session when that view participates in the current scope

Why:

- the user asked for one unified search/replace view
- the chosen scope may span more than one tile or tab
- a purely view-local search state no longer matches the desired UX
- we still avoid duplicating search logic per tile by using one canonical session plus scope-aware match resolution

Recommended shape:

```rust
pub struct SearchSession {
    pub is_open: bool,
    pub query: String,
    pub replacement: String,
    pub scope: SearchScope,
    pub options: SearchOptions,
    pub matches: Vec<SearchMatch>,
    pub active_match_index: Option<usize>,
    pub selection_mode: SearchSelectionMode,
}

pub enum SearchScope {
    CurrentContext,
    CurrentTab,
    AllOpenTabs,
}

pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

pub struct SearchMatch {
    pub target: SearchTarget,
    pub char_range: std::ops::Range<usize>,
}

pub enum SearchTarget {
    ActiveContext,
    Buffer(BufferId),
}

pub enum SearchSelectionMode {
    None,
    ActiveMatch,
    MultiCursor(Vec<usize>),
}
```

Exact types can vary, but the important part is:

- one state object
- one scope field
- one match list
- one active match index
- one multi-selection source of truth

### Shared Search Engine

Do not split search logic between:

- highlight generation
- next/previous navigation
- replace current
- replace all
- multi-cursor expansion

Instead, add one reusable search module that:

1. scans editor text
2. returns match ranges in one canonical coordinate system
3. tags each match with its target buffer/context
3. supports "next from current position"
4. supports "previous from current position"

Recommended module boundary:

- `src/app/services/search.rs` or similar for pure matching and replacement planning
- scope resolvers provide the set of searchable targets
- editor/view modules consume the resolved results

### Canonical Coordinates

Use one coordinate system consistently for search results and cursor operations.

Preferred:

- character indices if current editor selection/editing logic is character-based

Avoid mixing:

- bytes for search
- chars for cursor movement
- egui row offsets for selection

If the text editor internals already use a different coordinate system, the plan should standardize conversions in one place only.

## 5. Live Search Design

### Match Lifecycle

Whenever any of these change:

- query
- scope
- case sensitivity
- whole-word mode
- text in any searchable target inside the active scope

recompute matches for the active view.

Rules:

- If query is empty: `matches = []`, `active_match_index = None`, `selection_mode = None`.
- If matches exist and the previous active match still exists semantically in the same target, keep the nearest equivalent match.
- Otherwise select the first match after the primary cursor, wrapping to the first match.

### Highlight Rendering

Render:

- passive highlights for all matches
- distinct highlight for the active match
- distinct selection visuals for search-driven multi-cursor selections

The renderer should consume the same `matches` vector and current `selection_mode`, not re-run matching.

For cross-buffer scopes:

- only the currently visible participating views need highlight paint
- non-visible targets still contribute to match count and navigation order

### Performance

For the first pass, full-buffer rescanning on query changes is acceptable if:

- search is limited to currently open editor content
- matching is plain-text only
- results are computed once per change, not once per paint

If repaint pressure becomes visible, add incremental recompute later. Do not start with premature complexity.

## 6. Replace Design

### Replace Current

- If there is an active match and no multi-cursor selection, replace that match.
- After replacement, recompute matches and advance to the next logical match.

### Replace All

- Build replacement edits from the canonical match list.
- Apply from end to start so indices remain valid.
- Recompute matches after the edit.

For `All Open Tabs`, replacement should only touch open buffers that are included in the scope.

### Replace Across Search Selections

When multi-cursor search mode is active:

- replace all selected search matches, not all matches in the file
- respect the current scope when determining which selected matches are eligible
- preserve deterministic ordering
- clear or normalize multi-selection state after the edit

The edit planner for this should reuse the same replacement application path as replace-all, just with a filtered match set.

## 7. Multi-Cursor Search Design

### User Model

Multi-cursor search should be additive and predictable:

- start with one active search match
- add next match
- add previous match
- stop when no more unmatched hits remain

For cross-target scopes, "next" and "previous" should move through one deterministic global ordering:

- current target first when practical
- then remaining targets in stable tab/view order

### Internal Model

Do not store full duplicate ranges in multiple places.

Use:

- `matches: Vec<SearchMatch>`
- selected match indices for multi-cursor mode

That keeps:

- navigation
- paint
- replacement
- dedupe logic

all tied to one source of truth.

### Conflict Rules

- A search-derived multi-cursor selection should collapse if the search query changes.
- Normal user selection edits should clear search multi-cursor mode unless explicitly preserved.
- Replacing selected search hits should reset to a single active match or no match, whichever is more stable.

## 8. Keyboard Shortcuts

Recommended first-pass bindings:

- `Ctrl + F`: open find
- `Ctrl + H`: open find + replace
- `Enter`: next match
- `Shift + Enter`: previous match
- `Esc`: close search strip and clear highlights
- `Ctrl + D`: add next match to multi-cursor selection
- `Ctrl + Shift + D`: add previous match to multi-cursor selection
- `Alt + Enter`: select all matches into multi-cursor mode
- `Ctrl + Shift + H` or button-driven flow: replace all in scope

If some bindings conflict with existing app behavior, keep the behavior model and adjust the exact keys later.

## 9. Suggested Module Breakdown

- `src/app/services/search.rs`
  - query matching
  - scope-aware target traversal
  - next/previous lookup
  - replace planning

- `src/app/domain/...` or app-state module
  - `SearchSession`
  - `SearchScope`
  - search selection mode

- `src/app/ui/editor_area/...`
  - unified search/replace strip rendering
  - match highlight rendering
  - keyboard dispatch for next/previous/add-next/add-prev

- `src/app/commands.rs`
  - open find
  - open replace
  - next/previous match
  - add next/previous match
  - replace current
  - replace all

## 10. Implementation Sequence

### Phase 1: Search State And Engine

1. Add one unified `SearchSession` with query, replacement text, options, and scope.
2. Add a pure search service for plain-text matching.
3. Add scope resolution for current context, current tab, and all open tabs.
4. Standardize search match ranges in one coordinate system with target metadata.
5. Add tests for empty query, case sensitivity, whole word, scope traversal, next, and previous.

### Phase 2: Live Search UI

1. Add the unified search/replace strip overlay.
2. Wire `Ctrl + F` and `Ctrl + H` to the same surface.
3. Add scope selection to that strip.
4. Recompute matches on query, option, and scope changes.
5. Render active/passive match highlights.
6. Scroll active match into view when navigation changes it.

### Phase 3: Replace

1. Add replace field and replace buttons.
2. Implement replace current.
3. Implement replace all in scope.
4. Add tests for stable replacement ordering and active-match progression across multiple targets.

### Phase 4: Multi-Cursor Search

1. Add `Add Next Match`.
2. Add `Add Previous Match`.
3. Add `Select All Matches`.
4. Reuse the existing edit path to replace selected search matches.
5. Add tests for dedupe, wrap behavior, and state reset after edit.

### Phase 5: Polish

1. Improve no-match and inactive-match visuals.
2. Preserve useful focus behavior when opening/closing search.
3. Add tooltips and discoverability labels.
4. Revisit whether regex should be the next extension.

## 11. Testing Plan

Add focused tests for:

- match generation
- whole-word filtering
- case-sensitive and insensitive search
- scope resolution for current context, current tab, and all open tabs
- next/previous wraparound
- replace current
- replace all in scope
- add-next/add-previous selection growth
- replace-selected multi-cursor matches
- clearing search state when query changes
- clearing/recomputing state when scope changes
- retaining one auto-selected active match after edits when possible

Add UI-level smoke coverage for:

- open/close search
- typing updates highlights
- next/previous navigation changes active match
- replace buttons trigger document edits

## 12. Definition Of Done

This work is done when:

- one unified search/replace strip serves all search actions
- matches highlight while typing
- scope switching works for current context, current tab, and all open tabs
- next/previous navigation works
- replace current and replace all-in-scope work
- multi-cursor search selection works from the same search model
- changing tab/view keeps the search session coherent with the selected scope
- the implementation uses one shared search engine and one shared match model rather than parallel codepaths
