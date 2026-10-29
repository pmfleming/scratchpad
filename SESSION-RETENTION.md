# Retained session data

## Closed tabs (item 1)

Explicit workspace-tab closes capture the final text, layout and undo/redo history
before removing the tab. The next session transaction commits the closed record
and live workspace together. Failed saves leave the pending capture retryable;
background acknowledgements retire only the captures they actually carried.
Closing the last tab therefore preserves it even though the application exits.

Archives in `closed_tabs` use the same buffer rows and content-addressed blobs
as the live session. Settings > History & Recovery controls their logical storage
budget (default 512 MiB) and age limit (default 90 days; zero disables age expiry).
There is no fixed count limit. Under budget pressure, older detailed history is
compacted into snapshot-only recovery, then interior snapshots are thinned, then
oldest eligible archives expire. Quarantined data is exempt and may exceed the
budgets. Closed-tab pinning is no longer offered. Archives pinned by older versions
receive a one-time 30-day migration grace period, persisted in settings; afterward
normal age and storage limits apply. Protected bytes are excluded from ordinary
budgets so they cannot evict other archives or live undo-boundary points.
Oversized unprotected items may expire immediately.

The close transaction commits the archive and remaining workspace before removing
the tab from the UI. A retryable failure keeps the live tab open. A permanently
protected/unavailable database still does not trap application exit; the visible
failure status means recovery protection is unavailable.

Use **Recently Closed**, including after restarting.
One scrollable list merges archives and disk-only recent-file entries, newest
first. Old path-only settings lack closing times and appear last in their original
order. File-only duplicates are hidden when an archive covers the path; distinct
archived versions remain separate, identified by closing time.

Click a row to reopen it. A file icon indicates an existing disk file, a crossed
file indicates a missing file, and a warning icon indicates inaccessible paths.
An independent dot marks unsaved content. Hover for full paths, closing time and
recovery details. Split tabs list the status of each file in their tooltip.
Missing files without a retained copy cannot be reopened; retained tabs remain
recoverable even if their files are gone. Disk checks run outside the UI thread.
Ordinary file opens (including Open Here) also consult archives when enabled;
archived workspace layout takes precedence.

Reopening compares a saved decoded-text fingerprint with current disk text. An
unchanged file restores archived text/layout/undo, including unsaved edits. A changed
clean file opens from disk and leaves its old archive available. Conflicting unsaved
text and missing files recover as untitled buffers; opening the changed disk file
alongside them is configurable. This is decided per buffer, including mixed
splits: changed clean buffers use disk text with no archived undo attached. If any
clean file changed, the restored workspace receives new recovery identities so
saving its other buffers cannot consume the original archive. Nothing writes
original files automatically.

Startup conflicts likewise separate recovered text/history from current disk text
without a recurring choice dialog. Clean buffers with undo history now retain a
recovery snapshot so externally changed startup files cannot inherit an unrelated
undo stack. Legacy metadata-only histories without a verifiable baseline are
retained as non-replayable records rather than applied to arbitrary disk text.
Cold file shells with no stored snapshot still depend on their source file; loading
previews cannot archive unavailable bytes. Closing individual split views is not
workspace-tab retention.

The current database schema is **9**. Schemas 6–8 upgrade transactionally.
Schema 9 adds snapshot content fingerprints, edit-sequence anchors, and optional
saved-branch history with indexed byte counts. Existing snapshots gain fingerprints
on demand in the database worker. Schemas 6 and 7 also backfill stored byte counts
for undo entries and text blobs; schema 6 gains idempotent undo-boundary tokens.
Existing snapshots and archives remain. A failed upgrade rolls back without
resetting the database. Unsupported
schemas (including older than 6, future versions and unexpected application IDs)
are automatically replaced by a fresh database. The previous session, undo history,
closed-tab archives and recovery points are discarded. The `[]` status log records
successful upgrades as information and resets as warnings. Ordinary I/O, ownership
and corruption failures do not trigger a reset. Recovery export also refuses an
unsupported source rather than resetting it. Back up the whole database/WAL before
switching builds if that disposable history matters. Saved text files and settings
are unaffected.
The logical open-session export does not include closed archives; retain the
whole database/WAL using the documented safe backup procedure for full recovery.

Tests cover close-last/restart/undo, bounded retention and blob sharing, idempotent
retries, and transaction failure before archive commit.

## Per-buffer save points (item 2)

The first persisted recovery snapshot creates a recovery point. Subsequent changed
snapshots create another at most once every **five minutes**. Before undo pruning,
the oldest reachable earlier state is sampled independently of that interval; one
pending boundary per buffer is kept until a successful save acknowledges it.
Persisted undo payload stripping also captures an earlier boundary. Its computed
snapshot is cached by oldest entry, history revision, content generation and
payload budget. Successful acknowledgement retires it until that key changes;
already committed tokens do not trigger another write transaction while an
acknowledgement is in flight. Snapshot writes and persisted history removal commit
atomically; failed transactions are retryable.
Pre-existing non-replayable edits cannot be reconstructed retroactively.

There is no fixed per-buffer snapshot count. A global logical snapshot budget
(default 256 MiB) thins interior periodic states before boundary/endpoint states.
Eventually unprotected endpoints can expire too. Shared references count
conservatively toward retention budgets, not just their exclusive physical bytes.
Live recovery text is never deleted merely to meet the historical snapshot budget.
Maintenance runs on saves, and periodically while idle.

`buffer_versions` holds timestamp/length/checksum metadata; `version_parts` holds
references to existing content-addressed chunks. Creating a point copies chunk
references, not text bodies. Unchanged chunks are shared. Version roots participate
in both targeted and full garbage collection. Points expire with their buffer
when it is no longer live, retained-closed, or quarantined.

### One file-history navigator

Use **History…** in a tab context menu. There is no separate recovery-points tab or
preview window. The active file's group opens by default, including files whose
detailed undo history is empty. Selecting an applied edit undoes through it;
selecting an undone edit redoes through it in the normal editor. The up/down arrow
icons beside Delete (or Up/Down keys) move through the selected file's states.
The **Now** marker identifies the current state. Closing history keeps that state;
browsing does not save original files or create replacement undo records.

Snapshots that can be reconstructed by retained undo/redo appear as dotted,
non-selectable markers beside their matching edit boundary. Only snapshots outside
that reachable history become selectable under **Snapshots beyond undo**. Identical
text states are displayed once. When undo expires, previously covered snapshots
become selectable automatically. A stored sequence anchor is only a hint: coverage
requires successful replay and an exact decoded-text fingerprint, never timestamp
overlap. Coverage work runs off the UI thread and is reused while the document,
history and snapshot list are unchanged. Legacy snapshots without anchors are
verified against reachable states as well.

Moving into a snapshot checkpoints the current text and its native undo/redo branch
transactionally before changing the editor. These return checkpoints share text
chunks with existing snapshots and are deduplicated by content and history identity.
Ordinary periodic snapshots contain text references, not another undo log. Only a
saved return branch carries native history, bounded by the persisted-history budget;
its bytes count toward retention and undo usage. Navigating multiple older snapshots
does not generate a chain of replacement edits. Down rejoins the oldest reachable
undo state, then continues through native redo; clicking an edit returns directly to
that state. Editing an older snapshot starts a new branch without deleting the saved
return branch. Session persistence retains the selected snapshot and return identity,
so navigation can resume after restart. Original file contents remain untouched until
an explicit save.

The most recent navigation pair per buffer is protected during the run. Persisted
navigation pairs are also retention roots after restart; other saved branches follow
the normal snapshot/archive budgets. Failed loads or checkpoint writes leave the
editor unchanged. Results arriving after an edit, reload or file close cannot replace
the newer document. Content checksums and file identities are validated on restore.
All split views are refreshed when switching snapshot roots.

### Recovery & storage

Settings > History & Recovery > **Recovery & storage** combines usage and retention
controls in one card; **Undo memory** remains separate. Usage refreshes asynchronously
every five seconds. A compact summary shows combined file size, a disk-composition
bar, retained tab/snapshot counts and recovery payload size. Counts also appear
beside their corresponding limits. Policy controls remain usable while usage is
loading or unavailable; unknown counts are shown as a dash, never a false zero.

Expand **Disk breakdown** for a horizontal bar chart of database, WAL/shared-memory,
other state files and settings files. Each bar shows its share of total file size.
On Unix it also reports allocated filesystem blocks. **Recovery details** shows a
second bar chart, scaled to total recovery payloads. Both charts keep category
labels and exact sizes visible, including zero-byte entries. Reusable space and
overlapping archive references are reported separately, outside the charts.
Both sections are collapsed initially. File sizes use readable units; hover a size
for the exact byte count. The data-directory footer can copy the complete path.
Portable/nested roots are deduplicated; original documents, application binaries
and symlink targets outside these roots are excluded.

The chart shows physical file composition, not capacity or retention-budget
utilisation. The recovery payload total counts each exclusive category once;
reusable pages and overlapping archive references are excluded. Payloads are
already included in the physical files, not additional disk usage.

Usage and archive-cost queries read covering byte-count indexes, not the undo or
text blobs themselves. Closed metadata is decoded once per preparation pass and
reused for policy decisions. Preparation still walks archive metadata and size
indexes; this is not a claim of constant-time saves. Actual over-budget undo
compaction necessarily reads archived content. Damage encountered there is
quarantined durably before continuing healthy saves; if quarantine itself fails,
the store remains blocked rather than overwriting recovery data.

The stored-payload breakdown reports undo, session/archive metadata, exclusive
current/archived text, exclusive historical text, and text shared by both. Shared
text is counted once. Snapshot metadata is explicitly marked as an estimate;
archive-referenced payload totals overlap the breakdown. These are **not exclusive
filesystem allocations per feature**: one database shares chunks, indexes and pages.
The database total includes that overhead; reusable pages can remain allocated
after cleanup. Retention budgets are not a promise that the database/WAL file will
shrink to exactly the configured amount.

Tests cover interval throttling, restart, bounded pruning, shared chunks, rollback
during point creation, corruption detection, and final-buffer garbage collection.
Byte-count migration tests cover backfill, rollback and covering query plans.
The opt-in `profile_payload_lengths_against_covering_byte_counts` test compares
both accounting queries on 128 blobs totalling 64 MiB. On this development host,
three debug-build scans took 462–482 ms using `sum(length(body))`, versus
0.75–1.33 ms using the covering byte-count index. This is an isolated accounting
microbenchmark, not an end-to-end save or UI-latency claim.

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

## Incremental snapshot work (item 4)

Piece trees retain a bounded journal of the latest **128 byte-range edits**, with
unique process-local tokens that distinguish copies, replacements and undo
branches. Captured snapshots carry their own immutable journal. The worker keeps
only the last committed chunk lengths/hashes/token, not an old document copy.

For a changed snapshot, the writer maps unchanged chunk ranges through the edits,
reuses their hashes, and reads/rechunks modified ranges until FastCDC boundaries
resynchronize. An old EOF chunk cannot become an interior chunk without rescanning.
The byte reader skips unchanged tree metadata without materializing text and can
read chunk boundaries inside UTF-8 sequences safely.

Schema 6 uses a domain-separated ordered chunk-manifest checksum (chunk byte
length plus BLAKE3 body hash), rather than a flat whole-text hash that would force
another complete pass. Restore validates every emitted body, order and total
length against that checksum, including save points. New cache state is accepted
only after commit; failures discard it.

Full scanning remains the safe fallback after restart, replacement, a missing or
branched edit journal, observed damage, rollback, or for file-backed trees whose
underlying file might change outside the edit journal. Large/dispersed edits can
also require substantial rescanning. Layout/metadata capture and undo-row
fingerprinting remain separate costs; this does not make every save constant-time.

Regression tests compare incremental output against fresh full FastCDC after
mixed Unicode/front/middle/end edits, deletions, appends, undo, branching, journal
overflow, replacement and failed saves. Byte-work counters are exposed in
`SessionPersistProfile`.

Reproduce the opt-in 16 MiB local-edit probe:

```sh
cargo test --release --all-features --lib profile_local_edit_saves_against_forced_full_scan -- --ignored --nocapture
```

Nine alternating paired saves on this host compared the same schema-6 writer
with reuse enabled versus its chunk cache deliberately cleared each time. Median
worker save time was **12.457 ms full-scan → 2.039 ms incremental**. Incremental
saves examined 67–262 KiB and hashed 53–102 KiB, versus approximately 16 MiB in
both categories for each full scan. This isolates reuse, not a historical binary
comparison or a cross-platform performance guarantee. One first incremental
save was 9.946 ms. The local raw log is `/tmp/retention-stage4-profile.log`.
