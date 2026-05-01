# Undo History Improvement Plan

This document reviews the current undo/history implementation and defines a new best-practice direction.

The new direction is explicit:

- remove file operations, tab operations, layout operations, and similar workspace actions from undo history
- focus the history product on becoming the best possible undo/redo system for text changes
- make the primary history a complete list of text changes
- make the secondary history a way to undo only the changes made to one specific file, even when those changes are interleaved with edits to other files

File, tab, and workspace actions should be excluded from undo/redo rather than moved into a parallel history surface.

## 1. Current Implementation Review

Scratchpad currently has two different history systems.

### A. Per-buffer text undo/redo

Text editing history lives in `TextDocument` and is operation-based.

Current characteristics:

- `TextDocument` stores `operation_undo` and `operation_redo` stacks.
- Each record stores inserted/deleted text plus previous/next selection.
- New edits clear redo history.
- History depth is capped by `TEXT_DOCUMENT_MAX_UNDOS = 100`.
- `Ctrl + Z` and `Ctrl + Y` operate on the active buffer's text-operation history.
- This path already uses the document and piece-tree layer, which is the right foundation for text-only undo.

Relevant files:

- `src/app/domain/buffer/document.rs`
- `src/app/domain/buffer/state.rs`
- `src/app/app_state/workspace/mutation.rs`
- `src/app/ui/editor_content/native_editor/interactions/keyboard.rs`

### B. Legacy workspace snapshot history

Workspace history previously lived in a separate snapshot-based history surface, but that surface has now been removed from the project.

Current characteristics:

- Each entry stores a full `TransactionSnapshot` captured before an action.
- `TransactionSnapshot` clones the full `TabManager` plus surface/settings state.
- The removed snapshot-history surface mixed text-edit labeling with file/tab/layout-style operations.
- Undoing a history entry restores a snapshot and truncates later entries.
- There is no symmetric workspace redo path.
- The transaction-log UI exposes `Undo to this point`, not a focused text-undo model.

Relevant files:

- `src/app/transactions.rs`
- removed snapshot-history UI and store code
- `src/app/commands/tests.rs`

## 2. Main Findings

### Finding 1. Non-text actions are diluting undo history

File operations, tab operations, split/layout operations, and other workspace actions are mixed into history even though the stated goal is now best-in-class undo/redo of text changes.

Practical effect:

- the undo surface is noisy
- the mental model is weaker than it should be
- users cannot trust that "history" primarily means text change history

### Finding 2. The removed snapshot-history surface was not a good fit for text-first undo

The removed snapshot-history surface was snapshot-based and rollback-oriented. That was a poor primary model for high-frequency text editing.

Practical effect:

- it is heavier than a text-operation ledger needs to be
- it mixes metadata and rollback payload too broadly
- it encourages workspace-state rollback instead of precise text-change reversal

### Finding 3. The document-layer undo foundation is directionally correct

The piece-tree-backed `TextDocument` operation history is already the right substrate for text undo/redo.

Practical effect:

- the next plan should build around that layer, not around workspace snapshots
- text history should become more complete and more visible, not replaced by a broader workspace engine

### Finding 4. A complete text history and a per-file selective undo are not the same thing

A global text ledger is straightforward. A per-file undo across dispersed history is a selective-undo problem.

Practical effect:

- the secondary per-file undo cannot be treated as a separate independent stack copied from the primary history
- it needs to be a filtered or selective view over the same underlying text transaction ledger
- dependency and rebase behavior must be designed deliberately

### Finding 5. Redo should have one shortcut

The text-only undo product should remove `Ctrl + Shift + Z` from undo/redo behavior. `Ctrl + Z` should mean undo, and `Ctrl + Y` should mean redo.

Practical effect:

- there is no conflict between redo and any removed history surface
- shortcut tests only need to validate one redo binding
- future history UI work cannot accidentally reclaim `Ctrl + Shift + Z`

## 3. Product Decision

The history product should now be text-only.

That means:

- text edits belong in undo/redo
- replace-current and replace-all belong in undo/redo as text transactions
- live preview states do not belong in undo/redo
- file open/save/close do not belong in undo/redo
- tab open/close/reorder/combine do not belong in undo/redo
- split/layout/settings navigation do not belong in undo/redo
- closing a file removes that file's text transactions from visible/selectable undo history

Non-text actions should stay outside undo/redo entirely. Do not keep mixing those actions into the primary undo surface.

## 4. Best-Practice Target Model

### A. Primary history: complete text-change list

The primary undo list should be the complete chronological list of committed text changes across the workspace.

Expected properties:

- every committed text mutation appears exactly once
- entries are ordered by commit time
- each entry represents one user-meaningful text transaction
- entries for a file are retained only while that file remains open
- examples: typing burst, paste, delete burst, replace-current, replace-all, multi-cursor edit, formatter pass if treated as one text action

This primary list is the source of truth.

#### Transaction commit boundaries (coalescing rules)

The ledger needs deterministic rules for when in-flight edits become a single committed transaction. Use these rules:

- typing in the same buffer coalesces into one transaction until any of the following occurs: idle gap of 750 ms, cursor/selection move that is not a direct consequence of the keystroke, focus change, save, explicit undo/redo, or a non-typing edit (paste/delete-line/replace).
- backspace/delete bursts coalesce under the same idle/focus rules but do not coalesce with adjacent insert bursts.
- paste, cut, drag-and-drop, formatter pass, and replace-current each commit immediately as their own transaction.
- multi-cursor edits commit as one transaction covering all carets in that single user gesture.
- replace-all commits as exactly one transaction even when it spans multiple buffers; the transaction's execution payload is a vector of per-buffer inverse operations applied atomically.
- save does not create an undo entry, but it acts as a coalescing boundary: the next keystroke starts a new transaction.

#### Live preview vs commit

Live preview states (search highlight overlays, replace previews, IME composition, refactor previews) must mutate display state only. They never insert into the ledger. A preview becomes a transaction only when the user confirms it; cancelling a preview must leave the ledger and document untouched.

#### Selection-only and cursor-only changes

Pure cursor moves, selection changes, scroll, and viewport changes never create ledger entries. The ledger captures text mutations only.

### B. Secondary history: per-file selective undo

The secondary undo should let the user undo only the changes made to one specific file even when those changes are dispersed through the global text history.

Best-practice interpretation:

- this is not a separate duplicated undo stack per file
- this is a filtered/selective view over the same primary text ledger
- undoing from this view should support arbitrary older transactions that target the selected file, not only the latest visible file transaction
- older per-file undo should use rebase/conflict handling against later edits in the same file
- unrelated text changes to other files must remain intact

Important design constraint:

- per-file selective undo is more advanced than normal linear undo and should be treated as a selective-undo feature, not as a cheap filter over display rows only
- if an older file edit cannot be rebased cleanly over later same-file edits, the command should fail clearly and leave the document unchanged

### C. Third history lens: recommended options

The best third lens is not another structural workspace history. It should still be text-focused.

Recommended option:

- change-set or source lens

Examples:

- only changes made by search/replace
- only changes made by paste operations
- only changes made by multi-cursor editing
- only changes made by formatter/refactor commands

Why this is the strongest third option:

- it stays text-focused
- it remains meaningful to users
- it aligns with how large text edits are often mentally grouped
- it avoids dragging file/tab/layout operations back into undo history

Other acceptable ideas:

- time/session lens: changes made in the last few minutes or since the last save
- author/source lens: user typing versus automation/formatter if that distinction ever matters

Less recommended idea:

- tab-scoped undo view

Reason:

- tabs are transient presentation containers; files and change sets are more stable identities for text history

## 5. Architecture Direction

### A. Canonical text transaction ledger

The system should maintain one canonical ledger of committed text transactions.

The ledger should be piece-tree-backed, but it should not be stored inside `PieceTreeLite`. `PieceTreeLite` remains the per-document storage and edit execution engine; the ledger is an app-level history structure that records committed text transactions using payloads produced by that engine.

Each transaction should carry:

- transaction id
- timestamp
- affected file identity
- affected buffer identity
- category or source tag
- undo payload
- redo payload
- coalescing metadata
- optional display metadata

This ledger should be the engine behind both the primary global text history and any filtered views.

Do not couple global multi-file history into single-buffer tree storage. A single piece tree only knows about one document, while the ledger must order text changes across open files and prune entries when a file closes.

When a file is closed, the ledger must purge or tombstone all transactions associated with that file so they no longer appear in the primary history, per-file history, source/change-set lenses, redo queues, or selectable undo targets. Closing the file itself is not an undoable history entry.

### B. Piece-tree-backed undo payloads

Do not store full text snapshots for normal text undo. Store operation records and rebasing metadata that refer to piece-tree character coordinates, revisions, and anchors.

The text undo engine should interact with and utilize the piece tree through the document layer.

That means:

- normal linear undo/redo should continue to use operation records and piece-tree-backed edits generated by `TextDocument`
- the history system should orchestrate committed transactions, but the storage and text reversal path should stay rooted in `TextDocument`
- selective older per-file undo should store enough revision, anchor, and affected-range metadata to rebase the inverse operation over later same-file edits
- conflict detection should be atomic: if the operation cannot be mapped cleanly onto the current piece tree, the command fails and leaves the document unchanged
- redo payloads should remain derived from the same operation record rather than a workspace snapshot
- do not reintroduce broad workspace snapshots as the normal text-undo mechanism

### C. History metadata separated from execution payloads

The data needed to render a row in history should be distinct from the data needed to reverse the text change.

Separate concerns:

- display metadata: label, file, source, time, summary
- execution payload: inverse text operation, redo text operation, selection/reveal restoration state

### D. Per-file undo as a filtered/selective view, not a duplicate stack

The secondary per-file undo should be implemented as a selective lens over the canonical ledger.

That implies:

- one underlying history source of truth
- filtered projections for file-specific views
- arbitrary older file transactions can be selected for undo from the file lens
- rebase rules must translate the selected transaction through later same-file edits before applying the inverse payload
- conflict rules must detect when the selected transaction's inverse no longer applies cleanly
- failed selective undo must be atomic: no partial document mutation and no redo/history cursor change

Closed files must not have a selectable per-file undo lens. If the user closes a file, all undo entries associated with that file should disappear from every history view immediately.

### E. Non-text activity removed from history

File, tab, and workspace actions should not be represented as a second history surface.

Do not make those the same thing as undo/redo, and do not replace the removed snapshot-history surface with another parallel log.

### F. Closure model: purge, not tombstone

When a file closes, the ledger must purge its transactions outright. Do not retain tombstones. Reasons:

- tombstones invite UI ambiguity ("greyed-out unreachable rows")
- tombstones leak buffer identities into a closed-file world the user has already left
- selective per-file undo cannot operate on a closed file anyway, so retained metadata serves no product purpose

Purge is the authoritative model. Closing a file is silent on undo history except by removing the file's entries.

### G. Redo invalidation policy

Redo entries are invalidated according to these rules:

- a new committed text transaction in any file clears the global redo stack for the primary history view.
- a per-file selective undo clears redo entries that touch the same file; redo entries that touch only unaffected files are preserved.
- closing a file purges that file's redo entries the same way it purges its undo entries.
- failed selective undo (rebase conflict) does not change the redo stack, because the document was not mutated.

### H. Save markers and clean state

Saving a file does not create an undo entry, but the ledger should record a "clean checkpoint" against the affected buffer so that undo/redo navigation can show whether the buffer is currently at a saved state. Clean checkpoints are display metadata, not transactions.

### I. External file changes

If a buffer is reloaded from disk because of an external change, all of that buffer's transactions are purged from the ledger using the same path as file close. Reload starts a fresh history identity for that buffer. The reload itself is not undoable.

### J. Persistence across sessions

Undo history is in-memory only and does not persist across application restart. Any future persistence design is out of scope for this plan and must be decided as a separate product.

### K. Retention budget

The canonical ledger has an explicit retention policy:

- soft cap: 5,000 transactions across the workspace
- hard cap: 10,000 transactions; oldest transactions are dropped first when the hard cap is exceeded
- per-buffer cap: 2,000 transactions; oldest same-buffer transactions are dropped first
- the previous `TEXT_DOCUMENT_MAX_UNDOS = 100` per-buffer document-level cap is removed in favour of the ledger-level caps

Dropped transactions are removed from primary, per-file, and source-lens views consistently and are not redo targets.

## 6. Phase Plan

## Phase 0. Freeze the text-only product contract

Goals:

- define undo history as text-only
- define which text mutations count as committed transactions
- define the canonical shortcut map

Deliverables:

- explicit list of included transaction categories
- explicit list of excluded file/tab/layout categories
- one source of truth: `Ctrl + Z` for undo, `Ctrl + Y` for redo, and no `Ctrl + Shift + Z` undo/redo binding

## Phase 1. Remove non-text actions from the undo surface

Goals:

- stop presenting file/tab/layout actions as undo history
- remove the old snapshot-history surface instead of replacing it with another log

Deliverables:

- text-only undo surface
- no replacement log or secondary history surface
- no text-history rows for tab reorder, split resize, file open/save, or similar workspace actions

## Phase 2. Build the primary global text history

Goals:

- make the primary undo list a complete chronological list of text changes
- keep the document-layer operation model as the underlying text reversal mechanism

Deliverables:

- canonical text transaction ledger
- complete text-history rows for typing, paste, delete, replace-current, replace-all, and similar committed text actions
- explicit coalescing rules for typing bursts and other grouped text edits

## Phase 3. Add secondary per-file selective undo

Goals:

- allow users to undo arbitrary older changes made to one file even when those changes are interleaved globally
- avoid building a second disconnected undo stack
- support rebase/conflict handling when later edits changed the same file

Deliverables:

- per-file history lens over the canonical text ledger
- selective-undo semantics for arbitrary older file-scoped reversal
- deterministic rebase rules for translating older inverse operations over later same-file edits
- deterministic conflict handling when an older inverse operation cannot be applied cleanly
- deterministic rules for redo and branch invalidation after per-file undo actions

## Phase 3B. Add file-close history pruning

Goals:

- remove closed-file text transactions from the primary history and all filtered lenses
- ensure closing a file never leaves stale undo targets behind
- keep file close itself out of undo/redo history

Deliverables:

- file-close hook that purges or tombstones text transactions by file identity
- primary-history refresh that removes closed-file rows immediately
- per-file/source/change-set lens refresh that removes closed-file entries immediately
- redo invalidation for closed-file transactions
- tests proving closed-file entries cannot be undone, redone, filtered to, or selected

## Phase 4. Add a third text-focused lens

Recommended goal:

- add a change-set or source lens

Examples:

- search/replace changes only
- paste changes only
- multi-cursor changes only
- formatter/refactor changes only

Deliverables:

- source tagging in the canonical ledger
- filtered history view by transaction source
- product decision on which source filters are worth shipping first

## Phase 5. Polish correctness and scale

Goals:

- keep undo/redo reliable under large histories and cross-file edit patterns
- preserve selection, focus, and reveal correctly for text undo/redo

Deliverables:

- stable selection and reveal restoration
- memory/performance budgets for large text history
- clear retention policy for old history entries

## 7. Recommended Product Rules

- Only committed text mutations belong in undo/redo history.
- File operations, tab operations, and layout operations do not belong in undo/redo history.
- The primary history view is a complete global list of text changes.
- The secondary history view is a per-file selective undo lens over that same global list.
- Closing a file removes that file's text changes from all history views and undo/redo targets.
- The third history view, if added, should be text-source or change-set based, not workspace-structure based.
- Replace-current and replace-all should each commit one user-meaningful text transaction.
- Live previews must be discardable without creating undo entries.
- `Ctrl + Z` remains text undo.
- `Ctrl + Y` remains text redo.
- `Ctrl + Shift + Z` is not an undo/redo shortcut.
- A new committed text transaction clears the redo stack for the primary view.
- Reloading a buffer from disk purges that buffer's history.
- Undo history is in-memory only; it does not persist across application restart.
- File-close removes transactions outright; tombstones are not kept.

## 8. Acceptance Criteria

The undo/history system should be considered improved only when all of the following are true.

- The undo surface contains only text changes.
- The primary history is a complete list of committed text changes.
- Users can undo and redo normal text history predictably.
- Users can selectively undo arbitrary older changes to a specific file without disturbing unrelated file edits.
- Per-file selective undo either rebases cleanly over later same-file edits or fails clearly without mutating the document.
- Closing a file immediately removes that file's text transactions from primary history, per-file history, source lenses, and redo state.
- Closed-file text transactions cannot be selected or executed after close.
- Non-text file/tab/layout actions are absent from undo history.
- Replace-current and replace-all behave as clean text transactions.
- Shortcut behavior is documented and matches implementation.
- The history engine clearly uses the document/piece-tree text operation model rather than broad workspace snapshot rollback.

## 9. Validation Plan

Recommended validation coverage:

- typing burst coalescing tests
- paste/cut/delete undo-redo tests
- replace-current and replace-all undo-redo tests
- interleaved multi-file edit tests for the global text ledger
- per-file selective undo tests where arbitrary older edits to one file are dispersed through history
- per-file selective undo tests where later same-file edits require rebase
- per-file selective undo conflict tests where an older edit cannot be safely inverted
- file-close pruning tests for primary history, per-file history, source/change-set filters, and redo state
- close-and-reopen tests proving the reopened file starts with a fresh history identity unless a deliberate persistence design is added later
- redo tests after selective undo
- selection and reveal restoration tests after text undo/redo
- performance checks confirming that high-frequency text editing uses document operations rather than broad workspace snapshot cloning

Recommended implementation review questions:

- does every primary-history entry correspond to a committed text change and nothing else?
- can the per-file undo view be explained as a filtered lens over the same canonical text ledger?
- can an arbitrary older per-file undo be rebased or rejected without partial mutation?
- do closed files have zero visible/selectable undo entries in every history lens?
- are non-text actions fully excluded from undo without introducing a replacement log?