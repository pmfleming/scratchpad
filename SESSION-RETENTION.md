# Retained session data

## Closed tabs (item 1)

Explicit workspace-tab closes capture the final text, layout and undo/redo history
before removing the tab. The next session transaction commits the closed record
and live workspace together. Failed saves leave the pending capture retryable;
background acknowledgements retire only the captures they actually carried.
Closing the last tab therefore preserves it even though the application exits.

The newest **32 tabs** are retained in `closed_tabs`. Their snapshots/history use
the same buffer rows and content-addressed blobs as the live session. Retention
expiry removes their recovery roots and ordinary garbage collection reclaims
unreferenced content. A close is not free: unique text and undo payloads consume
space. The limit is currently fixed, not a configurable byte budget.

Use **Reopen Last Closed Tab** in a tab's context menu, including after restarting.
The archive is consumed only by a successful save of the reopened workspace.
Recovered disk-backed text is marked dirty deliberately, so external file changes
cannot silently replace the archived text or invalidate its undo history. No
reopen operation writes original files. Cold file shells with no stored snapshot
still depend on their source file; loading previews cannot archive unavailable
bytes. Closing individual split views is not workspace-tab retention.

The current database schema is **5** (closed tabs introduced schema 4; save
points introduced schema 5). Older schemas are rejected intact, with no
migration or automatic reset. To opt into a fresh schema, stop Scratchpad and
move the old state directory aside yourself. Original text files are unrelated.
The logical open-session export does not include closed archives; retain the
whole database/WAL using the documented safe backup procedure for full recovery.

Tests cover close-last/restart/undo, bounded retention and blob sharing, idempotent
retries, and transaction failure before archive commit.

## Per-buffer save points (item 2)

The first persisted recovery snapshot creates a save point. Subsequent changed
snapshots create another at most once every **five minutes**, keeping the newest
**16 per buffer**. This is sampled editing history, not every keystroke or an idle
timer: the latest text is always in the live snapshot, even between save points.
No duplicate point is created if the last point already contains identical text.

`buffer_versions` holds timestamp/length/checksum metadata; `version_parts` holds
references to existing content-addressed chunks. Creating a point copies chunk
references, not text bodies. Unchanged chunks are shared. Version roots participate
in both targeted and full garbage collection. Points expire with their buffer
when it is no longer live, retained-closed, or quarantined.

Use **Saved Versions** in a workspace-tab context menu to recover the active
buffer's point into a **new untitled tab**, leaving the current buffer and disk
files untouched. Save points contain text, not separate copies of undo history.
The public store API also lists and reads points with integrity validation.

Tests cover interval throttling, restart, bounded pruning, shared chunks, rollback
during point creation, corruption detection, and final-buffer garbage collection.

## Worker history memory (item 3)

The worker now retains only `(entry ID, BLAKE3 digest)` per undo row: 40 bytes per
entry, excluding vector/map allocation overhead. Hashing covers the complete JSON
representation, including payloads, flags, selections and changing global sequence
IDs. Serialization streams into the hasher without allocating another JSON blob.
Cache priming decodes and validates one stored history row at a time, discarding
its payload before moving to the next row. Cache changes are still accepted only
after commit and invalidated on transaction failure.

This removes the worker's persistent duplicate of every undo payload. It does not
remove the live editor history, the capture's transient exported history, or the
largest individual row during hydration. No claim is made that the historical
223 MiB whole-process peak is now a specific lower number; that workload needs a
fresh RSS measurement. A deterministic regression retains over 1 MiB of undo text
with only a 40-byte comparison entry and verifies matching hashes after reopen.
Existing coalescing, undo/redo, pruning, rollback and quarantine tests still pass.
