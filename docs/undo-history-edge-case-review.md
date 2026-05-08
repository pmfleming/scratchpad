# Undo History Edge-Case Implementation Plan

This plan turns the undo-history edge-case review into an implementation sequence.
It is scoped to the current per-buffer text history implementation and the global
text-history view that projects those per-buffer entries.

## Implementation Status

Implemented on 2026-05-08.

Validation completed:

- `cargo test --lib app::domain::buffer::document::tests --quiet`
- `cargo test --lib --quiet`

Notes from the implementation pass:

- Replay validation now uses the selected history index.
- Undo/redo replay and redo-branch truncation now break coalescing.
- No-op text records are filtered before history mutation.
- Per-file entry and byte budgets are both enforced.
- Span replacement now asserts span-count invariants in debug builds.
- Delete coalescing now respects hard dividers.
- The adjacent-pre-burst backspace rule is implemented for single-character
  non-divider deletes with cursor continuity.
- Imported redo state is normalized to a contiguous undone suffix; older
  non-contiguous undone entries are kept historical but made non-replayable.
- `visible_generation_before` is captured before production direct/replacement
  edit mutations and consumed when the history record is pushed.
- Follow-up review fixes were applied: per-edit no-ops are dropped from multi-edit
  records, missing generation capture is debug-asserted, budget eviction bumps
  history revision even during budget shrink, and the adjacent-pre-burst rule is
  documented as a cursor-continuity-only behavior.

Primary files:

- `src/app/domain/buffer/document/history_ops.rs`
- `src/app/domain/buffer/history.rs`
- `src/app/domain/buffer/history/coalescing.rs`
- `src/app/domain/buffer/history/budget.rs`
- `src/app/domain/buffer/document.rs`
- `src/app/text_history.rs`
- `src/app/app_state/workspace/mutation.rs`

## Current Model

Undo/redo state is still owned by each `TextDocument`.

Each document stores:

- `history: Vec<PieceHistoryEntry>`
- `next_history_id`
- `latest_history_update_at`
- `latest_operation_record`
- `revision_counter`

The global history dialog is a projection over open buffers. It collects each
buffer's entries, annotates them with `buffer_id` and label metadata, and sorts
by `global_seq`. Applying a history row routes back to exactly one document via
`TextHistoryAction { buffer_id, entry_id }`.

The implementation should keep that architecture for this pass. The goal here is
not to replace the history engine with a new ledger. The goal is to make the
existing per-document history logic correct at the edge cases where replay,
coalescing, import, and retention currently rely on fragile assumptions.

## Implementation Order

Implement these fixes in the order below. Earlier phases reduce ambiguity for
later phases and make failures easier to diagnose.

1. Make replay validation index-addressed.
2. Make undo/redo a hard coalescing boundary.
3. Filter no-op history records before they enter history.
4. Enforce both entry-count and byte budgets.
5. Harden history span replacement during compaction.
6. Improve divider-aware delete coalescing.
7. Decide and implement the adjacent-pre-burst backspace rule.
8. Normalize or harden imported redo state.
9. Capture generation-before explicitly instead of deriving it heuristically.
10. Add focused regression tests for each behavior.

## Phase 1. Index-Addressed Replay Validation

### Problem

`apply_text_history_entry` already finds the target history index, but
`validate_text_history_record` currently re-finds a matching entry by comparing
operation records:

```rust
self.history
    .iter()
    .find(|entry| self.operation_from_history_entry(entry) == *record)
```

Two different entries can have identical operation records. Validation should use
the entry being replayed, not the first record-equal entry in the document.

### Change

- Change `validate_text_history_record` to accept `index: usize`.
- Read `visible_generation_before` or `visible_generation_after` from
  `self.history[index]`.
- Keep the existing fingerprint/current-text fallback.
- Update the call in `apply_text_history_entry` from:

```rust
self.validate_text_history_record(&record, direction)?;
```

to:

```rust
self.validate_text_history_record(idx, &record, direction)?;
```

### Tests

Add a regression test that creates at least two record-equal history entries and
then replays the later one. The test should prove validation uses the selected
entry's generation metadata instead of the first equal record.

Suggested shape:

- Type `a` as an isolated entry.
- Create an intervening edit that prevents coalescing.
- Type another `a` as a separate but record-equal entry if possible.
- Undo or redo via `apply_text_history_*` targeting the later entry.
- Assert the document text and undo/redo depths are correct.

If fully record-equal entries are awkward to produce through public helpers, add a
small document-module test helper rather than weakening production code.

## Phase 2. Undo/Redo Must Break Coalescing

### Problem

New typing after undo correctly removes redo entries with:

```rust
self.history.retain(|entry| !entry.is_undone());
```

But the next edit can still coalesce into the previous surviving entry because
`latest_history_update_at` is not cleared by undo/redo replay. That can make:

```text
A B C D E
undo D E
type X
```

become:

```text
A B CX
```

instead of:

```text
A B C X
```

The redo branch is removed, but the new "now" does not always create a clean
entry boundary.

### Change

- Set `self.latest_history_update_at = None` whenever
  `apply_text_history_entry` successfully applies at least one replayed entry.
- Also clear the timestamp in `push_operation_record` when the redo branch
  truncation actually removes entries.

Recommended implementation:

```rust
let old_len = self.history.len();
self.history.retain(|entry| !entry.is_undone());
let truncated_redo = self.history.len() != old_len;
if truncated_redo {
    self.latest_history_update_at = None;
}
```

This makes both explicit replay and branch truncation establish a coalescing
boundary.

### Tests

Add tests for both paths:

- Undo one or more isolated entries, immediately type a new character, and assert
  the new character is a separate history entry.
- Undo multiple entries, type within `TEXT_HISTORY_COALESCE_WINDOW`, and assert
  the redo list is cleared while the previous surviving entry is not rewritten.
- Redo an entry, immediately type adjacent text, and assert redo replay also
  prevents accidental coalescing.

## Phase 3. Filter No-Op History Records

### Problem

`history_edit_from_operation_edit` has a `(true, true)` arm that creates an
`Inserted` history edit with a zero-length span. That allows a no-op edit to enter
history, bump `revision_counter`, and consume a budget slot.

### Change

Filter no-op records before they become history entries.

Add a helper near `push_operation_record`:

```rust
fn record_has_text_change(record: &TextDocumentOperationRecord) -> bool {
    record
        .edits
        .iter()
        .any(|edit| !edit.deleted_text.is_empty() || !edit.inserted_text.is_empty())
}
```

Then make `push_operation_record` return early for records with no text changes.

Important details:

- Do not update `latest_operation_record` for a no-op.
- Do not truncate redo for a no-op.
- Do not bump `revision_counter`.
- Do not call `enforce_history_budget`.
- Keep the `(true, true)` match arm only if needed as a defensive fallback, but it
  should be unreachable for normal production paths. Prefer `debug_assert!` in
  that arm if the type shape cannot remove it cleanly.

### Tests

Add tests proving an empty replacement:

- does not change text
- does not add an undo entry
- does not clear redo
- does not change the document history revision counter

## Phase 4. Enforce Per-File Entry Limit

### Problem

`TextHistoryBudget` exposes `per_file_entry_limit`, but
`enforce_history_budget` currently enforces only `per_file_byte_budget`.

Tiny entries can remain below the byte cap while growing `history.len()` beyond
the configured entry limit.

### Change

Update `enforce_history_budget` so it evicts oldest entries while either limit is
exceeded:

```rust
while self.history.len() > self.history_budget.per_file_entry_limit
    || bytes as u64 > self.history_budget.per_file_byte_budget
{
    ...
}
```

Preserve the existing byte accounting and eviction metrics.

If the removed entry is evicted because of entry count but has very small byte
cost, still record a per-file eviction. The byte metric can remain the removed
entry's byte cost.

### Tests

Add a test that:

- sets `per_file_entry_limit` to a small value after sanitization constraints are
  accounted for, or uses a test-only constructor/helper if needed
- inserts more isolated entries than the limit
- asserts only the newest limit-sized suffix remains
- asserts undo/redo depths reflect the retained entries

If the public budget clamps make small limits impossible, test with the minimum
sanitized value and generate entries programmatically.

## Phase 5. Harden History Span Replacement

### Problem

`replace_history_spans` silently leaves old spans in place if the replacement
span iterator runs short:

```rust
if let Some(next) = spans.next() {
    *slot = next;
}
```

`compact_add_buffer` currently preserves span count, but the invariant is not
checked at the replacement site. A future compaction change could leave stale
`ByteSpan`s pointing into old add-buffer coordinates.

### Change

Make the invariant explicit.

Recommended implementation:

- Count expected spans before replacement, or consume exactly one span per slot
  and `debug_assert!` on missing replacement spans.
- After walking all slots, `debug_assert!(spans.next().is_none())` to catch extra
  replacements.

In debug builds, a mismatch should fail loudly. In release builds, prefer not to
panic during normal editing unless the project already treats span corruption as
fatal. A reasonable release behavior is to keep current replacement semantics but
make debug builds catch invariant drift.

### Tests

Unit testing the mismatch path may require factoring the span replacement helper
so it can be called with synthetic span vectors. Add a narrow test if that can be
done without exposing internals broadly.

At minimum, add a normal compaction test proving:

- history spans remain replayable after budget eviction triggers compaction
- undo still restores text after compaction
- redo still reapplies text after compaction

## Phase 6. Divider-Aware Delete Coalescing

### Problem

`entry_sealed_by_divider` only checks the latest edit's inserted text. A
continuous backspace burst across a hard divider can become one large delete
history entry.

### Product Rule

Hard dividers should seal delete bursts as well as insert bursts.

Recommended hard dividers:

- newline
- period
- question mark
- exclamation mark

Soft dividers for deletion are optional. Keep the first pass conservative: hard
divider support is enough to prevent giant paragraph-crossing backspace entries.

### Change

Update divider sealing so it can inspect deleted text when the latest entry is a
delete-only entry.

Possible approach:

- Rename or extend `entry_sealed_by_divider` so the logic is explicit for insert
  and delete entries.
- For insert entries, keep existing behavior: hard dividers always seal; soft
  dividers seal after `TEXT_HISTORY_SOFT_DIVIDER_PAUSE`.
- For delete-only entries, seal when the accumulated deleted text contains or
  crosses a hard divider at the boundary relevant to continued backspacing.

Be careful with direction:

- Backspace prepends deleted text as the user moves left.
- Forward delete appends deleted text as the user deletes right.
- The divider check should match the side that was just crossed.

### Tests

Add tests for:

- continuous backspace within a word still coalesces
- continuous backspace across a newline creates a boundary
- continuous backspace across a period creates a boundary
- forward delete across a hard divider creates a boundary, if forward delete uses
  the same coalescing path
- soft dividers keep current behavior unless the product rule changes

## Phase 7. Adjacent Pre-Burst Backspace Rule

### Problem

`coalesce_into_inserted_text` rejects deletes outside the inserted range:

```rust
if incoming_edit.start_char < inserted_start || incoming_end > inserted_end {
    return None;
}
```

That means a backspace immediately before a typing burst does not coalesce with
the burst, even when it is part of one local correction gesture.

### Decision

Implement this only if the desired product behavior is "local correction burst"
rather than "only edits inside the current inserted payload."

Recommended product rule:

- Deleting inside the just-inserted text coalesces into the insertion entry.
- Deleting the character immediately before the just-inserted text may coalesce
  only when the incoming edit is a single-character delete, contains no inserted
  text, shares cursor continuity, and occurs within the coalescing window.
- The merged record must become a replacement-style history edit, because it now
  includes original document text in `deleted_text` plus the inserted burst.

### Change

If implementing the recommended rule:

- Extend `coalesce_into_inserted_text` for the special case:
  `incoming_edit.start_char + deleted_len == inserted_start`.
- Preserve the deleted original text in `latest_edit.deleted_text`.
- Move `latest_edit.start_char` to `incoming_edit.start_char`.
- Keep or adjust inserted text so undo restores the original deleted character
  and removes the burst as one action.
- Verify `deleted_spans` are preserved or reconstructed correctly. This is the
  risky part; do not throw away original spans if the merged edit needs them for
  replay or persistence.

### Tests

Add tests for:

- type `abc`, backspace inside burst, still one insert entry
- type after existing text, backspace the character immediately before the burst,
  undo restores the original text
- the same pattern across a hard divider does not coalesce
- multi-character deletes before the burst do not coalesce on the first pass

## Phase 8. Imported Redo State Coherence

### Problem

`replay_last_operation(OperationDirection::Redo)` picks the first undone replayable
entry. That is correct only when undone entries form a contiguous suffix. Normal
in-session behavior keeps that invariant, but persisted imports restore
`flags.undone` directly and can create non-contiguous undone patterns.

### Decision

Normalize imported history to the invariant instead of making redo support
arbitrary malformed states.

Reason:

- The current undo/redo model is linear per document.
- In a linear model, redo entries are exactly the undone suffix.
- Normalizing once at import keeps runtime replay simple and predictable.

### Change

After `restore_exported_history` imports all entries, normalize `flags.undone`:

- Scan history from newest to oldest.
- Once an applied entry is found, all older entries must be applied.
- Any older undone entries before that point should be marked applied or marked
  non-replayable.

Recommended policy:

- Preserve the newest contiguous undone suffix as redoable.
- Convert non-contiguous older undone entries to applied entries if their payload
  matches the current text model.
- If preserving them as applied is unsafe, mark them `replayable = false` and
  `undone = false` so they display as historical but cannot corrupt replay.

Choose one policy and document it in the code comment.

### Tests

Add import tests with synthetic persisted entries:

- contiguous undone suffix imports unchanged
- non-contiguous undone state is normalized
- `redo_last_operation` replays the nearest valid redo entry after import
- malformed imported flags cannot cause redo to replay an older entry before a
  newer applied entry

## Phase 9. Capture Generation-Before Explicitly

### Problem

`history_entry_from_operation` derives `visible_generation_before` by subtracting
a heuristic mutation count from the post-edit generation:

```rust
let mutation_count = ...
let generation_before = generation_after.saturating_sub(mutation_count);
```

This couples history metadata to the current internal generation bump behavior of
`PieceTreeLite`.

### Change

Capture the generation before edits are applied and carry it into history entry
creation.

Recommended approach:

- Extend the operation record or push path so `generation_before` is known when
  `push_operation_record` runs.
- In `replace_char_ranges_with_source`, capture:

```rust
let generation_before = self.piece_tree.generation().min(u32::MAX as u64) as u32;
```

before applying replacements.

- In native editor paths that call `push_edit_operation`, either:
  - include generation-before in `OperationRecord`, or
  - expose a document method that applies the edit and records before/after
    generations atomically.

Important: avoid adding another heuristic at the call site. The generation before
must be captured before mutation, not reconstructed after mutation.

### Migration Strategy

This phase may touch more call sites than the previous phases. It can be split:

1. Add `visible_generation_before_override: Option<u32>` to the internal push
   path.
2. Use the override where the caller can capture it.
3. Keep the old heuristic temporarily only for call sites that cannot yet supply
   the value.
4. Remove the heuristic once all production call sites provide the captured
   generation.

### Tests

Add tests for generation metadata around:

- pure insert
- pure delete
- replace
- multi-range replacement
- no-op filtered records
- undo/redo validation after several mixed operations

The tests should assert behavior rather than private generation numbers wherever
possible. Use private module access only if necessary.

## Phase 10. Cross-Buffer View Checks

### Problem

Most fixes are local to one document, but the text-history dialog depends on
`revision_counter`, `global_seq`, and buffer IDs for cache invalidation and row
ordering.

### Change

After the local fixes, verify the cross-buffer projection still behaves
correctly:

- Coalesced entries should still receive a new `global_seq` when rewritten.
- No-op records should not bump `revision_counter`, so the history cache should
  not rebuild for no visible history change.
- Undo/redo replay should continue to bump `revision_counter`, because row
  undone/applied state changes.
- Budget eviction should bump or otherwise invalidate the history view when
  entries disappear. If current code already calls through edit finalization, make
  that explicit in tests.

### Tests

Add or update tests for:

- timeline ordering after coalescing
- cache key changes after undo/redo
- cache key does not change after no-op push
- per-file budget eviction removes rows from the projected history

## Acceptance Criteria

The implementation is complete when all of these are true:

- Replay validation uses the selected history index, not record equality.
- Undo and redo establish a coalescing boundary.
- Typing after undo clears redo for that file and creates a new entry rather than
  rewriting the previous surviving entry.
- No-op text records do not enter history and do not clear redo.
- `per_file_entry_limit` is enforced in addition to `per_file_byte_budget`.
- History span remapping asserts span-count invariants during compaction.
- Delete coalescing respects hard divider boundaries.
- The adjacent-pre-burst backspace rule is either implemented with tests or
  explicitly left out with a documented product reason.
- Imported history cannot create non-contiguous redo behavior.
- `visible_generation_before` is captured from the real pre-edit tree generation
  on all production edit paths.
- Existing document-history tests still pass.
- New regression tests fail on the old behavior and pass after the fix.

## Validation Commands

Run the focused tests first:

```powershell
cargo test --lib app::domain::buffer::document::tests --quiet
```

Then run broader library tests:

```powershell
cargo test --lib --quiet
```

If binary harnesses require elevation on Windows, keep the required verification
to `--lib` unless the change touches binaries directly.

## Implementation Notes

- Keep this pass local to text history correctness. Do not introduce a new global
  ledger in this work.
- Prefer small helpers over broad refactors. The bugs are mostly boundary
  conditions in existing control flow.
- Preserve existing public behavior where it is already intentional, especially
  ordinary typing coalescing and normal contiguous redo.
- Add comments only around invariants that are easy to accidentally break, such
  as imported undone suffix normalization and span-count preservation.
- Treat failed replay or failed import normalization as non-mutating whenever
  possible.
