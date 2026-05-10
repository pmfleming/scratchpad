# External File Change / Stale Tab Plan

This plan covers how Scratchpad should respond when a path-backed buffer no longer matches the underlying file on disk.

The product goal is close to VS Code's behavior:

- silently reconcile safe changes
- protect user edits when external changes would be overwritten
- keep the tab bar calm
- show fuller explanation and actions only when the active file needs attention

## 1. Current Project Reality

Scratchpad already has much of the core safety model:

- `BufferState` stores `path`, `is_dirty`, `disk_state`, and `freshness`.
- `DiskFileState` stores the last known modified time and file length.
- `BufferFreshness` has:
  - `InSync`
  - `StaleOnDisk`
  - `ConflictOnDisk`
  - `MissingOnDisk`
- opening a file captures disk state.
- saving refreshes disk state and returns the buffer to `InSync`.
- session restore compares restored buffers with current disk state.
- save blocks `StaleOnDisk`, `ConflictOnDisk`, and `MissingOnDisk` instead of silently overwriting.
- the status bar can show compact freshness labels such as `On disk changed`, `Disk conflict`, and `File missing`.
- the missing-file dialog keeps the in-memory buffer and lets the user recreate or discard it.

The main missing piece is live runtime detection. Scratchpad has background I/O polling, but no filesystem watcher, so external file changes are primarily discovered at startup, before save, before explicit reload, or during other disk checks.

## 2. Product Policy

Treat external file changes as a safety concern, not as noisy tab telemetry.

Scratchpad should use this policy:

- clean buffer plus disk changed: reload automatically and quietly
- dirty buffer plus disk changed: preserve local edits and mark conflict
- backing file deleted: preserve in-memory text and mark missing
- normal Save from conflict/missing state: block and require explicit choice
- tab bar: show only unresolved attention states
- status bar: explain the active file's disk state and provide the path to action

This is intentionally quieter than showing a persistent tab badge for every auto-reload.

## 3. State Categories

### 3.1 Auto-updated from disk

Condition:

- the buffer is clean
- the file changed externally
- Scratchpad successfully reloads from disk

Behavior:

- replace buffer content with the newer disk content
- update `disk_state`
- leave `freshness = InSync`
- show a transient status message:
  - `Reloaded notes.txt because it changed on disk.`
- optionally record a short-lived non-persistent notice for the active tab/status bar

Tab-bar signal:

- none by default
- optional future refinement: a short-lived subtle refresh icon that clears on activation or timeout

Rationale:

- once the clean buffer is reloaded, there is no unresolved risk
- VS Code mostly avoids persistent tab signals for this case

### 3.2 Conflict with disk

Condition:

- the buffer has unsaved local edits
- the file changed externally after Scratchpad's last known disk state

Behavior:

- keep the in-memory buffer intact
- set `freshness = ConflictOnDisk`
- do not auto-reload
- do not allow normal Save to silently overwrite
- show a warning status:
  - `notes.txt changed on disk. Your tab has unsaved edits.`

Tab-bar signal:

- show a compact warning icon next to the file name
- color the tab name amber
- keep the ordinary dirty marker if the buffer is dirty

Status-bar signal for active tab:

- show `Disk conflict`
- expanded/clickable explanation:
  - `File changed on disk while this tab has unsaved edits.`
- actions:
  - compare with disk
  - overwrite disk with current buffer
  - reload from disk
  - save as copy
  - cancel

### 3.3 Deleted or missing backing file

Condition:

- the buffer still has a remembered `path`
- that path no longer exists

Behavior:

- keep the in-memory buffer intact
- set `disk_state = None`
- set `freshness = MissingOnDisk`
- do not allow normal Save to silently recreate without confirmation
- show a warning status:
  - `notes.txt is missing on disk.`

Tab-bar signal:

- show a compact missing-file icon next to the file name
- color the tab name red
- keep the ordinary dirty marker if the buffer is dirty

Status-bar signal for active tab:

- show `File missing`
- expanded/clickable explanation:
  - `File is missing on disk. The in-memory buffer is still open.`
- actions:
  - recreate at original path
  - save as copy
  - discard tab
  - cancel

## 4. Tab-Bar Signaling

The tab bar should answer one question: does this tab need attention?

Recommended visual model:

- ordinary dirty state remains the existing dirty marker
- unresolved disk conflict gets an amber icon plus amber file name
- missing backing file gets a red icon plus red file name
- clean auto-reload does not get a persistent tab signal

Use icon names rather than new text labels in the tab:

- conflict: `WARNING`
- missing: `FILE_X` if available, otherwise `WARNING` with red color
- optional transient auto-reload: `ARROW_CLOCKWISE`

Do not add words such as `conflict` or `missing` directly into tab titles. The tab strip is dense, and text labels will create layout pressure.

### Priority

If more than one condition applies, show one disk-state signal with this priority:

1. `MissingOnDisk`
2. `ConflictOnDisk`
3. `StaleOnDisk`
4. transient auto-reloaded notice
5. no signal

The dirty marker is independent and can appear alongside conflict/missing.

### Multi-buffer or split-tab aggregation

If a tab can contain multiple buffers through split views, aggregate by severity:

- if any visible buffer is missing, the tab shows missing
- else if any visible buffer is conflicted, the tab shows conflict
- else if any visible buffer is stale, the tab shows stale
- else if any visible buffer recently auto-reloaded, the tab may show the optional transient notice

The active view's buffer should drive the status bar details.

## 5. Status-Bar Behavior

The status bar should provide the complete explanation for the active buffer.

For normal states, keep the current compact labels:

- `On disk changed`
- `Disk conflict`
- `File missing`

When the user hovers or clicks the status segment, show the longer explanation and available actions.

Recommended details:

- `On disk changed`: `The file changed on disk. Reload or overwrite before saving.`
- `Disk conflict`: `File changed on disk while this tab has unsaved edits.`
- `File missing`: `File is missing on disk. The in-memory buffer is still open.`

Recommended interactions:

- click conflict status: open conflict resolution dialog
- click missing status: open missing-file resolution dialog
- click stale status: offer reload or save as copy
- click transient auto-reload notice: clear the notice

## 6. Runtime Detection Strategy

### Phase 1: explicit refresh points

Keep the first implementation deterministic:

- startup/session restore
- before save
- before explicit reload
- tab activation
- app focus return

This gives most of the user benefit without introducing watcher complexity.

### Phase 2: lightweight polling

Add a modest app-level poll for path-backed open buffers:

- only poll open buffers with a `path`
- deduplicate shared paths
- use a debounce interval such as 1-2 seconds while the app is focused
- avoid repeated warnings for the same unchanged freshness state
- queue reload work through existing background I/O

This is simpler than watcher setup and portable enough for the app's current architecture.

### Phase 3: optional filesystem watcher

If polling becomes insufficient, add filesystem watcher support later.

Watcher implementation should still feed the same freshness comparison path. The watcher should not have separate behavior rules.

## 7. Freshness Comparison

Centralize disk comparison in one helper instead of scattering metadata checks.

Suggested shape:

```rust
enum DiskFreshnessEvent {
    Unchanged,
    ChangedClean { disk_state: DiskFileState },
    ChangedDirty { disk_state: DiskFileState },
    Missing,
    Inaccessible { error: String },
}
```

Inputs:

- buffer path
- buffer dirty state
- previous `disk_state`
- current `FileService::read_disk_state(path)` result

Rules:

- no path: `Unchanged`
- metadata read returns `NotFound`: `Missing`
- current disk state equals known disk state: `Unchanged`
- known disk state missing but file exists: sync and treat as `Unchanged`
- changed and buffer clean: `ChangedClean`
- changed and buffer dirty: `ChangedDirty`
- other I/O error: `Inaccessible`

The handler then decides whether to auto-reload, mark conflict, mark missing, or show an error.

## 8. Save Semantics

### Save when `InSync`

Save normally.

### Save when `StaleOnDisk`

Do not silently overwrite.

Offer:

- reload from disk
- overwrite
- save as copy
- cancel

### Save when `ConflictOnDisk`

Do not silently overwrite.

Offer:

- compare with disk
- overwrite disk with current buffer
- reload from disk
- save as copy
- cancel

### Save when `MissingOnDisk`

Do not silently recreate without confirmation.

Offer:

- recreate at original path
- save as copy
- discard tab
- cancel

## 9. Implementation Plan

### Phase 1: UI signal foundation

- add a tab-bar helper that maps buffer freshness to visual severity
- render icon plus name color in tab entries
- aggregate multi-buffer tab severity
- keep auto-reload out of persistent tab state

Definition of done:

- conflicted tabs are amber
- missing tabs are red
- dirty marker still works independently
- active status bar still shows the compact freshness label

### Phase 2: status-bar action surface

- make the freshness status segment clickable
- route conflict/missing clicks to existing pending action dialogs
- add `Save As Copy` to the missing-file dialog
- add fuller hover text for each freshness state

Definition of done:

- tab bar draws attention
- status bar explains the active file and opens resolution actions

### Phase 3: shared freshness comparison

- extract disk-state comparison into a reusable helper
- reuse it from save, restore, tab activation, and later polling
- keep session restore behavior compatible with existing tests

Definition of done:

- save and restore use one classification path
- missing, stale, and conflict states are classified consistently

### Phase 4: runtime checks without watchers

- check active path-backed buffer on tab activation
- check all open path-backed buffers when app focus returns
- optionally add a low-frequency poll while focused
- auto-reload clean changed buffers through background I/O
- mark dirty changed buffers as conflict
- mark deleted files as missing

Definition of done:

- deleting an open file marks the tab missing without waiting for Save
- external edits to clean buffers reload automatically
- external edits to dirty buffers mark conflict without losing text

### Phase 5: optional watcher support

- evaluate adding a filesystem watcher only after the explicit/poll path is stable
- watcher events should call the same comparison and handling code

## 10. Test Plan

Add or update tests for:

- tab severity mapping for `InSync`, `StaleOnDisk`, `ConflictOnDisk`, and `MissingOnDisk`
- dirty marker plus conflict/missing icon rendering state
- multi-buffer tab aggregation picks the highest severity
- status-bar freshness label and hover/action routing
- missing-file dialog includes recreate, save as copy, discard, and cancel
- clean buffer changed on disk auto-reloads and returns to `InSync`
- dirty buffer changed on disk keeps text and becomes `ConflictOnDisk`
- deleted backing file keeps text and becomes `MissingOnDisk`
- Save on `MissingOnDisk` does not recreate without explicit confirmation
- Save on `ConflictOnDisk` does not overwrite without explicit confirmation
- session restore preserves dirty text and marks conflict when disk changed

## 11. UX Guardrails

- Do not put long text in tab titles.
- Do not use color as the only signal; always pair color with an icon.
- Do not show persistent auto-update tab signals unless user testing shows they are needed.
- Do not repeatedly toast the same conflict or missing state.
- Do not auto-reload dirty buffers.
- Do not silently recreate deleted files on ordinary Save.

## 12. Recommended Merge Strategy

Use small PRs:

1. tab-bar severity helper and visual treatment
2. status-bar click/hover details and missing-file dialog improvements
3. shared freshness comparison helper
4. tab activation/app focus runtime checks
5. optional polling or watcher follow-up

This keeps the visible UX change separate from the runtime detection work, and keeps the safety behavior testable at each step.
