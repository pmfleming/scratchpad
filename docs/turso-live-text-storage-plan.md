# Turso-backed live text storage

Status: stages 0–6 implemented on Linux, based on commit `087e045`. Native Windows
runtime validation remains pending under the user's authorization. The user
approved standard WAL on both platforms on 2026-11-05, with
multiprocess WAL to be revisited on later Turso versions. Historical source
observations below describe the baseline, not claims about later stages.

## Objective and scope

Replace the live editor's two temporary-file backings with one local Turso chunk
store shared by editing, undo, and session recovery. Keep a bounded RAM cache and
preserve exact text independently of later changes to source files.

The new database format is deliberately breaking. **Backward compatibility and
migration are out of scope.** Do not implement old-schema readers, conversion
tools, legacy chunk adapters, dual-format persistence, or automatic import of old
session databases. The existing `initialize` check already rejects an unknown
schema without modifying it, and **Start new session** remains the only way to
replace one. Only the expected versions change. Ordinary source text files remain
usable.

Backup, crash recovery, corruption handling, and database replacement still apply
to the new format. They are not migration features.

Not in scope:

- Moving small owned originals out of RAM while editing. `OriginalTextStorage::Owned`
  stays as the live form below the large-file threshold; it is chunked only for
  durable storage.
- Non-UTF-8 large files. They decode into RAM through `StagedDocument` today and
  continue to do so.
- History age or size limits. [Retention](../SESSION-RETENTION.md) stays unbounded
  until the user forgets history.

## Current behavior the design must account for

### Live text

- UTF-8 files of at least 16 MiB are copied to an unlinked temporary file, with
  one `FileBackedChunk` and one `Piece` per roughly 256 KiB. Each original has its
  own 8 MiB cache, which `set_limits` does not govern. See
  [file loading](../src/app/services/file_service/read.rs) and
  [piece-tree storage](../src/app/domain/buffer/piece_tree/storage.rs).
- Inserted text uses 256 KiB tails in a `PersistentVector`. Starting another tail
  spills the finished one, even under the RAM limits, into a process-wide file with
  an aggregate/per-owner cache. A failed spill keeps the text inline. See
  [text cache](../src/app/domain/buffer/piece_tree/text_cache.rs).
- Both caches hold their mutex during disk I/O, so a cold read or a spill can block
  any thread waiting on that cache, including paint.
- `PieceTreeText` is infallible and dereferences to `str`. A read failure panics
  (`FileBackedOriginal::load_chunk`, `ChunkText::load`). The only preflight is
  `check_history_span`, which covers add spans during history seeks.
- Ordinary edits read text on the UI thread: `slice_piece_by_chars` re-measures a
  split piece, and `insert_history_spans` reads every reattached span to rebuild
  metrics. Paint reads through `borrow_range`/`extract_range` in
  [layout](../src/app/ui/editor_content/native_editor/layout.rs).
- Search, analysis, disk save, content digests, and recovery chunking all scan
  whole documents through the same infallible span iterators and caches. See
  [write](../src/app/services/file_service/write.rs) and
  [analysis lane](../src/app/services/background_io/worker/analysis_lane.rs).
- `compact_add_buffer` has no production caller. Add offsets are stable in practice.
- Add storage can diverge between clones, which share chunks but append separately.
  `pending_reopen_original` keeps such a clone and can later be restored.
  `owner` is inherited by clones, so it is a budget key, not a lineage identifier.

### Persistence

- Dirty sessions save about once per second (`SESSION_SNAPSHOT_INTERVAL`); clean
  sessions save once per minute. Each commit uses `synchronous=FULL`.
- **Every save materializes all live history text.** `Prepared::new` calls
  `history::fingerprints`, which exports each entry through `tree.text_for_span(..)
  .to_string()` and hashes it, including unchanged entries.
- **A large file is stored several times.** `record_initial_content` creates an
  "Initial contents" entry, so the whole file is serialized into a single
  `history.entry` JSON value, on top of the temporary copy and the FastCDC
  `blobs`/`parts` snapshot. Restore then copies that text into a new add buffer.
- `IncrementalChunks` disables chunk reuse when `is_file_backed()` is true, so one
  edit in a large file re-reads and re-hashes the whole document. That read, and
  all history rows, happen inside `BEGIN IMMEDIATE` (`write_prepared`).
- FastCDC boundaries (16/64/256 KiB) can split UTF-8 and do not line up with live
  chunks. See [chunking](../src/app/services/session_store/database/chunking.rs).
- History character offsets and lengths are `u32`. Past 4 Gi characters they wrap
  (`initial_file_history`, `record_initial_content`) or saturate
  (`history_edit_from_operation_edit`, `char_len_u32`), silently in both cases.
- `history.entry` is a JSON copy of the row. Six of its fields repeat `history` columns, and
  `history_entry` checks that they still match on every read. `current` and `selected_tip` repeat
  `document_history`. Because `current` and `undone` live in the JSON, undo and redo change the
  serialized value of every entry they cross, and each of those rows is rewritten.
- Restore reads the snapshot into `StoredSession.contents` as complete `String`s,
  re-hashes every history payload in `import_history_entry`, then replays from the
  root in `finish_history_import`. `Restorer` assumes it holds owned text.

### Other stored JSON

- `documents.payload` repeats `name`, `path`, `observed_digest`, and `observed_missing` from its own
  columns. `read_document` reads only the JSON and doesn't compare them, so a mismatch goes unnoticed.
- `encoding` and `has_bom` duplicate `format`, and exist only for a fallback in
  `session_buffer_format_and_metadata` when `format` is missing.
- `Database::open_owned` parses the JSON of every document and every live and closed layout to
  reserve the highest buffer and view IDs. `usage.rs` also reads inside `closed_tabs.payload`.
- `tabs.payload` and `closed_tabs.payload` hold the same structure. Their buffer lists repeat
  `tab_documents` and `closed_documents`, and `validate_document_refs` checks that they match.
- Any change to a tab rewrites its JSON and deletes and reinserts its index rows. Closing the first
  tab shifts every later position, so all later tabs are rewritten, and that save runs on the UI
  thread through `persist_tab_close`.
- `read_stored_tab` runs one query per document.
- `session_state.payload` holds `font_size` and `word_wrap` only for `adopt_legacy_startup_settings`,
  which copies them into settings when no settings file exists. That is a migration path.
- `session_buffer_format_and_metadata` re-inspects the whole text during restore to rebuild
  `BufferTextMetadata`.

### Traps that would break a naive chunk table

Each of these is existing code that treats unknown rows or writes as damage, or
deletes them. All must change with the schema.

| Code | What would happen |
| --- | --- |
| `read_manifest` | Any `blobs` row without a `session_state` row returns "missing session commit marker". A crash during the first large import would block the next startup. |
| `delete_stale_content` with `full_cleanup` | `DELETE FROM blobs WHERE digest NOT IN (SELECT digest FROM parts)` runs on the first save after opening and would delete live-only and history-only chunks. |
| `refresh_inventory` | Compares `total_changes()`; a chunk write on the worker connection looks like an external write, drops the inventory, and forces that full cleanup. |
| `finish_transaction` | A failed commit is treated as ambiguous and clears the comparison caches. Pending chunk state must follow the same rule. |
| `ContentInventory`, `read_all_contents`, `validate_required` | Their integrity checks assume every text body is reached through `snapshots`/`parts`. |

### Threads and lifecycle

- [`DatabaseWorker`](../src/app/services/session_store/database/worker.rs) runs FIFO
  boxed jobs on a `sync_channel(2)`; `call` blocks the caller. `writes_blocked()`
  uses `call` and runs on the UI thread (`queue.rs`, `open.rs`, `session_manager.rs`),
  as do `persist_tab_close`, `persist_view_close`, and `persist_session_now`. A long
  worker job therefore already stalls the UI at those points.
- `read_content_parallel` proves the app already opens extra connections from
  other threads, but only for a bulk read, and those connections do not set
  `cache_size`.
- `PrimaryInstance::elect` plus the `session.db.lock` file lock give a single owning
  process. No other process can hold live references.
- **Editing does not currently depend on database health.** With a failed open, an
  incompatible schema, or the `blocked` latch, saving pauses but large files still
  open and text still spills. See [session reset](../src/app/services/session_manager/reset.rs).
- `FileService::read_file` has no store handle. It runs on the path lane, and also
  for `read_observed_file` and restore reconciliation, where the document may be
  hashed and discarded. Domain tests, benches, and many parallel `SessionStore`s
  create documents with no store at all.
- [Reset](../src/app/services/session_store/database/reset.rs) closes handles,
  copies the file set to a temporary backup, removes the originals, and opens a new
  database at the same path. Open reader connections would block removal on Windows.

### Pinned engine (`turso_core` 0.8.3 source)

To be confirmed by the contract probe in stage 0:

- New connections have `BusyHandler::None`, and the app never sets a timeout. A
  second connection attempting `BEGIN IMMEDIATE` during a save should fail at once.
- Auto-checkpoint runs after a commit past a fixed 1000-frame threshold
  (`storage/wal.rs`), about 4 MiB at 4 KiB pages. No pragma for changing it was found.
- `synchronous` and `cache_size` are per-connection settings.
- The final connection's close triggers a shutdown checkpoint, so reader connections
  affect close behavior.
- The engine is pre-1.0 and contains panics. A panic on the paint thread would take
  down the window, whereas the worker thread contains one today.

The rejected experimental `Builder::experimental_multiprocess_wal(true)` mode has
the following properties in the pinned source. These are reference notes for a
future reassessment; the application does not enable this mode on either platform:

- WAL coordination moves to a memory-mapped `session.db-tshm` file, and the database file opens
  with `NoLock`. Reader slots, the writer, and the checkpointer are all tracked in that mapping.
- Open **fails** with `InvalidArgument` when the backend or filesystem isn't supported.
  - **Linux:** `IoBackend::Syscall` uses `UnixIO`, which is supported. NFS, CIFS/SMB, AFS, Ceph,
    GFS2, Lustre, OCFS2, 9p, NCP, and Coda are rejected by `filesystem_magic_allows_shared_wal`.
  - **Windows:** `IoBackend::Syscall` uses `PlatformIO`, and `io/windows.rs` doesn't implement
    `supports_shared_wal_coordination`, so enabling the option there would make the session
    database fail to open. It therefore stays off on Windows.
- While a database is open in one mode, opening it in the other is rejected, so every opener needs
  the same setting. That includes the contract probe.
- The shared frame index holds at most 262,144 entries per WAL generation (about 1 GiB at 4 KiB
  pages). In the stage 0 probe it returns persistent Busy at overflow; the live
  read path refuses a blocking fallback scan. Do not assume recovery works.
- Reclaiming a dead process's slots is documented as best-effort, so a slot left by a crash might
  hold back checkpoints.
- A close only runs the shutdown checkpoint when this is the last process mapping the file.

## Design

### Components

```text
piece tree / history         logical (buffer, byte offset) spans, unchanged
  └─ PieceTreeStorage        Option<ChunkStoreHandle>, tails, sealed chunk handles
       └─ ChunkStore         pending set, cache, readers; no SQL in domain code
            ├─ writer        existing session-database worker
            └─ readers       foreground and background threads, own connections
```

Keep logical offsets. Pieces and `ByteSpan`s do not change, so history, anchors,
and search keep their current addressing. `compact_add_buffer`, `take_add_if_nonempty`,
`replace_add`, and `rewrite_add_spans` are deleted first, making add offsets stable.

Domain storage holds an explicit `Option<ChunkStoreHandle>`, inherited by clones
and bound when a document joins a workspace or restores. Path-load requests carry
the handle for imports. A process-global slot, the pattern `text_cache` uses, is
rejected because tests and profile harnesses run many stores in one process. An
unbound document keeps sealed chunks in RAM, which also keeps domain tests hermetic.

### Chunk states

```text
Tail(Arc<String>)      mutable by copy-on-write; owned by one tree revision
Sealed(Arc<Chunk>)     immutable; byte_len, char_len, and
    Pending            body in RAM, queued for the writer
    Committed(row)     body in the cache only; evictable
    Failed(cause)      body retained in RAM, reported
```

- **Sealing happens only at rollover and at import**, never at capture or save.
  Sealing moves the `Arc<String>` to the pending set. Hashing runs on the writer,
  so the UI thread does no BLAKE3 work.
- A chunk becomes evictable only after its row commits. A failed or ambiguous commit
  leaves it pending. `INSERT .. ON CONFLICT(digest) DO NOTHING` followed by a row
  lookup makes the retry idempotent.
- `Chunk` handles are shared by captures, snapshots, and clones through
  `PersistentVector`, so a state change is visible to every holder.
- The cache is keyed per chunk and replaces both existing caches, under the one
  `set_limits` budget. Originals become subject to the per-file limit, which they
  are not today. Check cold-scroll results against that limit in stage 3.

### Saving once per second

Hashing and rewriting a growing tail each second would write up to 256 KiB per
save. Sealing at each save would instead produce a stream of tiny chunks. Use
**append-only tail segments**:

- Each save writes only the tail bytes since the last save to `document_tail`.
  Segments are valid UTF-8 because appended text is.
- At rollover, one transaction publishes the chunk mapping and deletes the segments
  it covers. Write volume stays close to the bytes typed.
- The worker's comparison cache keeps the persisted tail length and a BLAKE3 hash of
  that prefix. A mismatch, such as a restored divergent clone, replaces the segments.
- `document_text` is compared with the captured chunk list by position and replaced
  from the first difference. It is therefore a per-commit value rather than an
  assumed append-only log, which also handles Forget history and divergent clones.

### Schema

Bump `user_version` to 12 and `SESSION_VERSION` to 10, including the hard-coded
"expected version 11" in `schema_reset.rs`. Remove `snapshots`, `blobs`, `parts`, `tab_documents`, and
`closed_documents`. The only JSON left in the database is `quarantine.payload` and `layouts.root_pane`.
The tables for documents, session state, and layouts are under
[Documents, session state, and layouts](#documents-session-state-and-layouts).

```sql
CREATE TABLE chunks(id INTEGER PRIMARY KEY, digest BLOB NOT NULL UNIQUE,
    byte_len INTEGER NOT NULL, char_len INTEGER NOT NULL,
    newline_count INTEGER NOT NULL, is_ascii INTEGER NOT NULL) STRICT;
CREATE TABLE chunk_bodies(chunk_id INTEGER PRIMARY KEY REFERENCES chunks(id),
    body BLOB NOT NULL) STRICT;
CREATE TABLE document_text(document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    buffer INTEGER NOT NULL CHECK(buffer IN (0,1)), start_byte INTEGER NOT NULL,
    chunk_id INTEGER NOT NULL REFERENCES chunks(id),
    PRIMARY KEY(document_id,buffer,start_byte)) STRICT;
CREATE INDEX document_text_chunk ON document_text(chunk_id);
CREATE TABLE document_tail(document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    start_byte INTEGER NOT NULL, body BLOB NOT NULL,
    PRIMARY KEY(document_id,start_byte)) STRICT;
CREATE TABLE document_pieces(document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    part INTEGER NOT NULL, digest BLOB NOT NULL, body BLOB NOT NULL,
    PRIMARY KEY(document_id,part)) STRICT;
CREATE TABLE history(id INTEGER PRIMARY KEY,
    temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL, parent_id INTEGER NOT NULL CHECK(parent_id>=0 AND parent_id<seq),
    edited_at INTEGER NOT NULL, global_seq INTEGER NOT NULL CHECK(global_seq>0),
    source TEXT NOT NULL CHECK(source IN ('load','edit','paste','cut','search_replace','external')),
    summary TEXT NOT NULL, bytes INTEGER NOT NULL CHECK(bytes>=0),
    replayable INTEGER NOT NULL CHECK(replayable IN (0,1)),
    previous_primary INTEGER NOT NULL, previous_primary_next INTEGER NOT NULL,
    previous_secondary INTEGER NOT NULL, previous_secondary_next INTEGER NOT NULL,
    next_primary INTEGER NOT NULL, next_primary_next INTEGER NOT NULL,
    next_secondary INTEGER NOT NULL, next_secondary_next INTEGER NOT NULL,
    UNIQUE(temp_id,seq),
    FOREIGN KEY(temp_id,seq) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE,
    FOREIGN KEY(temp_id,parent_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE) STRICT;
CREATE TABLE history_spans(entry_id INTEGER NOT NULL REFERENCES history(id) ON DELETE CASCADE,
    edit INTEGER NOT NULL CHECK(edit>=0), part INTEGER NOT NULL CHECK(part>=0),
    start_char INTEGER NOT NULL CHECK(start_char>=0),
    role INTEGER NOT NULL CHECK(role IN (0,1)), buffer INTEGER NOT NULL CHECK(buffer IN (0,1)),
    start_byte INTEGER NOT NULL CHECK(start_byte>=0), byte_len INTEGER NOT NULL CHECK(byte_len>=0),
    char_len INTEGER NOT NULL CHECK(char_len>=0),
    PRIMARY KEY(entry_id,edit,part)) STRICT;
```

`history_nodes`, `document_history`, and the existing `history` indexes stay. The selection columns
are abbreviated in the sketch, and `*_next` holds each cursor's `prefer_next_row`.

- Bodies are separate from metadata so scans never touch overflow pages. The current
  `INDEXED BY blob_sizes` queries show this already matters.
- **History has no JSON.** See [History rows](#history-rows).
- **`document_text` is the only text root.** Every stored span resolves through its
  document's mapping, so reachability needs only one table.
  Closed and quarantined documents stay rooted because they keep their `documents` rows.
- `document_pieces` is the encoded piece list at Now, with the `Piece` metrics, in
  bounded parts rewritten only when changed. Add a digest per part.
- `PersistedHistoryEntry` remains for recovery export only, the one payload-bearing
  format. It is not imported automatically. Remove or confine the other `text_history` paths
  (`HistoryRows::Exported`, `model.rs`, `restore.rs`, `buffer_content.rs`).

Add `Chunk` metadata to `Piece` construction where a span covers a whole chunk, so
undo of a large deletion need not read the text just to rebuild metrics.

### History rows

Remove `history.entry`. Each entry becomes one typed `history` row plus one `history_spans` row per
referenced span, so the database enforces what `history_entry` and `validate_entry` check by hand today.

| Field in today's JSON | New home |
| --- | --- |
| `id`, `parent_id`, `edited_at`, `global_seq`, `source`, `summary` | Existing `history` columns, with no second copy |
| `current`, `selected_tip` | `document_history.now_id` and `tip_id` only |
| `flags.undone` | Not stored. `refresh_tree_flags` derives it from the route on load |
| `flags.replayable` | `replayable` column |
| `flags.persisted` | Removed from the domain type. It is only ever set to false, and only a test reads it |
| `visible_generation_before`, `visible_generation_after` | Not stored, and removed from the domain type if the audit agrees, since only tests read them |
| `fingerprint` | Not stored. Chunk digests provide integrity |
| `previous_selection`, `next_selection` | Eight typed columns |
| `edits` | `history_spans` |

**Spans.** An edit is identified by `edit`, and its spans are ordered by `part`. `role` is 0 for deleted
text and 1 for inserted text, and the edit's kind follows from which roles it has. `char_len` lets restore
validate lengths without reading text. Once `PieceHistoryEdit` carries it too, undo can also remove the
right range without a read.

**What this gains.**

- **Undo and redo update one row.** Only `document_history` changes, however many entries they cross.
- **Less to maintain.** The JSON-to-column consistency checks and their tests go away, and
  damage tests inject bad rows instead of `entry=x'00'`.
- **Change detection needs no serialization.** It hashes typed fields directly and excludes
  derived state, replacing the serialized hash in `history::fingerprint`.
- **SQL can answer questions directly.** A span past its document's mapped length, for example,
  is a query rather than a decode of every entry.

**Rules.**

- `bytes` is redefined as the text bytes an entry references, computed when it's written. That keeps the
  `history_sizes` covering index, so the Settings usage query, which runs every 5 seconds, needn't scan spans.
- Keep `history.id` stable across updates. A coalesced entry updates its row with an upsert, deletes its
  spans, and reinserts them. `INSERT OR REPLACE` would change the id.
- Read one document's entries and spans with one query each and merge them in order. Never run a
  query per entry.
- All offsets go through `i64::try_from` on write, as the current code does for `seq`.
- A very large entry, such as replace-all, becomes many rows in the manifest transaction. Stage 0
  measures this. If rows cost too much, packing an entry's spans into a fixed-layout `BLOB` column is
  the fallback. Either way the entry stays typed and JSON doesn't return.
- `PersistedHistoryEntry` keeps its JSON form for recovery export, which writes it from these rows.

### Documents, session state, and layouts

Stored metadata moves from JSON to typed rows. Live and closed tabs share one set of layout tables.

```sql
CREATE TABLE session_state(id INTEGER PRIMARY KEY CHECK(id=1),
    tab_count INTEGER NOT NULL CHECK(tab_count>=0),
    active_tab_index INTEGER NOT NULL CHECK(active_tab_index>=0),
    active_surface TEXT NOT NULL CHECK(active_surface IN ('workspace','settings'))) STRICT;
CREATE TABLE documents(id TEXT PRIMARY KEY, buffer_id INTEGER NOT NULL,
    name TEXT NOT NULL, path TEXT, path_key TEXT, search_text TEXT NOT NULL,
    is_dirty INTEGER NOT NULL CHECK(is_dirty IN (0,1)),
    text_is_empty INTEGER NOT NULL CHECK(text_is_empty IN (0,1)),
    has_recorded_edits INTEGER NOT NULL CHECK(has_recorded_edits IN (0,1)),
    is_settings_file INTEGER NOT NULL CHECK(is_settings_file IN (0,1)),
    show_control_chars INTEGER NOT NULL CHECK(show_control_chars IN (0,1)),
    right_to_left INTEGER NOT NULL CHECK(right_to_left IN (0,1)),
    encoding_name TEXT NOT NULL, has_bom INTEGER NOT NULL CHECK(has_bom IN (0,1)),
    encoding_source TEXT NOT NULL, line_endings TEXT NOT NULL, preferred_line_ending TEXT NOT NULL,
    lf_count INTEGER NOT NULL, crlf_count INTEGER NOT NULL, cr_count INTEGER NOT NULL,
    is_ascii_subset INTEGER NOT NULL CHECK(is_ascii_subset IN (0,1)),
    has_decoding_warnings INTEGER NOT NULL CHECK(has_decoding_warnings IN (0,1)),
    line_count INTEGER NOT NULL CHECK(line_count>=0),
    disk_modified_millis INTEGER, disk_len INTEGER, disk_content_digest TEXT,
    observed_digest TEXT, observed_missing INTEGER NOT NULL CHECK(observed_missing IN (0,1))) STRICT;
CREATE INDEX documents_path ON documents(path_key);
CREATE INDEX documents_buffer ON documents(buffer_id);
CREATE TABLE layouts(id INTEGER PRIMARY KEY, active_view_id INTEGER NOT NULL,
    root_pane TEXT NOT NULL) STRICT;
CREATE TABLE tabs(position INTEGER PRIMARY KEY CHECK(position>=0),
    layout_id INTEGER NOT NULL UNIQUE REFERENCES layouts(id)) STRICT;
CREATE TABLE closed_tabs(sequence INTEGER PRIMARY KEY, token TEXT NOT NULL UNIQUE,
    closed_at INTEGER NOT NULL, layout_id INTEGER NOT NULL UNIQUE REFERENCES layouts(id)) STRICT;
CREATE TABLE layout_documents(layout_id INTEGER NOT NULL REFERENCES layouts(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL, buffer_id INTEGER NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    PRIMARY KEY(layout_id,ordinal), UNIQUE(layout_id,document_id), UNIQUE(layout_id,buffer_id)) STRICT;
CREATE INDEX layout_document_lookup ON layout_documents(document_id,layout_id);
CREATE TABLE layout_views(layout_id INTEGER NOT NULL REFERENCES layouts(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL, id INTEGER NOT NULL, buffer_id INTEGER NOT NULL,
    show_line_numbers INTEGER NOT NULL CHECK(show_line_numbers IN (0,1)), zoom_factor REAL NOT NULL,
    primary_index INTEGER, primary_next INTEGER, secondary_index INTEGER, secondary_next INTEGER,
    CHECK((primary_index IS NULL)=(primary_next IS NULL)
      AND (primary_index IS NULL)=(secondary_index IS NULL)
      AND (primary_index IS NULL)=(secondary_next IS NULL)),
    PRIMARY KEY(layout_id,ordinal), UNIQUE(layout_id,id)) STRICT;
CREATE INDEX layout_views_id ON layout_views(id);
```

**What stays JSON.**

- `layouts.root_pane`: it's recursive, always read whole, and never queried. `validate_pane` has to
  check its shape regardless, so rows would add reassembly code with no benefit.
- `quarantine.payload`: a frozen copy of damaged metadata whose bytes are never updated. It must stay
  readable on its own even when the normal tables it protects are damaged.
- Recovery export's `session.json`. Serde derives on the session model stay for this and for quarantine.
- `json_each(?)` query parameters, which pass ID lists into a query and aren't stored data.

**Layout identity.**

- `WorkspaceTab` gains a stable `layout_id`, because today a tab has no identity beyond its position.
  It is allocated from a counter when a tab is created, carried through capture and `SessionTabParts`, and
  restored from the row. IDs are reserved at open the same way as view IDs.
- A layout belongs to exactly one `tabs` or `closed_tabs` row. Each table enforces one owner on its own
  side, and load validates across both. A layout left with no owner is deleted in the same transaction.
- Closing a whole tab moves its layout to `closed_tabs` without rewriting it. Closing one view creates
  a new layout for the closed document, because `close_view` builds a new `WorkspaceTab` for it.
- A reopened archive that a live tab consumes deletes the archive row and its layout, unless the live tab
  keeps that layout.

**Writes.**

- The worker compares typed rows, replacing the serialized bytes in `document_cache`, `tab_cache`, and
  `state_cache`. Only rows that changed are written. A cursor move updates one `layout_views` row.
- Moving tabs only changes `tabs`. `layout_id` is `UNIQUE`, so a shift can't update rows in place in
  position order. Delete the affected positions first, then insert them.
- `release_retained_path` becomes one `UPDATE` plus the empty-text case it already handles.
- Rows for quarantined documents are never written, matching `freeze_quarantined` today.
- `path` becomes authoritative, so it must never pass through `to_string_lossy`. A non-UTF-8 path keeps
  producing the save error that serde gives it today, rather than storing an altered path. Fixing
  that limit is separate work.
- `active_surface`, `source`, and the format enums are stored with their serde names.

**Reads.**

- Read tabs, layouts, documents, and views with one query per table and merge them in Rust, never one
  query per document. `ReadScope::Active` filters by position.
- Saves still don't read unrelated closed layouts, the rule `warm_saves_do_not_read_unrelated_closed_layout_payloads`
  checks today.
- `max(id)` queries on `documents`, `layout_documents`, `layout_views`, and `layouts` replace the startup JSON
  scans in `Database::open_owned`.
- `read_manifest` keeps the `session_state` row as the commit marker. Its emptiness check lists the new
  metadata tables and excludes `chunks` and `chunk_bodies`, so a crash during a first import isn't damage.
- `retained_tab_by_path` and `prepare_closed` use `layout_document_lookup`. `usage.rs` joins `layout_documents`
  instead of reading JSON.
- `session_metadata` and `archive_metadata` in `usage.rs` were payload lengths. They become
  estimates from row counts plus `root_pane` length, and the labels in
  [Settings storage](../src/app/ui/settings/recovery/storage.rs) must say so.

**Removals, since nothing needs old data.**

- `SessionBuffer.encoding`, `has_bom`, and the `Option` on `format`, with the detection fallback in
  `session_buffer_format_and_metadata`.
- `SessionManifest.font_size`, `word_wrap`, and `legacy_settings`, along with
  `adopt_legacy_startup_settings`, `RestoredSession.legacy_settings`, and the same field on
  `SessionRestoreStarted`. After this, deleting the settings file resets font size and word wrap to
  defaults instead of recovering them from the session.
- `StoredTab`, `DocumentRef`, `validate_document_refs`, and the `version` value. `SESSION_VERSION` remains for
  recovery export.
- `#[serde(default)]` fields kept only for older payloads in the types quarantine and export serialize.

### Chunk boundaries

Add-log chunks are cut at rollover with no CDC: they are
append-only, so edits never shift them and CDC has nothing to resynchronize. Imports
use UTF-8-adjusted FastCDC (16/64/256 KiB). The stage 0 edited-version fixture
needed approximately 114 KiB of new unique CDC chunks versus 5.8 MiB of fixed
chunks, justifying CDC for adopted/reopened originals. Add-log rollover remains
fixed, since add offsets never shift. Remove `IncrementalChunks`, `chunk_cache`, the `fastcdc`
dependency, and `save_changes` if nothing else uses them.

### Commit protocol

1. Capture stays O(tabs) on the UI thread with no sealing or hashing.
2. The worker commits every pending chunk the capture reaches, in bounded separate
   transactions before the manifest transaction. Crashing here leaves unreferenced
   rows, which the startup sweep removes.
3. Outside any transaction, encode changed pieces, history rows, and tail suffixes.
   Skip unchanged documents and entries by history revision rather than fingerprinting
   every entry.
4. `BEGIN IMMEDIATE`, then publish mappings, tail segments, pieces, history, layout,
   closed rows, and activity. Commit with `synchronous=FULL`.
5. Update caches after commit only, and queue released chunks for collection.

No text reads occur inside the manifest transaction. **A committed chunk is not a
committed editor state:** recovery only exposes what step 4 published. Chunk-only
transactions use `synchronous=NORMAL`; manifest transactions remain FULL. Stage 0
verified NORMAL-to-FULL process-exit recovery across automatic checkpoint
thresholds. Inspection of the pinned pager confirms that FULL sync waits for WAL
writes, and NORMAL checkpoints sync the WAL before backfill and the database
before WAL truncation. This is ordering evidence plus crash testing, not a
physical power-loss test. Re-run that contract on every engine upgrade.

### Standard WAL on both platforms

Linux and Windows both keep standard WAL with `IoBackend::Syscall`. Do not enable
`.experimental_multiprocess_wal(true)` in any application opener. Reader threads
use separate connections in the owning process; the experimental mode is not
needed for this concurrency. Keep `session.db.lock`, `PrimaryInstance::elect`,
and the single writer. Every application opener must use the same standard mode.

- Reader SQL transactions end after each bounded chunk read. Keep immutable chunk
  handles and pinned `Arc<String>` text in memory, without retaining an SQL
  snapshot throughout search, export, paint, or undo. This permits checkpoint
  progress without changing the captured document's contents.
- Standard WAL needs no multiprocess-filesystem capability detection or mode
  fallback. Do not add that plumbing, `.tshm` accounting, or a Linux-only mode.
- Reset and shutdown still close all readers before replacing files or running
  the writer's final checkpoint. The existing ownership lock remains essential
  to collection and live-reference correctness.
- **Revisit multiprocess WAL on later Turso versions**, rather than treating the
  0.8.3 rejection as permanent. Preserve the optional Linux contract and overflow
  reproducer. Before considering adoption, verify overflow, long-held readers,
  abrupt exit with active slots, reopen, checkpoint/TRUNCATE, reader deadlines,
  filesystem support, reset sidecars, and native Windows behavior on the candidate
  version. Update both lockfiles and the engine contract together. Do not enable
  the option simply because an engine upgrade compiles or routine tests pass.
- The failed 0.8.3 experimental contract remains historical evidence, not a
  required passing gate for the standard-WAL implementation.

### Writer

The existing worker is the only writer. A second write connection would contend for
the lock with no busy handler, and offers nothing a single writer lacks.

- Pending chunks live in a shared structure rather than job payloads. The UI thread
  never blocks on `sync_channel`; it wakes the worker with a non-blocking signal.
- The worker alternates queued jobs with bounded chunk batches, so a save waits for at
  most one batch. Stage 0 selects at most four 256 KiB chunks or 4 ms of batching work,
  whichever is reached first. Stop taking chunks at that boundary and service
  queued work. 50 ms is the user-approved scheduling target, not a hard bound
  on an in-flight kernel I/O or engine checkpoint (observed overruns reached
  approximately 92 ms). Record overruns and validate UI responsiveness; never
  wait for the writer on the paint/edit path. Keep the WAL near its checkpoint
  threshold and apply backpressure to bulk intake.
- `blocked` becomes readable without the queue, so `writes_blocked()` stops stalling
  the UI thread on every save.
- Chunk writes update the inventory's expected `total_changes()`, or that check is
  removed in favor of explicit invalidation.

### Reads

Reads run on **dedicated reader threads, not the calling thread**. That contains an
engine panic as an error, permits a deadline, and lets a queue distinguish priorities.
Stage 0 measures the added hop against a cold read.

- A foreground reader serves paint and edit requests. A background reader serves
  prefetch and scans, so a search can never queue ahead of paint.
- Connections are opened at store startup, because `connect` runs a stats refresh, and
  set a small `cache_size`, because the chunk cache already holds the bodies.
- Reads hold no cache mutex during I/O. Duplicate loads coalesce per chunk.
- Each load verifies length, BLAKE3 digest, and UTF-8 before entering the cache.
- `Busy` from a reader is retried within a deadline, then reported. The probe determines
  how often this happens during checkpoints.
- The worker's own reads go to the readers as well. Nothing depends on its job queue,
  which removes the reentrant deadlock.

**Fallibility: preflight and pin.** Rewriting 32 files of `str`-based callers is not
needed, and is not done. Extend the existing `check_history_span` pattern instead:

- Each operation acquires a `TextPin` for the ranges it will touch, using a fallible
  call that returns `io::Result`. Pinned `Arc<String>`s make the infallible reads inside
  it safe. If the pin fails, the operation aborts before changing anything and reports
  an error.
- Edits, history navigation, copy, and replace pin before mutating.
- Scanners (search, analysis, disk save, digests, export) get a fallible span iterator
  that pins one chunk at a time on the background reader, with a scan policy that does
  not evict the viewport. A failed disk save must fail, never write partial text.
- Paint uses `ViewportTextSlice` and returns a fault state for an unreadable range. It
  never substitutes text, since placeholder text could reach an edit or a save.
- A failed read marks the document faulted and read-only, with a red dot on its tab. It
  uses the existing quarantine path where the session store owns the decision.
- `load()` remains for pinned data, and an unpinned miss is treated as a defect:
  `debug_assert` in development and a blocking fallible read with a fault state in release.

**Paint.** Cold reads already block paint, so stage 3 starts at parity: viewport pinning,
prefetch of nearby scroll ranges, and a blocking foreground read. Add a pending state
that paints without the text only if measured cold reads miss the frame budget. It is
larger work, because layout needs a defined result for absent text. Do not claim
"painting never waits" before then.

### Memory and backpressure

Account separately for tails, pending bodies, pinned bodies, cached bodies, queued
payloads, and page caches. None of these is a process RSS limit.

**While the store is healthy, no limit refuses text.** The only control is internal flow
control, which keeps the pending set from growing without bound when bulk text arrives faster
than the writer commits it. Its threshold is a fixed value chosen in stage 0, not a setting.
It delays bulk work and never rejects it. The user-facing limit applies only during a database
failure; see [When the store is unavailable](#when-the-store-is-unavailable).

`AddTextStorage::append` receives text on the UI thread and cannot wait, so flow control needs
a defined route per caller:

| Caller | Behavior at the flow-control threshold |
| --- | --- |
| File import (path lane) | Blocks its own thread until bytes commit. Already off-UI. |
| Paste or replacement above a size threshold | New `PreparedText`: chunked, measured, and queued off the UI thread with progress, then applied on the UI thread as sealed chunks. |
| Typing and small edits | Always accepted. They are bounded by input rate, and the tail is in RAM regardless. |

`replace_from_loaded_buffer_source` and `replace_exact_with_history_source` also need
`PreparedText`. They append a whole file at once, including when an external replacement is
applied to an open tab through `tab/layout.rs`.

### Import and observation

Split **observing** a file from **adopting** it. `read_observed_file`, reopen, and restore
reconciliation only need a digest in most cases. As written, each would import a full
file for every external-change check.

- Add a streaming decode-and-digest over the source that produces no document.
- Import only when a document is adopted, reusing the existing chunks when digests match.
- A digest match on restore keeps the stored chunks and reads no file text. Replace
  the restore-time `content_digest()` on clean buffers with the stored digest.
- Comparing a dirty document with disk streams its text off the UI thread, only when needed.
- An import that hits invalid UTF-8 midway drops its handles and falls through to the
  existing lossy path. Its committed chunks become ordinary collection candidates, as do
  those of a cancelled loading preview.

### Restore

Build a document from `document_text`, `document_tail`, `document_pieces`, and history
metadata, with **no text read**:

- Replace `StoredSession.contents` and the `HashMap<String, String>` in `Restorer`,
  `hydrate`, and `restore_closed_tab`.
- Restore `EditTree` directly at Now and tip, without `navigate_history` replay.
- Do not recompute fingerprints in `import_history_entry`. Chunk digests now provide
  integrity, verified on load. Validate at restore that every span and piece falls within
  its mapped length, and that piece totals agree. Remove the domain `fingerprint` too, including
  the edit-time hash that reads deleted text, once recovery export no longer needs it.
- `PieceProvenanceStore` is not persisted today. Its only reader outside compaction and tests
  is an entry count in `resource_probe`. Delete it rather than designing a persisted form.
- `session_buffer_format_and_metadata` stops inspecting text. `format` and `line_count` come from
  `documents`. `artifact_summary` and `has_non_compliant_characters` start from their defaults and are
  refreshed off the UI thread by the existing `analysis_lane` work.
- A cold tab stays metadata-only until hydration, as it does now.
- Recovery export streams each document to its file instead of holding `contents`.

If rewriting the piece parts proves too costly for fragmented documents, fall back to
checkpointing them periodically and replaying the few history rows after the checkpoint.
Do not build this unless stage 4 measurements need it.

### Retention and collection

The single-owner lock removes the need for durable leases or stale-lease detection:
**at startup, nothing but durable roots can reference a chunk.**

- Live references are an in-memory count per chunk row, held by `Chunk` handles.
- The startup sweep, before any document binds, deletes chunks with no `document_text`
  row. This replaces `full_cleanup`.
- At runtime, candidates are chunks whose mapping rows were removed or whose last handle
  dropped. The writer deletes one only when it has no mapping and no live handle.
- Pending chunks have no row yet and are written by the same thread that collects, so a
  chunk cannot be collected while it is becoming referenced. Deduplication onto an existing
  row takes a live reference before the row is returned.
- Delete in bounded transactions, separately from saves. Disk space is reused, not returned;
  keep the current `reusable_pages` reporting.

### When the store is unavailable

This is the largest behavior change. Today, database failure pauses saving but not
editing, and removing the temporary files removes that independence. There is no
temporary-file or scratch-database fallback. Text stays in RAM, under a limit, and the user is
told.

**Store health.** The store reports one of three states, readable without the worker queue:

| State | Entered when | Left when |
| --- | --- | --- |
| Healthy | The next commit succeeds | A failure below occurs |
| Failing | A chunk batch or session commit fails, including on a full disk. The worker retries with backoff. | A commit succeeds |
| Unavailable | The open fails, the schema is incompatible, or the `blocked` latch is set. All writes stop. | **Start new session** succeeds |

Committed chunks stay readable in the last two states if the database can still serve reads.
A read that fails follows the normal document fault path.

**RAM limit.** The limit applies only in the Failing and Unavailable states.

- It counts **RAM-only text**: tails and sealed chunks with no committed row. Cached committed
  bodies are not counted, since they can be evicted. Owned originals below the import threshold
  are not counted either, since they live in RAM in normal operation too.
- The default is automatic: 25% of total system RAM, read with `sysinfo` at each startup, with
  a 256 MiB minimum. If total RAM can't be read, it assumes 2 GiB, as `budget.rs` does.
- It is exposed in the History settings, beside the existing budgets, using `byte_budget_row`
  and the same override and reset-to-automatic pattern as `TextHistoryBudget`. The range runs
  from 64 MiB to total system RAM.
- The automatic value is recomputed at every startup until the user overrides it. An override
  above the current machine's RAM is clamped at startup. `TextHistoryBudget` derives its value
  only on creation or reset, so this needs its own startup step.
- Changing the setting applies immediately. Lowering it below current use discards nothing and
  only stops further growth.

**Behavior under the limit.**

- Typing and small edits always continue, even above the limit.
- A bulk paste or replacement that would exceed the limit is refused before any change.
- A file at or above the import threshold opens only if it fits under the limit. An import
  already running when the store fails continues in RAM, and is abandoned with an error if it
  reaches the limit. No partial document is opened.
- Accepted text is never discarded.
- When the store recovers, or after a successful reset, RAM-only text drains through the normal
  writer path and the limit stops applying.

**Status bar warnings.** Warnings appear in the status bar and do not open a dialog. This needs
a new segment, because the status bar shows no status messages today.

- Add a storage-health segment directly after the status-history icon, before the path. Items
  after the path are dropped at narrow widths by `status_bar_path_layout`, and this one must
  stay visible.
- Render it in `render_active_status` and `render_settings_status`, and also when there is no
  active document, since `show_status_bar` currently renders nothing in that case.
- Drive it from store health, not `status.current`, so an unrelated message cannot replace it.
- Long text is truncated, with the full text on hover. Hovering also gives the cause, RAM-only
  usage, and the limit. Clicking opens the Status dialog at that entry.
- Each transition is also recorded once in Status history through `StatusDomain::Session`. Retries
  record nothing. The segment stays for as long as the state lasts.

| When | Segment text and color |
| --- | --- |
| Entering Failing or Unavailable | Attention color: "Session not saving. Text is in memory only." The hover adds that unsaved text will be lost on a crash or exit unless saved to a file. |
| RAM-only text passes 80% of the limit | Attention color: "Session not saving. Memory nearly full." The hover adds that large pastes and file opens will soon be refused, and that the user can save to files, free disk space, or raise the limit in Settings. |
| Limit reached | Red: "Session not saving. Memory limit reached." |
| An operation is refused | Red: names the refused paste or file until the next edit or state change. The file-open result reports the same reason. |
| Back to Healthy | The segment disappears. "Session saving has resumed" is recorded in Status history. |

Dirty-close confirmation keeps running as it does today.

**Red dot on affected files.** A tab with an affected file shows a red dot in the existing
attention slot.

- A file is affected in either of two cases:
  - **Unsaved to the database.** The store is Failing or Unavailable, and the document's changes
    since its last committed revision are held only in RAM. A clean file holding only its initial
    load is not affected, because it can be reloaded from disk.
  - **Unreadable.** A chunk failed to load or verify, whatever the store state. This is the
    document fault described under [Reads](#reads).
- Add `BufferState.storage_attention`, a non-persisted field of either kind, and
  `TabAttentionState::StorageProblem`, which outranks `DiskProblem` in
  `summary::attention_state`. That function runs per tab per frame, so it only reads the field.
- Update the field when store health changes, when an edit happens in a non-Healthy state, and when
  a commit is acknowledged. Extend `SessionCaptureAck` with each document's committed revision, so
  that a commit in progress cannot clear a dot for a newer edit. A Healthy store does no extra work.
- `DiskProblem` already paints a red square through `paint_attention_square`, so this state is
  painted as a circle in the same red to tell them apart. Replace `attention_color: Option<Color32>`
  in [chrome/tabs.rs](../src/app/chrome/tabs.rs) with a color and shape. This reaches the tab strip,
  vertical tabs, and overflow list together.
- The tab tooltip gives the reason. A multi-file tab shows one dot, and its tooltip names the
  affected files.
- "Unsaved" dots clear once their revision commits, and "Unreadable" dots stay until the document
  is closed or replaced.

**Start new session.** Offer it **only in the Unavailable state**, when the database can't be used
at all. A Failing store keeps retrying and doesn't offer it, since deleting the session isn't the
fix for a full disk.

- The segment shows a **Start new session** action in the Unavailable state. It opens the existing
  card in the Status dialog, which explains the temporary backup. It does not reset immediately.
- `start_new_session` and that card check `session_restore_failed`, which is set only by a failed
  restore. A `blocked` latch set during a running session never sets it, so today that state has
  no way to offer a reset. Change both checks to use the Unavailable state, and clear it in
  `poll_session_reset`.
- The existing restore-failure paths also call `status_history.open()` at startup. Those are left as
  they are, since startup has no working session to interrupt.

### Reset and shutdown

**Reset.** A live document can reference old chunks only when the worker had opened
successfully. Otherwise everything is already in RAM.

1. Stop readers and the writer, so no handle blocks removal on Windows.
2. Copy the old files to the backup, as today, and remove the originals.
3. Reopen readers against the backup, marking this as the old `StoreEpoch`. Paint uses
   cached and pinned text and the deadline during the brief closed window.
4. Create the new database, copy every chunk reachable from live documents in bounded
   batches, and commit the captured session.
5. Rebind handles to the new epoch and close the old readers.

A failure before step 5 leaves live documents on the old epoch, with saving still paused.
A chunk that fails verification during the copy faults that document only.

**Shutdown.** Stop bulk intake, commit pending chunks, make the final save, close readers,
then let the worker run its final checkpoint. Readers must close first because they can
prevent `wal_checkpoint(TRUNCATE)` and the shutdown checkpoint.

## Implementation stages

Native Windows validation remains pending at the user’s explicit direction on
2026-11-05. Complete Linux checks and continue implementation; retain native
Windows runtime validation as an outstanding release acceptance item. Cross-target
compilation is useful evidence but must never be reported as native runtime testing.


Stages 2 through 4 share one development branch and one release, so users see one schema change
and old and new persistence are never shipped together. The transitional schemas in between use
fresh databases only.

### 0. Baselines and engine contract (go/no-go)

Measure the current build with [`resource_probe`](../src/bin/resource_probe.rs),
`benches/{promise_latency,frame_budget,search_speed}`, and the session cycles:

- Large-file open and first visible paint; cold scroll and jump.
- Large paste, undo, and history seek; typing during a save.
- Save and restore on large and many-tab sessions; save latency once per second with long
  history (expected to show the O(history) export).
- Search over a large file; peak RSS; database, WAL, and process write volume.

Extend the [Turso contract probe](../tools/turso-contract/src/main.rs). It currently opens
with default I/O, so first match the app's `IoBackend::Syscall` and pragmas. Measure the current build
on Linux in standard WAL. Keep the existing experimental comparison as
historical evidence, not a production prerequisite.

- All of the checks below in standard WAL on Linux and Windows.
- Abrupt exit with active reader and writer connections, then reopen, checkpoint,
  and confirm that the WAL can still be truncated. Include a WAL larger than the
  experimental frame-index capacity as a standard-mode stress control.
- Unsupported-filesystem fallback is not applicable: no experimental mode is
  attempted. File-open/I/O failures still require ordinary degraded-operation coverage.

- Readers on other threads during `BEGIN IMMEDIATE`, commit, auto-checkpoint, and TRUNCATE.
- Latency of `SELECT body` for 256 KiB rows at database sizes up to several GiB.
- Sustained 256 KiB inserts: throughput, WAL size, checkpoint pauses, write amplification.
- `synchronous=NORMAL` chunk commits followed by FULL commits, with abrupt exit.
- Disk-full, `Busy` behavior, and handling of an engine panic on a reader thread.
- CDC versus fixed chunks on edited file versions.
- `history_spans` rows versus a packed `BLOB`: save and restore with 100,000 entries, and one
  replace-all entry with a very large number of edits.
- Typed document and layout rows versus today's JSON at 10,000 tabs: steady-state save, restore,
  startup ID reservation, and closing the first tab.

**Exit:** reproducible baseline, the numeric budgets below assessed, and a decision to
proceed. Stop and reassess if foreground cold reads cannot approach today's file reads,
writes starve saves, or standard WAL fails correctness/concurrency checks. Do not enable MVCC to avoid queueing or transaction-size problems.

### 1. Preparatory refactors, no format change

Implemented on 2026-11-05; see [stage 1 evidence](../tools/turso-contract/STAGE-1.md).
Database schema 11 and session version 9 are retained at this boundary.

- Delete compaction and, if confirmed unused, `PieceProvenanceStore`, `flags.persisted`, and
  `visible_generation_before`/`after`.
- Keep standard WAL on both platforms. Ensure every opener uses the same mode,
  and keep bounded reader SQL transactions independent of in-memory text pins.
- Move history character offsets and lengths to `u64`: `PieceHistoryEdit`,
  `PersistedHistoryEdit`, and the casts in `storage.rs`, `tree_ops.rs`, `history_ops.rs`, and
  `char_len_u32`. Remove the wrapping and saturating conversions. Check entry size against the
  `provenance_retained_memory` and fragmented-session probes.
- Add `TextPin`, fallible scanners, and the viewport fault state. Replace the panics.
- Unify both caches and stop holding mutexes during I/O, still on temporary files.
- Introduce sealed/pending chunk states behind the file backing. This scaffold is temporary.
- Make `blocked` readable without the queue. Add observation without adoption.
- Add `WorkspaceTab.layout_id`, and remove `adopt_legacy_startup_settings` and its plumbing.

**Exit:** behavior matches baseline, injected read failures produce errors instead of panics
or altered text, and history, Unicode, and snapshot tests pass through state changes. A test
covers offsets above `u32::MAX` without allocating that much text.

### 2. Typed metadata, chunk store service, and new schema

Implemented on 2026-11-05; see [stage 2 evidence](../tools/turso-contract/STAGE-2.md).
This intermediate development boundary uses schema 12 / export version 10 with
the existing text/history path retained until stage 4; it is not a separate release.

Convert `session_state`, `documents`, and layouts to typed rows first. This doesn't depend on the chunk
store, so it can be tested on the existing text and history paths, and its measurements stay separate
from the storage change.

Then add the chunk tables, `ChunkStore`, reader threads, writer drain, deadlines, and epoch handle, with
explicit binding. Fix the marker, cleanup, and inventory traps listed above. Collection stays
off until roots exist in stage 4.

**Exit:** the layout and document tests pass on typed rows. Closing the first tab rewrites no layout
content, and a cursor move writes one view row. Chunks stay readable through every state, a save
waits for at most one batch, no worker reentrancy exists, and committed-chunk-only databases open
without a damage error.

### 3. Back live text with the store

Implemented on 2026-11-05; see [stage 3 evidence](../tools/turso-contract/STAGE-3.md).
The remaining shutdown and degraded-operation integration belongs to stage 6.

Replace rollover spills with sealing, and add viewport pinning, prefetch, flow control, and
`PreparedText`. Large imports may still use the temporary path until stage 5.

**Exit:** sustained paste and edit workloads stay within budget, with no unbounded queue,
deadlock, or UI wait on the writer. Accepted text survives injected write failures.

### 4. Reference-based persistence and restore

Implemented on 2026-11-05; see [stage 4 evidence](../tools/turso-contract/STAGE-4.md).
The implementation keeps `(temp_id, seq)` as history's stable composite identity
and uses composite span foreign keys. Piece metadata is split into 64 KiB parts
with digests. Its header preserves logical address lengths when Forget history
releases whole chunks. Typed artifact-analysis columns permit restore without
scanning text. Startup disk observations follow installation on the background
path lane. Export/lifecycle integration continues in stage 6, with final large
history-navigation and resource checks in stage 7.

Add typed `history` and `history_spans` rows (using the measured packed-span
fallback for very large entries when warranted), `document_text`, tail segments, `document_pieces`, the commit
protocol, metadata-only restore, collection, export, and new usage reporting. Preserve
coalescing, branches, closed documents, quarantine, cold tabs, and Forget history.

**Exit:** save/reopen and crash recovery keep exact text and history on fresh databases. No
history row or snapshot holds a text payload. Save cost no longer scales with total history
text. Restore reads no chunk bodies until text is needed.

### 5. Large-file import

Implemented on Linux. See [stage 5 evidence](../tools/turso-contract/STAGE-5.md).
Production large UTF-8 imports stream into explicit store chunks, with one-pass
inspection/digest and metadata-only initial history. Temporary backing/cache is
removed. Degraded RAM admission and lifecycle cancellation follow in stage 6.

Stream imports into chunks on the path lane, with backpressure, per-chunk metadata, and
`OriginalTextStorage::Chunked` replacing `FileBacked`. Remove the `is_file_backed`
special cases and `initial_file_history` payload duplication.

**Exit:** a source file's later change cannot alter live or undo text, one edit in a large file
writes only new bytes, and observation of an unchanged large file writes nothing.

### 6. Lifecycle and degraded operation

Implemented on Linux. See [stage 6 evidence](../tools/turso-contract/STAGE-6.md).
Health, failure RAM settings, storage status and attention, revision-aware
acknowledgements, backup-reader reset/rebinding, shutdown and streaming recovery
export are in place. Final large-action/UI validation follows in stage 7.

Implement store health, the RAM limit and its setting, the status bar segment, the red dot, the
Unavailable gate on **Start new session**, reset, shutdown order, and recovery export.

**Exit:** each Failing and Unavailable condition behaves as specified on Linux and Windows, with
the limit enforced only in those states and every transition recorded once. Only affected files show
a dot. **Start new session** is offered only when Unavailable, including a `blocked` latch set during
a running session. Reset preserves live and undo text, and a failed reset leaves documents readable
with saving paused.

### 7. Validate and remove temporary backing

Run the full validation below. Then remove `tempfile` use from `storage.rs` and `text_cache.rs`,
along with the scaffold. Keep `tempfile` where reset and tests still use it. Update
[SESSION-RETENTION.md](../SESSION-RETENTION.md), which describes temporary backing, and
[Incremental Turso sessions](turso-incremental-session.md), which describes the old schema.

**Exit:** all sealed text uses the store, with no spill path and with documented evidence.

## Validation and acceptance

**Budgets.** These are stage 0 targets. On 2026-11-05 the user explicitly approved treating
50 ms as a batch scheduling target, recording I/O overruns and separately
validating UI responsiveness; no other performance target was relaxed. Compare against the baseline in the
same conditions, and do not relax existing performance budgets to pass.

| Metric | Proposed limit |
| --- | --- |
| Foreground cold read, p99, during saves | Under 4 ms (half of a 120 Hz frame) |
| Keystroke latency during a save | No regression |
| Save wait caused by chunk writes | At most one bounded batch; 50 ms scheduling target, report I/O overruns and validate UI responsiveness |
| Write volume when typing | Close to bytes typed, plus metadata and WAL overhead |
| Large-file open and first paint | No worse than 1.5× baseline; report import write volume separately |
| Restore of a large document | Faster than baseline; zero chunk bodies read before first paint of that tab |
| 10,000-tab steady-state save and restore | No regression against `session_persist_cost` and `session_restore_cost` |
| Closing the first of 10,000 tabs | Faster than baseline, since no layout content is rewritten |
| Search over a large file | No worse than 1.5× baseline |

**Correctness.**

- Exact Unicode, CRLF, and byte/character ranges across chunk boundaries, tails, and segments.
- Edits, undo, redo, branches, Forget history, closed documents, and cold tabs across save,
  crash, and reopen.
- Divergent clones, including a `pending_reopen_original` restored after later edits and saves.
- A source file modified or deleted after open, and observed repeatedly without adoption.
- History rows: every edit kind, multi-span deletes, coalescing, and very large entries match the
  live document after reopen. Undo and redo across many entries write only `document_history`.
- Metadata rows: every `SessionBuffer`, `SessionView`, and pane field round-trips exactly, including
  selections, absent disk values, split layouts, and every format enum value.
- Layouts: tab reorder, close, view close, split, combine, and reopen each leave every layout with one
  owner and no orphans. Save As, retained path lookup, cold tabs, and closed archives behave as before.
- A non-UTF-8 path produces a save error and is never stored altered. Quarantine archives stay readable
  when document and layout rows are damaged.

**Failure.** Each must keep text intact and report an error:

- Disk full, failed and ambiguous commits, abrupt exit between each commit-protocol step.
- Corrupt or missing bodies, on paint, edit, undo, save-to-disk, search, and export.
- Reader `Busy`, deadline expiry, and a reader panic.
- A cancelled import and invalid UTF-8 midway through an import.
- Each Failing and Unavailable condition, and a failed reset at each step.
- A healthy store never refuses text, however large, and only delays it through flow control.
- During a failure: the limit counts only RAM-only text, typing continues above it, bulk work
  and imports are refused cleanly, and text drains after recovery.
- The status bar segment stays visible at narrow widths, in Settings, and with no active document.
  It is not replaced by other status messages, and no warning opens a dialog. History records each
  transition once, including at 80%.
- The red dot appears on every tab surface for affected files only, not for clean files holding
  only their initial load. It stays visually distinct from the `DiskProblem` square, and a commit
  in progress doesn't clear it for a newer edit.
- **Start new session** appears when Unavailable and never when Failing.
- The limit: the automatic value follows system RAM across restarts, an override persists and is
  clamped on a smaller machine, and lowering it during a failure discards nothing.

**WAL mode.** Linux and Windows both open standard WAL. Correctness, failure,
and concurrency checks pass on each platform. Reader snapshots are bounded to
chunk reads, reset/shutdown release all connections in the required order, and
a second owner is refused by `session.db.lock`. The experimental overflow
reproducer remains available for later Turso reassessment.

**Concurrency.** No deadlock or unbounded queue with cold reads, bulk writes, saves, checkpoints,
collection, and UI-thread persists running together. Collection never removes live-only text or
history-only text. Stale chunks are eventually reclaimed.

**Resources.** Report cache, pending, pinned, tail, and page-cache memory, and peak RSS, alongside
database growth, WAL peak, process write volume, deduplication, and manifest size. Removing
temporary files alone does not prove lower disk use.

Rejecting an incompatible database leaves it byte-for-byte unchanged, as the existing schema
tests check. No migration tests are needed.

## Risks

- **Live text now depends on a pre-1.0 engine.** Every read and undo goes through it. The
  go/no-go in stage 0, digest checks, and panic containment reduce this risk but do not remove it.
- **Standard WAL still depends on bounded reader lifetimes and checkpoint progress.**
  Keep SQL transactions short and test long scans concurrently with writes.
  Multiprocess WAL remains deferred after the 0.8.3 overflow failure and must be
  reassessed against its retained reproducer on later engine versions.
- **Writes cost more than a temporary file.** A body is written to the WAL and checkpointed to
  the database, with fsync, instead of one unsynchronized write. Import is the worst case.
  Against that, the design removes today's repeated copies of the same text.
- **Database health now limits editing,** as described above. Disk-full also matters more: the
  state directory now takes writes that previously went to the temporary directory.
- **The session schema is almost entirely rewritten.** Save, restore, cache comparison, and many
  session tests change, and tests that inject damage with `payload=x'00'` or use triggers on `payload`
  need new forms of damage. Doing typed metadata first in stage 2 keeps that change separately testable.
- **Reads may stay slower than the page cache.** The foreground reader, prefetch, and a pending
  paint state are the available responses, in that order.

## Decisions

1. **Database failure:** text stays in RAM with no fallback store, and the user is warned.
2. **RAM limit:** it applies only during a database failure, derives from system RAM at startup,
   and is exposed in settings. The 25% default and 256 MiB minimum are starting values, to be
   adjusted if stage 6 testing shows they need it.
3. **History offsets:** these move to `u64` in stage 1.
4. **Warnings:** these appear in the status bar and do not open a dialog. The one exception is a
   failed restore at startup, which keeps opening the Status dialog as it does today.
5. **Affected files:** these show a red dot on their tab. The exact definition of "affected" and the
   circle shape are this plan's interpretation.
6. **Start new session:** offered only when the database cannot be used at all.
7. **WAL:** standard WAL on both Linux and Windows, approved on 2026-11-05.
   Revisit multiprocess WAL on later Turso versions only after the retained
   overflow and recovery contracts pass. No automatic experimental-mode fallback.
8. **History storage:** typed `history` and `history_spans` rows replace the JSON `entry`.
9. **Other metadata:** `session_state` and `documents` become typed rows, and live and closed tabs share typed
   layout tables. Only `layouts.root_pane` and `quarantine.payload` stay JSON.
10. **Legacy settings:** `font_size` and `word_wrap` leave the session, along with the code that adopts them.
