This plan keeps the strong architectural direction from V2, but makes three improvements:

1. It distinguishes planning progress from implementation progress.
2. It closes the biggest experience gaps that usually make search/replace feel fragile.
3. It prioritizes the end-user workflow, not just the underlying engine.

Implementation status is intentionally left conservative here. This document should only claim progress that is visible from planning decisions, not from unverified code.

---

# Search & Replace Plan (V3)

## 1. Current Progress

### What Is Already Well-Decided
The existing plan already establishes several strong foundations:

* **Workspace-level ownership:** Search is treated as a shared workspace capability, not a per-widget bolt-on.
* **Provider-based architecture:** The UI can target buffers now and grow into other searchable surfaces later.
* **Atomic replace thinking:** "Replace All" is expected to be safe and undoable.
* **Cross-pane awareness:** Scope is treated as a first-class concept instead of an afterthought.
* **Index stability strategy:** Reverse-order replacement is the right baseline for avoiding offset drift.
* **Non-modal UX direction:** Search should remain active while the editor stays usable.

### What Is Still Missing
The current plan is strong on architecture, but still under-specified in the areas that determine whether the experience feels truly excellent:

* The exact **interaction contract** for incremental search, replace, and keyboard flow is incomplete.
* The plan does not define how results stay correct while the user is **typing, editing, undoing, or changing focus**.
* There is not yet a clear policy for **replace safety**, especially for multi-buffer or regex replacements.
* The roadmap is not explicit enough about **staged delivery**, fallback behavior, and what must land first.
* The "best search/replacement experience" requires more detail around **discoverability, preview, accessibility, and trust**.

---

## 2. Product Goal

Search and replace should feel:

* **Immediate:** Results appear as the user types.
* **Stable:** Matches do not jump unpredictably when the document changes.
* **Trustworthy:** Replace operations are previewable, undoable, and never ambiguous.
* **Context-aware:** Scope follows user intent without surprising them.
* **Fast at scale:** Small files feel instant; larger workspaces degrade gracefully.
* **Keyboard-first:** Power users never need the mouse, but the UI remains obvious for everyone else.

---

## 3. Experience Principles

### Search Must Never Steal the Editor
The search strip is persistent and non-modal. Users can type in the editor, click elsewhere, or navigate panes without losing the active query or match state unless they explicitly dismiss it.

### The Query Is State, Not a Temporary Widget Value
The search query, options, scope, and active match belong to a durable workspace search session. Closing and reopening the strip should restore the prior session when appropriate.

### Replace Requires More Trust Than Search
Search can be optimistic; replace must be explicit. The UI must always make it obvious:

* what will be replaced
* where replacements will happen
* how many replacements are affected
* whether the operation can be undone as one step

### Scope Should Be Helpful, Not Clever
Auto-defaulting to selection-only can be helpful, but only if the UI makes that scope obvious. Hidden scopes are a major source of confusion.

---

## 4. Architecture

### Search Provider Model
Search logic remains provider-driven so the same session can operate on different searchable surfaces.

Core provider responsibilities:

* expose searchable text targets
* translate provider-local coordinates into a unified match model
* apply replacements transactionally
* report invalidation when content changes
* support reveal/focus behavior for match navigation

### Unified Search Session
A single `SearchSession` continues to live at the workspace level, but it should also explicitly track:

* whether the session is currently **dirty** because underlying content changed
* whether results are **partial** or **complete**
* the **origin of scope** such as manual selection, active editor default, or open-buffers mode
* whether replace actions are currently **allowed**, **blocked**, or **require confirmation**

### Match Identity
A match needs more than offsets. To stay resilient during live edits and cross-buffer navigation, each match should carry:

* target identifier
* target revision/generation
* normalized range
* preview text before and after the match
* replacement preview when relevant

This avoids over-reliance on raw offsets and makes revalidation more reliable.

---

## 5. Scope Model

Supported scopes should be explicit and ordered from narrowest to widest:

* **Selection Only**
* **Active Buffer**
* **Visible Pane Group** if the product supports multi-pane workflows where only some panes are active at once
* **Open Buffers**
* **Workspace Files** as a future phase

Rules:

* If a user opens search with an active selection, default to `Selection Only`, but show that choice clearly.
* If the selection is cleared, do not silently keep a stale selection-only scope.
* Replace actions across more than one buffer must surface a clear count before execution.

---

## 6. Search Semantics

The plan should define first-class support for:

* plain text search
* case sensitivity toggle
* regex toggle
* whole word toggle
* preserve case behavior for replacement as a later enhancement, not a launch dependency

The engine should treat search semantics as a stable contract. The UI cannot feel reliable if the matching rules are fuzzy or inconsistent between search and replace.

---

## 7. Live Update Behavior

This is the largest missing gap in the prior plan and should be treated as a core requirement.

When the document changes while search is open:

* active highlights should refresh automatically
* the active match should remain anchored when possible
* if anchoring is no longer valid, the next sensible match should become active
* stale results should never remain visible as if they are current

When the query changes:

* recomputation should be incremental when possible
* match counts should update immediately
* large searches may show a short-lived "searching" state rather than freezing the UI

When scope changes:

* the active match should reset predictably
* counts and summaries should be recomputed before replace actions are enabled

---

## 8. Replace Safety Model

### Single Replace
Replacing the active match should be immediate and remain within a single undo unit.

### Replace All
Replace All should require strong guarantees:

* one undo step per user-triggered operation
* no partial completion within a target
* no silent skipping of invalid matches
* clear reporting when some targets could not be changed

### Replace Preview
For the best experience, the product should support lightweight preview before destructive multi-target operations.

Minimum acceptable preview:

* total match count
* number of affected buffers
* visible replacement string

Preferred preview:

* per-buffer counts
* a compact list of changed contexts
* regex replacement preview where captures materially affect output

### Confirmation Rules
Confirmation should be selective, not noisy:

* no confirmation for single replace
* no confirmation for replace-all within one active buffer if the count is small and undo is guaranteed
* confirmation for cross-buffer replace-all
* confirmation for regex replace-all when replacement text uses captures or when the result count is high

---

## 9. UX Specification

### Search Strip
The strip should include:

* query field
* result count
* next/previous actions
* replace field
* replace-one and replace-all actions
* scope selector
* option toggles
* dismiss action

### Keyboard Flow
The keyboard contract should be explicit:

* `Ctrl+F`: open search and focus query
* `Ctrl+H`: open replace and focus query or replacement based on current state
* `Enter`: next match
* `Shift+Enter`: previous match
* `Esc`: return focus to the last editor context; a second `Esc` may dismiss the strip if focus is already outside the input
* `Ctrl+Enter` or another deliberate chord: replace current match
* `Alt+Enter` or another deliberate chord: replace all in current scope
* `Ctrl+D`: promote active match into multi-selection, if that feature lands

Exact shortcuts can be adjusted to match existing app conventions, but the flow must be documented and testable.

### Visual Model
The UI should clearly distinguish:

* active match
* passive matches
* out-of-date results
* selection-only mode
* blocked replace state

The user should never have to infer whether they are searching one buffer or many.

### Empty and Error States
The current "No results found" note is a start, but the plan should define:

* no-results visual state
* invalid regex visual state
* large-search-in-progress state
* replace-blocked state when a target is read-only, stale, or otherwise unavailable

---

## 10. Multi-Cursor Integration

Multi-cursor promotion remains a strong power feature, but it should be treated as a second-wave enhancement unless the current selection model already supports it cleanly.

Requirements before shipping it:

* deterministic promotion order
* compatibility with undo/redo
* no ambiguity between search matches and persistent cursors
* clear exit path back to normal search navigation

If this is not ready, the plan should still deliver an excellent search/replace experience without depending on multi-cursor editing.

---

## 11. Performance Strategy

The prior latency target is good, but the plan needs explicit tactics:

* search only visible/active content synchronously
* debounce or background wider scopes
* cache normalized text where practical
* invalidate results by target revision, not by broad global resets
* cap expensive previews for cross-buffer operations

For large workspace search in future phases, background execution and progressive result streaming should be part of the design, not retrofitted later.

---

## 12. Updated Roadmap

### Phase 1: Reliable Single-Buffer Search
Ship the smallest version that already feels solid.

Deliverables:

* workspace-level session state
* active buffer search
* next/previous navigation
* case sensitivity, regex, and whole-word options
* active/passive highlight model
* no-results and invalid-regex states

Exit criteria:

* search remains stable during ordinary typing
* focus flow feels predictable
* no broken highlights after edits

### Phase 2: Trustworthy Replace
Add replace only after search behavior is stable.

Deliverables:

* replace-one
* replace-all within active buffer
* single-step undo for replace-all
* replace counts and blocked-state handling

Exit criteria:

* replace operations never leave stale highlights behind
* replace-all never corrupts match ordering
* failures are visible and recoverable

### Phase 3: Multi-Buffer Scope
Extend the same model across open buffers.

Deliverables:

* open-buffers scope
* cross-buffer navigation
* cross-buffer replace-all summary
* confirmation rules for multi-buffer actions

Exit criteria:

* scope is always visible
* navigation between buffers feels intentional
* replace summaries are accurate and trusted

### Phase 4: Premium Workflow Enhancements
Only after the fundamentals feel excellent.

Candidates:

* session persistence across workspace reopen
* preview-rich replace-all
* multi-cursor promotion
* workspace-on-disk search
* history of recent queries and replacements

---

## 13. Quality Gates

| Criterion | Requirement |
| :--- | :--- |
| **Undo Integrity** | Each replace action is reversible in one user-level undo step. |
| **Index Stability** | Replacing shorter or longer strings never invalidates remaining replacements. |
| **Focus Flow** | Keyboard transitions between search UI and editor are predictable and repeatable. |
| **State Freshness** | Result highlights never persist after they become stale. |
| **Scope Clarity** | Users can always tell what area they are searching and replacing. |
| **Regex Safety** | Invalid regex input is surfaced immediately and never produces misleading results. |
| **Performance** | Active-buffer search remains responsive on large files; wider scopes degrade gracefully. |
| **Replace Trust** | Multi-target replacements show enough context that users feel safe executing them. |

---

## 14. Immediate Plan Updates

The next revision of implementation work should prioritize these items in order:

1. Define the exact `SearchSession` lifecycle, including invalidation and refresh rules.
2. Lock down the keyboard and focus contract before building more UI.
3. Ship excellent active-buffer search before broadening scope.
4. Add replace with strict undo guarantees and explicit blocked/error handling.
5. Treat cross-buffer replace and multi-cursor promotion as follow-on milestones, not prerequisites for a great first release.

This keeps the plan ambitious, but grounded in the behaviors users actually notice first: speed, clarity, and trust.
