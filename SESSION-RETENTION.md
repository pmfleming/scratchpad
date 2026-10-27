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
oldest eligible archives expire. Pinned archives and quarantined data are exempt
and may exceed the budgets. Reopening transfers history into the live tab and ends
that archive's pin. Oversized unpinned items may expire immediately.

The close transaction commits the archive and remaining workspace before removing
the tab from the UI. A retryable failure keeps the live tab open. A permanently
protected/unavailable database still does not trap application exit; the visible
failure status means recovery protection is unavailable.

Use **Recently Closed** or **Reopen Last Closed Tab**, including after restarting.
The list merges archives and disk-only recent-file entries; distinct archives are
not collapsed just because their paths match. **Archived text** explicitly recovers
an older branch into untitled buffers. Ordinary file opens (including Open Here)
also consult archives when enabled; archived workspace layout takes precedence.

Reopening compares a saved decoded-text fingerprint with current disk text. An
unchanged file restores archived text/layout/undo, including unsaved edits. A changed
clean file opens from disk and leaves its old archive available. Conflicting unsaved
text and missing files recover as untitled buffers; opening the changed disk file
alongside them is configurable. Nothing writes original files automatically.

Startup conflicts likewise separate recovered text/history from current disk text
without a recurring choice dialog. Clean buffers with undo history now retain a
recovery snapshot so externally changed startup files cannot inherit an unrelated
undo stack. Legacy metadata-only histories without a verifiable baseline are
retained as non-replayable records rather than applied to arbitrary disk text.
Cold file shells with no stored snapshot still depend on their source file; loading
previews cannot archive unavailable bytes. Closing individual split views is not
workspace-tab retention.

The current database schema is **7**. Schema 6 upgrades transactionally by adding
idempotent undo-boundary snapshot tokens; existing snapshots and archives remain.
Schemas older than 6 and future schemas are rejected intact, without automatic
reset. Back up the whole database/WAL before upgrading; older binaries cannot read
schema 7. Original text files are unrelated.
The logical open-session export does not include closed archives; retain the
whole database/WAL using the documented safe backup procedure for full recovery.

Tests cover close-last/restart/undo, bounded retention and blob sharing, idempotent
retries, and transaction failure before archive commit.

## Per-buffer save points (item 2)

The first persisted recovery snapshot creates a recovery point. Subsequent changed
snapshots create another at most once every **five minutes**. Before undo pruning,
the oldest reachable earlier state is sampled independently of that interval; one
pending boundary per buffer is kept until a successful save acknowledges it.
Persisted undo payload stripping also captures an earlier boundary. Snapshot writes
and persisted history removal commit atomically; failed transactions are retryable.
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

Use **History…** in a tab context menu. Detailed edits and **Recovery points** live
in the same history window. Selecting an edit or snapshot previews text; **Restore
into this tab** creates an undoable replacement, while **Open as new tab** leaves
the source untouched. Snapshot rows show readable UTC dates and distinguish
automatic points from undo-compaction boundaries. Snapshots contain text, not
separate copies of undo history. The store validates checksums on recovery.

### Disk usage

Settings > History & Recovery > Disk usage refreshes asynchronously every five
seconds. It reports the actual database, WAL/shared-memory, other state files,
separate settings files and combined file lengths; on Unix it also reports actual
allocated file blocks. Portable/nested roots are deduplicated; original documents,
application binaries and symlink targets outside these roots are excluded.

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
