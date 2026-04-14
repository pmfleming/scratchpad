# External File Change / Stale Tab Plan

This plan covers what Scratchpad should do when a file open in a tab is older than the latest version on disk.

The most important case is:

- the user had a file open in Scratchpad
- Scratchpad was closed
- the file was edited elsewhere
- Scratchpad later restores the old in-memory/session version

It also covers the simpler live-app case where a file changes on disk while Scratchpad is still running.

## 1. Current Project Reality

Today:

- `BufferState` stores:
  - `path`
  - `is_dirty`
  - encoding/BOM metadata
- session restore can restore buffer content from:
  - a temp session buffer file
  - or the original on-disk path
- save/write behavior is centralized in `FileController`
- there is currently no explicit file-version or file-staleness model in app state
- there is currently no file watcher or reload-on-change flow

Important consequence:

- after restore, Scratchpad can reopen content that is newer or older than the current file on disk, but it has no structured way to detect or explain that mismatch

## 2. Goal

Scratchpad should detect when an open buffer is out of date relative to disk and present that clearly and safely.

We want predictable behavior for:

- clean buffers whose file changed externally
- dirty buffers whose file changed externally
- restored session buffers whose file changed while Scratchpad was closed
- deleted or moved files

We do not want silent data loss, silent overwrite, or confusing save behavior.

## 3. First-Pass Product Behavior

### 3.1 Clean buffer, file changed on disk

If the buffer is not dirty and the file changed externally:

- mark the tab as stale/out-of-date
- show a clear status/warning
- offer reload
- allow auto-reload only if we are confident the buffer has no local edits

First-pass recommended behavior:

- detect change
- auto-reload the buffer
- show an info/warning status such as:
  - `Reloaded notes.txt because it changed on disk.`

This is acceptable because the buffer is clean.

### 3.2 Dirty buffer, file changed on disk

If the buffer has unsaved local edits and the file also changed externally:

- do not auto-reload
- mark conflict/stale state clearly
- block silent overwrite on normal save
- require an explicit user choice

First-pass recommended behavior:

- keep the in-memory buffer intact
- show a warning such as:
  - `notes.txt changed on disk. Your tab has unsaved edits.`
- normal `Save` should not silently overwrite
- present a conflict dialog with choices like:
  - reload from disk
  - overwrite disk with current buffer
  - save as copy
  - cancel

### 3.3 Restored session, file changed while Scratchpad was closed

This is the key case.

If session restore loads dirty buffer content from the session store, and the underlying file on disk has changed since that snapshot:

- restore the session buffer exactly as the user left it
- mark it as stale/conflicted relative to disk
- do not silently replace the restored content
- do not silently overwrite the newer disk file on save

This preserves user edits and still tells the truth about disk state.

### 3.4 File deleted or path missing

If the restored/open buffer points at a file path that no longer exists:

- keep the buffer content open
- mark the file as missing on disk
- `Save` should behave like conflict resolution:
  - either recreate at original path
  - or require explicit `Save As` depending on chosen UX

First pass recommendation:

- keep original path remembered
- show warning
- allow explicit save to recreate

## 4. Proposed Data Model

Add explicit file freshness metadata to `BufferState`.

Recommended shape:

```rust
pub struct DiskSyncState {
    pub known_disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
}

pub struct DiskFileState {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

pub enum BufferFreshness {
    InSync,
    StaleOnDisk,
    ConflictOnDisk,
    MissingOnDisk,
}
```

Meaning:

- `InSync`: buffer matches our latest known disk version
- `StaleOnDisk`: disk changed, buffer is clean or reloadable
- `ConflictOnDisk`: disk changed and buffer has unsaved local edits
- `MissingOnDisk`: original file path no longer exists

We should keep this intentionally simple in the first pass.

## 5. Metadata Capture Strategy

### 5.1 On file open

When `FileController` opens a file:

- read file contents as today
- also capture on-disk metadata:
  - modified time
  - file length
- store that in `BufferState`

### 5.2 On save

After a successful save:

- refresh disk metadata from the written file
- mark freshness back to `InSync`

### 5.3 On session persist

Persist enough freshness context to compare restored buffers against disk later.

Recommended session fields per buffer:

- last known file modified time
- last known file length

This lets restore decide whether the on-disk file has advanced since the session snapshot.

## 6. Detection Strategy

### 6.1 Phase 1: explicit checks only

Do not start with filesystem watchers.

First pass should check for drift at safe moments:

- app startup/session restore
- tab activation
- before save
- before reload
- optionally on focus return to the app

Why this is the right first step:

- lower complexity than live watchers
- deterministic
- easier to test
- enough to solve the closed-editor stale-file case

### 6.2 Phase 2: optional watcher support

After the explicit-check flow is stable, we can add optional live file watching.

That should be a follow-up, not part of the first merge.

## 7. Restore-Time Policy

This is the most important section.

On session restore for buffers with a real file path:

1. restore the session buffer content exactly as today
2. stat the current on-disk file if it exists
3. compare current disk metadata with the persisted session metadata
4. if unchanged:
   - mark `InSync`
5. if changed and restored buffer is clean:
   - prefer reloading from disk instead of keeping stale session text
   - mark `InSync`
   - show an informational status
6. if changed and restored buffer is dirty:
   - keep restored buffer text
   - mark `ConflictOnDisk`
   - show warning
7. if file missing:
   - keep restored buffer text
   - mark `MissingOnDisk`

This gives the safest behavior for the closed-editor external-edit case.

## 8. Save Semantics

### Save when `InSync`

- save normally

### Save when `StaleOnDisk`

- if buffer is still clean, we should probably have reloaded already
- if this state survives, require a quick confirmation before overwrite

### Save when `ConflictOnDisk`

Do not silently overwrite.

Required flow:

- intercept normal `Save`
- show conflict dialog
- let the user choose:
  - overwrite disk
  - reload from disk
  - save as copy
  - cancel

### Save when `MissingOnDisk`

Allow one of:

- recreate original file
- or route to `Save As`

Recommended first-pass behavior:

- confirm recreate at original path

## 9. UI Plan

### Status bar

Show file freshness state in the status bar for the active buffer.

Examples:

- `On disk changed`
- `Disk conflict`
- `File missing`

This should be compact and persistent while the condition exists.

### Dialogs

Add explicit dialogs for:

- save conflict because disk changed
- reload confirmation when needed

### Tab/title hints

Do not overload the dirty marker alone.

If possible later, add a separate stale/conflict indicator distinct from `*`.

## 10. Integration Points

Expected file edits:

- `src/app/domain/buffer.rs`
  - add disk freshness metadata
- `src/app/services/file_service.rs`
  - add file metadata read helper
- `src/app/services/file_controller.rs`
  - capture metadata on open/save
  - intercept save when stale/conflicted
- `src/app/services/session_store/model.rs`
  - persist last-known disk metadata
- `src/app/services/session_store/mod.rs`
  - restore and compare freshness state
- `src/app/app_state.rs`
  - app-level helpers for refresh/conflict handling
- `src/app/ui/status_bar.rs`
  - show stale/conflict state
- `src/app/ui/dialogs.rs`
  - conflict/reload dialogs

Likely supporting edits:

- `tests/file_controller_tests.rs`
- `tests/session_store_tests.rs`
- `tests/app_tests.rs`

## 11. Delivery Phases

### Phase 1: disk metadata foundation

- add `DiskFileState` / `BufferFreshness`
- capture metadata on open/save
- persist metadata in session store

Definition of done:

- every path-backed buffer knows its last-known disk metadata

### Phase 2: restore-time stale detection

- compare restored buffer metadata against current disk state
- implement the closed-editor external-edit behavior

Definition of done:

- restored dirty buffers can be marked conflicted if disk moved on
- restored clean buffers can reload safely

### Phase 3: save conflict handling

- block silent overwrite for conflicted buffers
- add explicit conflict dialog

Definition of done:

- saving a conflicted file always requires an explicit decision

### Phase 4: runtime freshness checks

- check freshness on tab activation / before save / app refocus
- update status bar indicators

Definition of done:

- external edits are detected even when the app stays open

### Phase 5: optional watcher support

- add filesystem watcher if needed

## 12. Test Plan

Add tests for:

- opening a file captures disk metadata
- saving refreshes disk metadata and resets freshness to `InSync`
- restored clean buffer reloads from newer disk content
- restored dirty buffer remains open and is marked `ConflictOnDisk`
- restored path missing marks `MissingOnDisk`
- save on conflicted buffer does not silently overwrite
- split views sharing one buffer show one shared freshness state
- settings file external-change behavior remains safe

Most important explicit scenario:

- open file
- edit it in Scratchpad
- close Scratchpad with dirty session persisted
- modify the file externally
- reopen Scratchpad
- confirm restored tab keeps local session text and is marked conflicted

## 13. Risks

### Risk: false-positive stale detection from weak metadata

Mitigation:

- use both modified time and file length in first pass
- add content hash later only if needed

### Risk: silent overwrite of newer disk content

Mitigation:

- conflict state must intercept normal save
- do not treat dirty conflicted buffers as ordinary dirty buffers

### Risk: restore behavior surprises users

Mitigation:

- preserve dirty restored content
- explain conflict clearly
- prefer safety over silent reload

### Risk: settings file behaves differently from ordinary files

Mitigation:

- include settings file scenarios in tests
- keep settings refresh logic compatible with freshness state

## 14. Recommended Merge Strategy

Use small PRs:

1. disk metadata model + open/save capture
2. session persistence + restore-time comparison
3. conflict UI + save interception
4. runtime checks + status bar indicators
5. optional watcher follow-up

That keeps the highest-risk behavior change isolated to restore/save safety before adding live-refresh complexity.
