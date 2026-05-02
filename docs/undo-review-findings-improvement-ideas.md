# Undo Review Findings And Improvement Ideas

This note captures the three undo-history review findings and possible fixes. It is based on source review of the current implementation, especially `TextDocument` replay, history validation, and restore-conflict handling.

## 1. Normal Redo Replays Too Much

### Problem

`TextDocument::replay_last_operation(OperationDirection::Redo)` currently picks the newest undone entry:

```rust
self.history
    .iter()
    .rev()
    .find(|entry| entry.is_undone() && entry.flags.replayable)
```

That entry id is then passed into `apply_text_history_entry`, whose redo branch applies every undone entry from the beginning of history through the selected index:

```rust
(0..=index)
    .filter(|idx| self.history[*idx].is_undone())
```

After two normal undos, one normal redo can therefore restore both undone changes at once.

### Improvement Options

Option A: add a single-step replay path for keyboard/menu undo and redo.

- `undo_last_operation` should apply only the newest applied entry.
- `redo_last_operation` should apply only the oldest contiguous undone entry.
- The text-history dialog can keep using batch replay when the user clicks a target entry.

Option B: teach `replay_last_operation` to choose the redo boundary expected by batch replay.

- For redo, choose the oldest undone entry in the current redo run, not the newest undone entry.
- This keeps one Ctrl+Y to one operation while preserving the existing batch helper.

Option A is cleaner because it separates two concepts that currently share one API: single-step editor undo/redo and "time travel to this history entry."

### Test Coverage

Add a document-level test:

1. Start with `""`.
2. Insert `"a"`, `"b"`, `"c"` as separate non-coalesced operations.
3. Undo twice, expecting `"a"`.
4. Redo once, expecting `"ab"`, not `"abc"`.
5. Redo again, expecting `"abc"`.

Also add an app command test for `RedoActiveBufferTextOperation` if command-level tests are restored.

## 2. Batch Replay Is Not Atomic

### Problem

`apply_text_history_entry` validates and applies each entry inside the same loop:

```rust
for idx in indices {
    let record = self.operation_from_history_entry(&self.history[idx]);
    self.validate_text_history_record(&record, direction)?;
    self.apply_operation_record(&record, direction);
    self.history[idx].flags.undone = matches!(direction, OperationDirection::Undo);
}
```

If a later record in the batch fails validation, earlier records have already mutated the document and history flags. The caller receives an error, but the buffer is left partially replayed.

### Improvement Options

Option A: prevalidate every batch record before applying any mutation.

- Build `Vec<(usize, TextDocumentOperationRecord)>`.
- Validate each record against a temporary replay model or cloned document, because later validation depends on earlier replay.
- Only apply to `self` after the whole batch succeeds.

Option B: replay against a cloned `TextDocument`.

- Clone `self` into `candidate`.
- Apply the entire batch to `candidate`.
- If every step succeeds, assign `*self = candidate`.
- This is the simplest correctness-first approach. If clone cost becomes visible, replace it later with a lighter transaction object.

Option C: add rollback.

- Keep inverse records for every successfully applied step.
- If a later step fails, replay inverses before returning.
- This is more complex and easier to get wrong than candidate replay.

Option B is the best first fix: the code already relies on cloneable structures, and history replay is not expected to be on the hottest typing path.

### Test Coverage

Add a test that forces the second entry in a batch to conflict:

1. Create three history entries.
2. Manually alter visible text so the first replay still validates but the second does not.
3. Click/apply the older entry that requests a multi-entry undo or redo.
4. Assert the function returns an error.
5. Assert visible text and all `undone` flags are unchanged.

## 3. Restore Conflict Validation Does Not Check Visible Text

### Problem

`revalidate_history_for_current_text` recomputes each entry fingerprint from its stored history payloads:

```rust
let fingerprint = self.fingerprint_for_history_edits(&self.history[index].edits);
self.history[index].flags.replayable &= fingerprint == self.history[index].fingerprint;
```

That verifies the imported payloads still match their saved fingerprint. It does not verify that the current visible document is in a state where the entry can actually be undone or redone.

The result is that restored session history can stay marked replayable even when the current buffer text has diverged. The later replay path may catch the conflict, but the UI presents the entry as replayable until then.

### Improvement Options

Option A: validate each entry against current visible text using the same direction-specific checks as replay.

- For applied entries, validate undo preconditions.
- For undone entries, validate redo preconditions.
- Mark `flags.replayable = false` when validation fails.

Option B: rebuild replayability by simulating history from a known base.

- This is more rigorous but needs a reliable base text or generation/fingerprint model.
- It may be useful later for persisted history, but it is probably too much for the immediate restore-conflict fix.

Option C: be conservative after restore conflicts.

- If visible text has any uncertainty, keep the history rows but mark them non-replayable.
- This avoids stale UI promises at the cost of losing valid replay in some cases.

Option A is the most balanced near-term fix. It reuses existing validation logic and makes the history UI match actual replayability.

### Test Coverage

Add restore/session tests:

1. Persist history for a buffer.
2. Restore it onto matching text and assert entries remain replayable.
3. Restore it onto divergent text and assert conflicting entries become non-replayable.
4. Verify the text-history UI entry state reflects `replayable = false`.

## Suggested Implementation Order

1. Add focused document-level tests for redo ordering and atomic batch failure.
2. Split single-step undo/redo from text-history target replay.
3. Make batch replay transactional by applying to a candidate document first.
4. Strengthen restore-history revalidation against visible text.
5. Add session/restore tests for replayability flags.

This order should catch the highest-risk behavior first, then make the implementation easier to reason about before tightening restore semantics.
