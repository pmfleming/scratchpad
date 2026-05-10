# Live Replace Preview Implementation Plan

## Goal

When the replace box is open, changing the replacement text should make the affected open files look as if the replacement has already happened. If the user cancels, closes search, changes scope/query, or leaves replace mode without executing Replace Current or Replace All, every file must return to its original visible state with no document mutation, no dirty flag, no disk write, and no undo entry.

This feature should feel like a reversible visual preview, not like a partially applied edit.

## Current Baseline

The codebase already has useful pieces for this:

- `src/app/app_state/search_state.rs` owns durable search state, including query, replacement, scope, result freshness, match list, and replace confirmation.
- `src/app/app_state/search_state/replace.rs` already builds validated replacement plans and applies real replacements through the document undo path.
- `src/app/app_state/search_state/api.rs` calls `refresh_search_visual_state()` when `set_search_replacement()` changes the replacement text.
- `src/app/app_state/search_state/visual.rs` already pushes a replacement preview string into active-tab views when replace mode is open and results are ready.
- `src/app/domain/view.rs` already stores transient `search_replacement_preview` on `EditorViewState` and clears it with search highlights.
- `src/app/ui/editor_content/native_editor/painting.rs` already paints replacement preview labels over search highlight ranges.

The missing behavior is that the current preview is a single label string attached to highlighted ranges. It does not render each buffer as a projected edited document, does not carry per-match expanded replacement text, and does not clearly model preview lifecycle as its own transient state.

## Product Contract

The user-facing rules should be explicit:

- Typing in the replacement field updates the preview immediately for fresh matches.
- The original document text remains the source of truth until Replace Current or Replace All is invoked.
- Cancelling preview requires no undo because nothing was actually edited.
- Replace Current and Replace All always rebuild or validate a real replacement plan before mutating text; they must not blindly commit preview paint state.
- Preview disappears when search closes, replace mode closes, query/options/scope changes, results become stale, the target buffer changes, or the query becomes invalid.
- Preview must match actual replacement semantics, including regex replacement expansion once regex replacement is supported in execution.
- Empty replacement should look like deletion, not like a missing preview.
- Multi-line replacement should be represented predictably, even if the first implementation uses a compact marker before full reflow support.

## Recommended Technical Shape

Add a dedicated transient preview model derived from current search results and replacement text.

Suggested ownership:

- Keep canonical search and replacement input in `SearchState`.
- Add preview state under search state or a focused `search_state::preview` module.
- Push only view-specific display data into `EditorViewState`.
- Never store preview data in `BufferState`, `TextDocument`, history, session persistence, or file IO.

The preview state should track:

- search generation used to build the preview
- scope used to build the preview
- replacement input used to build the preview
- target buffer ids and document revisions
- per-match original range
- per-match expanded replacement text
- preview status: unavailable, ready, stale, blocked

This separates "what would happen" from "what has happened."

## Preview Planning

Build live preview from the same source of truth as real replacement, but stop before mutation.

Plan builder responsibilities:

- Run only when search is open, replace is open, query is non-empty, results are fresh, and status is ready.
- Use current `SearchMatch` entries as inputs.
- Validate target revision and expected text before preview is marked ready.
- Reuse replacement expansion logic from real replace so preview and commit cannot disagree.
- Group matches by target buffer/view.
- Preserve apply-order information for real replace, but expose document-order information for display.
- Fail closed if the query is invalid, replacement expansion fails, or results are stale.

The existing `ReplacementPlan` can be reused carefully, but the preview model should not require confirmation rules and should not call mutation helpers. If reusing `ReplacementPlan` makes that boundary fuzzy, introduce a sibling preview plan.

## Rendering Strategy

There are two viable implementation slices. The target behavior should be the second one.

### Slice 1: Better Overlay Preview

Upgrade the current paint path before changing layout:

- Replace the single per-view preview string with per-range preview entries.
- Paint each replacement using the exact expanded text for that match.
- Dim or cover the original match region so it reads as "pending replacement," not merely annotation.
- Show a clear deletion marker for empty replacement.
- Keep this active only for visible views in the active tab.

This is a low-risk bridge because it builds on `search_highlights` and current native-editor painting. It improves trust, especially for regex or per-match replacements, but it will not fully reflow lines when replacement length differs.

### Slice 2: Virtual Edited Layout

Render the editor from a projected text view while preview is active:

- For each visible buffer slice, apply preview replacements to an in-memory projection.
- Layout the projected text for painting only.
- Keep cursor, selection, search navigation, and commit coordinates anchored to original document coordinates.
- Maintain a mapping between original character offsets and projected display offsets for highlights, cursor reveal, hit testing, and scroll stability.
- Style inserted/replaced regions as preview text so users can tell the view is temporary.

This produces the experience the user asked for: the files look visibly edited while typing, but the underlying files remain unchanged.

## Preview Lifecycle

Preview should be rebuilt or cleared from the same places that already manage search freshness.

Rebuild preview when:

- replacement text changes
- search results become fresh
- replace mode opens
- the active tab/view changes and has eligible matches
- scope changes and the next search result generation is ready

Clear preview when:

- search closes
- replace mode closes
- query/options/scope change before new results are ready
- search is marked stale
- a target buffer mutates
- replacement expansion fails
- Replace Current or Replace All finishes
- selection-only scope loses its selection

`clear_search_highlights()` already clears `search_replacement_preview`; keep that behavior and make sure any new per-range preview data follows the same cleanup path.

## Commit And Cancel Semantics

Cancel path:

- Clear preview state only.
- Do not call document replacement APIs.
- Do not create undo records.
- Do not mark buffers dirty.
- Do not alter cursor history except ordinary focus restoration.

Commit path:

- Replace Current and Replace All build or validate a fresh replacement plan.
- If the preview generation is stale, block commit and refresh search rather than committing old visual state.
- After successful commit, clear preview, refresh search, and show the normal replacement summary.
- Undo should revert only committed replacements, never preview state.

## Scope Behavior

The preview should respect the current search scope:

- Selection Only: preview only inside the captured selection; clear when the selection is no longer valid.
- Active Buffer: preview the active buffer.
- Current Tab: preview all eligible visible views in the active tab.
- All Open Tabs: compute preview data for all eligible open buffers, but only paint views that are currently visible. When navigating to another tab, apply the already-valid preview if its generation and revisions still match.

For performance and clarity, the first implementation can paint only the active tab while still reporting the full replacement count from the plan.

## Performance Guardrails

- Debounce or coalesce preview rebuilds behind the existing search refresh path if replacement typing becomes expensive.
- Cap projected layout work to visible slices plus overscan.
- Avoid cloning whole large buffers for every keystroke; build projections from affected ranges in the visible slice.
- Reuse current generation and revision checks to skip redundant preview rebuilds.
- Disable rich virtual layout preview for very large result sets if necessary, falling back to overlay preview plus counts.

## Testing Plan

Add focused coverage for:

- typing replacement text updates preview without changing buffer text
- closing search clears preview and leaves document revision unchanged
- closing replace mode clears preview
- query or scope changes clear stale preview
- buffer edits during preview clear or rebuild preview safely
- Replace Current uses real validation, not preview state
- Replace All after preview produces one real undo path per current rules
- empty replacement previews as deletion
- replacement longer than match previews without corrupting ranges
- multi-line replacement preview behavior
- regex replacement preview matches commit semantics
- all-open-tabs preview does not mutate hidden tabs

Manual verification should include active buffer, current tab, all open tabs, selection-only, short-to-long replacements, long-to-short replacements, empty replacement, Unicode text, and cancelling with `Esc`.

## Suggested Implementation Order

1. Define the transient preview model and lifecycle rules.
2. Convert current single-string preview into per-match preview entries.
3. Make preview planning reuse real replacement expansion and validation.
4. Harden clear/rebuild behavior across close, stale search, scope changes, and buffer edits.
5. Upgrade painting to make overlay preview read as a pending edit.
6. Add virtual edited layout for visible slices.
7. Extend virtual preview across all visible views in wider scopes.
8. Add performance fallback for very large previews.

## Acceptance Criteria

- Replacement typing visibly changes the editor presentation for every eligible visible match.
- Cancelling preview restores the exact original visible document.
- Buffer text, document revision, dirty state, undo history, and saved file contents are unchanged until an explicit replace command runs.
- Actual replace commands remain validated, undoable, and consistent with the preview.
- Stale or invalid preview states fail closed instead of showing misleading edits.
