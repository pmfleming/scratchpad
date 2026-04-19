This implementation plan translates the updated product plan in `docs/search-replace-plan.md` into a realistic migration path from the current codebase.

The goal is not to rewrite search from scratch. The current implementation already has meaningful infrastructure. The goal is to move from the current "good internal feature" to a more durable, trustworthy, and extensible search and replace system with minimal regressions.

---

# Search & Replace Implementation Plan

## 1. Current Implementation Snapshot

The current code already provides a strong foundation:

* `src/app/app_state/search_state.rs`
  Hosts workspace-level search state, open/close behavior, match navigation, and active-buffer replace actions.
* `src/app/app_state/search_state/runtime.rs`
  Handles invalidation, target collection, result application, highlight refresh, and active-match selection.
* `src/app/app_state/search_state/worker.rs`
  Runs asynchronous matching across collected buffer snapshots and builds grouped result entries.
* `src/app/services/search.rs`
  Implements plain-text matching with case-sensitive and whole-word options using character-based ranges.
* `src/app/ui/search_replace/*.rs`
  Renders the search strip, replace controls, grouped result list, and current keyboard-driven actions.
* `src/app/shortcuts.rs`
  Wires `Ctrl+F`, `Ctrl+H`, and `Esc`.

### What Already Exists

* Workspace-level search state instead of per-widget local state.
* Async search worker with generation-based cancellation.
* Scopes for:
  * active buffer
  * active workspace tab
  * all open tabs
* Grouped search results with file and tab context.
* Active match navigation across tabs and panes.
* Live invalidation through `mark_search_dirty()`.
* Active-buffer replace current and replace all.
* Reverse-order replace-all in the active buffer.
* Character-based search ranges, which is the correct baseline for Unicode-safe editor integration.

### Gaps Relative To The Updated Plan

The current implementation still falls short of the updated product plan in several key areas:

* Search state is still modeled primarily as "query + flags + match list", not as a fuller search session with explicit freshness, status, and replace eligibility.
* There is no regex support yet.
* There is no `Selection Only` scope and no scope-origin tracking.
* Match identity is too thin for robust revalidation beyond immediate reruns.
* Result freshness is implicit; the UI does not clearly distinguish fresh, stale, partial, invalid, or blocked states.
* Replace is implemented only for the active buffer.
* There is no cross-buffer replace plan, confirmation model, or summary layer.
* The current worker pipeline is snapshot-based, but not yet abstracted as a provider model.
* The keyboard contract is workable, but not yet formalized to match the updated plan.
* The UI has useful grouped results, but limited replace-preview and replace-safety affordances.

---

## 2. Migration Strategy

The migration should follow four rules:

1. **Preserve working search behavior while evolving internals.**
2. **Separate engine/state changes from UX changes where possible.**
3. **Ship search robustness before shipping more replace power.**
4. **Avoid broad rewrites of the editor transaction system.**

This means the implementation should build on the existing `SearchState`, runtime, worker, and results UI instead of replacing them all at once.

---

## 3. Target Technical Shape

The updated plan implies five structural shifts.

### A. Evolve `SearchState` Into A Fuller Session Model

Current `SearchState` should become the host for a richer session contract rather than being replaced outright.

Add concepts for:

* session status
* result freshness
* partial vs complete results
* replace availability
* scope origin
* result anchor / active match retention policy

Suggested new concepts:

```rust
enum SearchStatus {
    Idle,
    Searching,
    Ready,
    NoMatches,
    InvalidQuery(String),
    Error(String),
}

enum SearchFreshness {
    Fresh,
    Stale,
}

enum ReplaceAvailability {
    Allowed,
    Disabled,
    RequiresConfirmation,
    Blocked(String),
}

enum SearchScopeOrigin {
    Manual,
    SelectionDefault,
    ActiveContextDefault,
}
```

This can remain inside `src/app/app_state/search_state.rs` at first, then be split if it grows too large.

### B. Introduce Provider-Like Search Targets Without Breaking Current Flow

The current `SearchTargetSnapshot` model is a good stepping stone toward a provider architecture.

Instead of introducing a large trait hierarchy immediately, phase in a lightweight provider adapter:

* keep `SearchTargetSnapshot`
* add per-target revision / generation metadata
* isolate target collection behind a search-target service layer

First step:

* move target collection logic out of `runtime.rs` into a dedicated search-target module

This gives the architecture room to grow without destabilizing current search behavior.

### C. Separate Search Planning From Replace Execution

The current code jumps fairly directly from selected matches to replacement execution. To support safe replace growth, add an intermediate planning layer.

Suggested concepts:

```rust
struct ReplacementPlan {
    scope: SearchScope,
    targets: Vec<ReplacementTargetPlan>,
    total_match_count: usize,
    requires_confirmation: bool,
}

struct ReplacementTargetPlan {
    buffer_id: BufferId,
    view_id: Option<ViewId>,
    replacements: Vec<(Range<usize>, String)>,
}
```

This lets the app:

* preview replacement counts
* enforce confirmation rules
* reuse the same replacement planning for active-buffer and cross-buffer operations

### D. Make Freshness Explicit

The current implementation refreshes quickly, but freshness is mostly inferred. That is not enough for a best-in-class experience.

The new implementation should explicitly track:

* the generation requested
* the generation displayed
* whether the underlying content changed after the displayed result set was produced

The UI should be able to show:

* searching
* results ready
* stale results pending refresh
* invalid query
* replace blocked

### E. Keep Character-Based Ranges As The Core Coordinate Model

This should remain unchanged. It is already the right internal coordinate system and should continue to be the source of truth for:

* search results
* cursor movement
* replace planning
* preview generation

---

## 4. Phase Plan

## Phase 0: Baseline And Guardrails

Before major refactors, capture current behavior and performance.

### Goals

* Prevent regressions in search navigation and replace behavior.
* Define what already works and should remain stable.

### Work

* Audit current search tests in:
  * `src/app/services/search.rs`
  * `src/app/app_state/search_state/tests`
* Add or expand coverage for:
  * active-buffer replace current
  * active-buffer replace all
  * scope switching
  * active match retention after query changes
  * async search result application
* Preserve the existing performance profile entry points in `src/profile.rs`.

### Definition Of Done

* Current behavior is documented and covered well enough to support refactoring.

---

## Phase 1: Session Model Refactor

This phase aligns state management with the updated plan without changing user-facing behavior too much.

### Goals

* Expand `SearchState` into a clearer session model.
* Make result status and freshness explicit.

### Work

* Add status/freshness/replace-availability fields to `SearchState`.
* Rename or wrap fields where needed so the model reads as a session, not just a UI bag.
* Track:
  * latest requested generation
  * latest applied generation
  * whether displayed results are stale
* Promote `dirty` handling into clearer lifecycle rules:
  * `dirty` means content/options changed
  * `searching` means request in flight
  * `stale` means current results are no longer authoritative

### Recommended File Targets

* `src/app/app_state/search_state.rs`
* `src/app/app_state/search_state/runtime.rs`
* `src/app/ui/search_replace/state.rs`
* `src/app/ui/search_replace/results.rs`

### Definition Of Done

* The app can distinguish idle/searching/ready/no-results/error-like states in state and UI.
* Existing search behavior still works.

---

## Phase 2: Formalize Search Semantics

This phase upgrades the engine contract to match the updated product plan.

### Goals

* Add regex support.
* Make query validation explicit.
* Keep plain-text search behavior stable.

### Work

* Extend `SearchOptions` in `src/app/services/search.rs` to support query mode:

```rust
enum SearchMode {
    PlainText,
    Regex,
}
```

* Add regex compilation and validation.
* Return structured search outcomes so invalid regex can be surfaced without pretending there are simply zero results.
* Keep whole-word and case-sensitive behavior consistent between plain-text and regex modes.
* Defer advanced replacement transformations like preserve-case until later.

### Recommended File Targets

* `src/app/services/search.rs`
* `src/app/app_state/search_state.rs`
* `src/app/app_state/search_state/worker.rs`
* `src/app/ui/search_replace/controls.rs`
* `src/app/ui/search_replace/results.rs`

### Definition Of Done

* Plain-text search still works as before.
* Regex search works for matching.
* Invalid regex is surfaced as an explicit state, not as a silent no-results condition.

---

## Phase 3: Scope Model Expansion

This phase aligns search scope behavior with the updated plan.

### Goals

* Add `Selection Only`.
* Improve clarity around scope defaults and scope transitions.

### Work

* Extend `SearchScope`:
  * `SelectionOnly`
  * keep `ActiveBuffer`
  * keep `ActiveWorkspaceTab`
  * keep `AllOpenTabs`
* Add scope-origin tracking so the app knows whether selection scope was auto-selected or manually chosen.
* Add selection snapshot or selection-derived target collection for the active editor.
* Define rules for when selection-only should automatically fall back or be cleared.

### Recommended File Targets

* `src/app/app_state/search_state.rs`
* `src/app/app_state/search_state/runtime.rs`
* `src/app/ui/search_replace/controls.rs`
* editor/view selection helpers as needed

### Definition Of Done

* `Ctrl+F` with a live selection can default into selection scope.
* The UI clearly shows when the search is limited to a selection.
* Clearing the selection does not leave the app in a misleading hidden scope state.

---

## Phase 4: Target Identity And Freshness

This is the most important correctness phase.

### Goals

* Make search results safer under live editing.
* Improve active-match retention behavior.

### Work

* Add revision metadata to `SearchTargetSnapshot`.
* Add target identity metadata to `SearchMatch`.
* Preserve or recover the active match based on:
  * same target
  * same revision when valid
  * nearest sensible fallback otherwise
* Mark results stale immediately when an underlying target mutates after the current result generation.
* Avoid showing old highlights as if they are still valid.

### Recommended File Targets

* `src/app/app_state/search_state.rs`
* `src/app/app_state/search_state/runtime.rs`
* `src/app/app_state/search_state/worker.rs`
* highlight helpers and editor view integration

### Definition Of Done

* Search results stay coherent during ordinary typing.
* Active match selection is preserved more reliably after edits.
* The UI can visibly indicate stale or refreshing results.

---

## Phase 5: Replace Planning Layer

This phase introduces the missing abstraction needed for trustworthy replace.

### Goals

* Separate identifying replacements from applying them.
* Enable preview and confirmation rules.

### Work

* Add a `ReplacementPlan` model.
* Build plans from current matches rather than replacing directly from UI actions.
* Keep active-buffer replace execution working through the current transaction system.
* Compute:
  * total replacement count
  * affected buffer count
  * confirmation requirement
  * blocked targets if any

### Recommended File Targets

* `src/app/app_state/search_state.rs`
* `src/app/app_state/search_state/runtime.rs`
* new `replace_plan` or `replace` module under `search_state`

### Definition Of Done

* Active-buffer replace current and replace all go through a common plan-and-execute path.
* The UI can query replacement counts and safety state before executing.

---

## Phase 6: Trustworthy Replace UX

This phase improves user trust without immediately expanding replace scope to everything.

### Goals

* Make replace behavior feel safe and explicit.
* Improve status, confirmation, and result handling.

### Work

* Add replace availability / blocked state to UI state.
* Disable replace actions when:
  * query invalid
  * no matches
  * stale state not yet recomputed
  * target not writable
* Add a lightweight replace summary:
  * matches affected
  * buffers affected
* Add confirmation rules for riskier operations.
* Make `Esc`, `Enter`, and replace shortcuts align with the updated keyboard contract.

### Recommended File Targets

* `src/app/ui/search_replace/controls.rs`
* `src/app/ui/search_replace/results.rs`
* `src/app/ui/search_replace/state.rs`
* `src/app/shortcuts.rs`

### Definition Of Done

* Replace actions are not ambiguous.
* UI feedback is clear before and after replacement.
* Keyboard flow is consistent and documented.

---

## Phase 7: Cross-Buffer Replace

This is the largest functional expansion and should land only after the plan-and-execute layer is stable.

### Goals

* Replace across open buffers safely.
* Preserve the transaction and status model.

### Work

* Extend replacement planning to all open targets in scope.
* Decide transaction semantics:
  * preferred: one user-level undo step per buffer, plus a top-level grouped action summary
  * if true cross-buffer single-undo is not feasible with the current transaction system, document that limitation and avoid pretending otherwise
* Add per-buffer success/failure handling.
* Add final summary toast/status text.

### Recommended File Targets

* `src/app/app_state/search_state/runtime.rs`
* transaction orchestration files
* `src/app/ui/search_replace/controls.rs`

### Definition Of Done

* Replace-all across open buffers works with clear user feedback.
* Failures are surfaced explicitly.
* Undo behavior is documented and predictable.

---

## Phase 8: Providerization

Only after the search and replace workflow is stable should we generalize the architecture for future searchable surfaces.

### Goals

* Decouple search UI/session from text-buffer-only target collection.

### Work

* Introduce a `SearchProvider` abstraction or equivalent adapter layer.
* Migrate current buffer-target collection to the provider interface.
* Keep the worker pipeline snapshot-driven.

### Definition Of Done

* Search target collection is no longer hard-coded in workspace-tab traversal logic.
* The architecture can support future searchable surfaces without redoing the session model.

---

## 5. UX Rollout Order

To keep quality high, ship visible improvements in this order:

1. Better state/status model.
2. Regex and explicit invalid-query handling.
3. Selection-only scope and scope clarity.
4. Stale-result visibility and active-match retention improvements.
5. Replace planning and disabled/blocked replace states.
6. Cross-buffer replace.
7. Rich preview and advanced workflow improvements.

This order matches the updated product goal: users will notice speed, clarity, and trust long before they care about provider extensibility.

---

## 6. Testing Plan

## Unit Tests

Add or expand tests for:

* plain-text matching
* regex matching
* invalid regex handling
* whole-word behavior for plain text and regex
* active match retention after result recompute
* replacement planning
* reverse-order application correctness
* scope resolution, including `SelectionOnly`

## Integration Tests

Add app-level coverage for:

* open search, type query, navigate results
* switch search scope while query remains active
* replace current in active buffer
* replace all in active buffer
* cross-buffer replace all
* stale-result refresh after buffer edits
* invalid regex disables replace

## Manual Verification

Verify:

* `Ctrl+F`, `Ctrl+H`, `Enter`, `Shift+Enter`, and `Esc`
* search while typing in the editor
* match highlighting after edits
* grouped results navigation across tabs and panes
* replace safety messaging
* large-file responsiveness

---

## 7. Risks And Watchouts

1. `search_state.rs` is already carrying a lot of responsibility.
   Avoid continuing to enlarge it without splitting supporting logic into focused modules.

2. Cross-buffer replace may expose transaction model limits.
   The implementation plan should respect current undo architecture instead of overpromising "single undo" across the entire workspace if the editor infrastructure cannot actually support that yet.

3. Regex support can complicate preview, replace, and whole-word semantics.
   Land regex matching first before landing regex replacement features that depend on captures.

4. Selection-only search can become confusing if selection invalidation is not explicit.
   Hidden scope behavior will make the UX feel broken even if the underlying engine is correct.

5. Result freshness is easy to get mostly right and still feel wrong.
   The UI needs visible stale/searching/blocked states, not just correct internal flags.

---

## 8. Recommended First Slice

If this should be broken into the smallest high-value implementation slice, do this first:

1. Refactor `SearchState` into a clearer session/status model.
2. Surface explicit ready/searching/no-results/invalid-query states in the UI.
3. Add regex matching.
4. Add `SelectionOnly` scope with explicit scope visibility.
5. Add replace planning for active-buffer replace actions.

That slice keeps the work grounded in current architecture, improves the user experience quickly, and creates the right base for cross-buffer replace afterward.

---

## 9. Exit Criteria For Alignment With The Updated Plan

The implementation should be considered aligned when all of the following are true:

* Search state behaves like a durable session, not a temporary widget.
* Search status and freshness are explicit in both state and UI.
* Plain-text and regex search are both supported.
* `SelectionOnly`, `ActiveBuffer`, and broader scopes behave predictably.
* Replace uses a plan-and-execute model rather than ad hoc direct actions.
* Replace operations are visibly safe, undoable, and clearly scoped.
* Cross-buffer replace has explicit confirmation and summary behavior.
* The architecture can evolve toward providers without another large search rewrite.
