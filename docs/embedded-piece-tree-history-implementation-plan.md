# Embedded Piece-Tree History Implementation Plan

## Goal

Implement per-file undo history by embedding lightweight provenance and history records into the piece-tree/document layer. Each file owns its own tree history; global history is a derived view that merges currently open files by monotonic sequence number.

The target model is:

- loaded and inserted spans carry source metadata such as `Load`, `Edit`, `Paste`, and `SearchReplace`
- deleted or cut spans become invalidated history records that reference the original bytes, not lost history
- closing a file drops that file's tree and therefore all associated history
- restoring the on-disk version after an external change drops that file's history
- restoring the session version may keep history if validation passes

## Capacity And Performance Budget

Capacity is a first-class requirement, not an afterthought. The implementation must hold these invariants:

- **Per-entry footprint**: target ≤ 64 bytes for the in-memory entry struct, excluding referenced text spans.
- **Text storage**: inserted and deleted text is referenced into existing piece-tree byte buffers, never duplicated as owned `String`. Owned bytes are only used as a fallback for cross-session persistence payloads.
- **Dual budget**: bounded by both `MAX_HISTORY_ENTRIES` (per file) and `MAX_HISTORY_BYTES` (per file and aggregate across open files). Eviction drops the oldest entries until both budgets are satisfied.
- **Provenance**: per-piece metadata is sparse; pieces from `Load` share a single sentinel and consume zero extra bytes.
- **Ordering**: a monotonic `u64` sequence is the canonical sort key; wall-clock timestamps are display-only and lazily materialized.
- **Replay validation**: fast-path fingerprint compare; full text compare only on mismatch.

### Memory-Adaptive Defaults

Budgets are not hard-coded constants. They are derived once at app startup from available system memory, then exposed as user-adjustable values in advanced settings.

**Startup derivation:**

1. Probe available physical memory via a thin platform helper (`sysinfo` crate or equivalent on Windows/macOS/Linux). Use *available* memory, not total — total over-allocates on machines with other workloads.
2. Compute the aggregate budget as a fraction of available memory, clamped to a sane window:

   ```
   aggregate = clamp(available_memory * 0.02, 16 MiB, 512 MiB)
   per_file  = clamp(aggregate / 8,           4 MiB,  64 MiB)
   persisted = clamp(aggregate / 16,          1 MiB,  16 MiB)
   entries_per_file = clamp(per_file / 8 KiB, 500,    10_000)
   ```

   The 2% fraction is conservative; the editor itself, syntax tries, and OS overhead must fit alongside history. The clamp window prevents a 256 GB workstation from dedicating gigabytes and a 4 GB laptop from starving.

3. Store the derived values in `AppSettings::history_budget` with a `derived_from_memory: bool` marker so the settings UI can show "auto" vs. "user-set."

**Settings exposure:**

The advanced settings panel exposes four fields under a "Text history" section:

- `Per-file entry limit` — integer, range `100..100_000`
- `Per-file byte budget` — bytes with MiB/GiB display, range `1 MiB..1 GiB`
- `Aggregate byte budget` — bytes with MiB/GiB display, range `4 MiB..4 GiB`
- `Persisted payload budget` — bytes with MiB/GiB display, range `0..1 GiB` (`0` disables Tier 2 persistence entirely)

Each field shows the auto-derived default beside the input and offers a "Reset to auto" action that re-runs the startup derivation against current available memory.

**Live application:**

When the user changes a budget at runtime:

1. Validate against the per-field range.
2. If the new aggregate is below current usage, immediately evict oldest entries across files until the budget is satisfied. Eviction is logged so users can see why entries disappeared.
3. Update `AppSettings::history_budget.derived_from_memory = false`.
4. Persist to settings storage so the value sticks across sessions.

A user-set value takes precedence over startup derivation on subsequent launches; "Reset to auto" is the only way back.

**Concrete example** on a 16 GiB machine with ~10 GiB available at launch:

```
aggregate        = clamp(10 GiB * 0.02, 16 MiB, 512 MiB) = 204 MiB
per_file         = clamp(204 MiB / 8,    4 MiB,  64 MiB) =  25 MiB
persisted        = clamp(204 MiB / 16,   1 MiB,  16 MiB) =  12 MiB
entries_per_file = clamp(25 MiB / 8 KiB,   500,  10_000) = 3_200
```

On a 4 GiB machine with ~2 GiB available:

```
aggregate        = clamp(2 GiB * 0.02,  16 MiB, 512 MiB) =  40 MiB
per_file         = clamp(40 MiB / 8,     4 MiB,  64 MiB) =   5 MiB
persisted        = clamp(40 MiB / 16,    1 MiB,  16 MiB) =   2.5 MiB
entries_per_file = clamp(5 MiB / 8 KiB,    500,  10_000) =   640
```

## Phase 1. Provenance Types And Sparse Storage

Add small provenance structures near the piece-tree domain model.

```rust
#[repr(u8)]
pub enum PieceSource {
    Load = 0,
    Edit,
    Paste,
    Cut,
    SearchReplace,
}

pub struct PieceProvenance {
    pub change_id: u64,
    pub source: PieceSource,
    pub session_generation: u32,
}
```

**Storage decision (resolves Open Design #1):** provenance lives in a sparse side store, not inline on `Piece`.

- `PieceTreeLite` (or `TextDocument`) owns a `PieceProvenanceStore` that maps a stable piece key to `PieceProvenance`.
- `Load` pieces never get an entry; the absence of an entry implies `PieceSource::Load` with `change_id = 0`.
- Only edited / pasted / cut / search-replace pieces incur a side-store row.

Rationale: `Piece` is currently 40 bytes ([src/app/domain/buffer/piece_tree.rs:69](src/app/domain/buffer/piece_tree.rs:69)); typing produces many small pieces, and a 4-byte inline `provenance_id` would cost 8 bytes per piece after alignment. A sparse map keeps the common case (loaded text) at zero overhead.

Wall-clock timestamps are not stored on entries. A monotonic sequence number is. See Phase 2.

## Phase 2. History Entries — Compact, Reference-Backed

Keep the existing visible piece tree behavior for rendering, lookup, search, and file writes. Add a parallel per-file history list that records every committed mutation using span references rather than owned text.

```rust
pub struct PieceHistoryEntry {
    pub seq: u64,                      // monotonic, global
    pub source: PieceSource,
    pub visible_generation_before: u32,
    pub visible_generation_after: u32,
    pub fingerprint: u64,              // FxHash of edit payload
    pub edits: SmallVec<[PieceHistoryEdit; 1]>,
    pub flags: PieceHistoryFlags,      // undone, replayable, persisted
}

pub enum PieceHistoryEdit {
    Inserted { start_char: u32, span: ByteSpan },
    Deleted  { start_char: u32, span: ByteSpan },
    Replaced { start_char: u32, deleted: ByteSpan, inserted: ByteSpan },
}

pub struct ByteSpan {
    pub buffer: PieceBuffer,           // Original or Add
    pub start_byte: u32,
    pub byte_len: u32,
}
```

Storage rules (resolves Open Design #2):

- **Inserted text** references the add-buffer span. The add-buffer is append-only within a session; references stay valid until the document is dropped.
- **Deleted text** stays alive as orphan pieces — pieces unlinked from the visible tree but still referencing live byte ranges. The history entry holds the byte span; the orphan piece holds the byte range alive against compaction.
- **Eviction** drops the oldest history entry *and* releases its orphan-piece references. When all references to a tail of the add-buffer are gone, the buffer can truncate.
- Owned `Box<str>` is used only when serializing for persistence (Phase 6).

`SmallVec<[PieceHistoryEdit; 1]>` avoids a heap allocation for the single-edit common case. `start_char: u32` and `byte_len: u32` cap individual edits at 4 GiB, which is well above any realistic single edit and saves 16 bytes per edit versus `usize`.

The visible tree always contains visible text only. Tree metrics, slices, anchors, and rendering are unchanged.

## Phase 3. Source-Aware Edit APIs With Coalescing Commit Points

Add source-aware edit entry points to `TextDocument` and `PieceTreeLite`. Each call is a *commit point* for a history entry.

Examples:

- `insert_with_source(offset, text, PieceSource::Edit)`
- `replace_ranges_with_source(ranges, source, previous_selection, next_selection)`
- search/replace uses `PieceSource::SearchReplace`
- paste uses `PieceSource::Paste`
- file load initializes provenance as `PieceSource::Load`

Existing APIs become wrappers that default to `PieceSource::Edit`.

**Coalescing rules (must move down with the history):**

- Adjacent insertions from `PieceSource::Edit` within `TEXT_HISTORY_COALESCE_WINDOW = 1200ms` merge into the previous entry, matching today's ledger ([src/app/text_history.rs:82-113](src/app/text_history.rs:82-113)).
- IME composition / keystroke runs batch within the editor's input layer before reaching `insert_with_source`. The piece tree never sees per-character commits during a composition.
- `Paste`, `Cut`, and `SearchReplace` never coalesce. Each call is its own entry.
- Coalescing extends the existing entry's `inserted` byte span (the add-buffer is contiguous for the run) and updates `fingerprint`, `visible_generation_after`, and `next_selection`.

Without enforced coalescing, a typed paragraph becomes hundreds of entries plus hundreds of orphan pieces. Capacity dies fast.

## Phase 4. Replay With Fingerprint Fast-Path

Use each file's embedded history as the source of truth for undo/redo.

Rules:

- normal undo targets the latest non-undone history entry for the active file
- redo targets the latest undone entry that is still replayable
- moving to a point in time applies or reverses entries until the requested entry is reached
- replay validates `visible_generation` and `fingerprint` (FxHash of edit payload) before mutation
- only on fingerprint match does replay proceed; on mismatch replay fails without partial mutation

Validation tiers:

1. **Fast path**: compare `visible_generation_before` against current generation. If equal, the document is in the expected state — no payload compare needed.
2. **Fingerprint path**: when generations differ (e.g. interleaved edits in another file restored that file's generation), compare the cached `fingerprint` against a fresh hash of the current span. Cheap, allocation-free.
3. **Slow path**: only on fingerprint mismatch, fall back to full text compare. This is the only path that allocates.

Stepping backward through 200 entries on the fast path is ~200 integer compares. Stepping through 200 entries on the slow path was the original concern; the fingerprint reduces it to one hash per entry.

This can initially reuse `TextDocumentOperationRecord` internally, but the long-term owner is the file's piece-tree/document history, replacing the separate `operation_undo` and `operation_redo` stacks.

## Phase 5. Global History As A Cached Merged View

Do not store a second global ledger. Build it on demand from open files, but cache the merged ordering.

1. each file exposes a `revision_counter: u64` that increments on any history mutation
2. the workspace caches `merged: Vec<(seq, BufferId, entry_id)>` plus a vector of per-file revision counters captured at build time
3. on dialog paint, compare current per-file counters against the captured set; rebuild only on mismatch
4. sort by `(seq, BufferId)` — `seq` is globally monotonic so the secondary key only matters for stable display
5. show file name, source, summary, and status
6. route undo/redo actions back to the owning file history

The current dialog rebuilds the merged view every frame ([src/app/ui/dialogs/text_history.rs:49-54](src/app/ui/dialogs/text_history.rs:49-54)). Caching by revision counter eliminates per-frame allocations on long sessions.

File close drops the file's tree and history; the cache is invalidated by the revision-counter check on the next paint. No special teardown needed.

**Cross-file atomic time-travel is explicitly out of scope (resolves Open Design #5).** Global history is a display-and-dispatch surface. Each undo/redo applies to one file at a time. Atomic multi-file replay would explode the validation surface for no concrete user benefit.

## Phase 6. Session Restore And Two-Tier Persistence

Connect history retention to the existing disk-versus-session choice.

Restore rules:

- If the user chooses the on-disk version after an external change, discard that file's embedded history.
- If the user chooses the session version, keep embedded history only if the session buffer fingerprint, tree generation, and saved metadata validate.
- If validation fails, restore the text but discard executable history with a warning.

**Two-tier persistence (resolves Open Design #4):**

- **Tier 1 — metadata, always persisted**: `seq`, `source`, `start_char`, edit lengths, `summary`, `fingerprint`, `visible_generation_before/after`, `flags`. Compact (~64 bytes per entry). The dialog can display historical entries even when replay payloads were dropped; non-replayable entries are marked.
- **Tier 2 — replay payloads, optional and bounded**: owned `Box<str>` for inserted/deleted text. Gated by `MAX_PERSISTED_PAYLOAD_BYTES = 4 MiB` per session. Oldest payloads drop first when the budget is exceeded.

On reopen, Tier 1 metadata reconstructs the dialog view. Tier 2 payloads, where present, allow replay; entries without payloads display normally but `flags.replayable = false`.

Re-validate fingerprints against the restored buffer before marking entries replayable. A silent rebuild without revalidation could replay against drifted text.

## Phase 7. Validation And Tests

Add focused tests before UI expansion:

- loaded spans get `Load` provenance with no side-store entry
- typed edits produce `Edit` history entries that coalesce within the window
- typed runs across the coalesce window boundary produce separate entries
- paste produces `Paste` entries that never coalesce
- search/replace produces `SearchReplace` entries
- deleted text is reachable through orphan pieces but absent from visible text
- per-file undo/redo replays expected text via fast-path generation match
- per-file undo/redo replays via fingerprint path when generations diverge
- replay fingerprint mismatch leaves the document unchanged
- moving backward/forward to a file-local point works
- byte-budget eviction drops oldest entries and releases orphan pieces
- entry-budget eviction drops oldest entries
- closing a file removes its entries from global history
- choosing on-disk restore drops that file's history
- choosing session restore keeps history only when fingerprints validate
- restored entries with no Tier 2 payload are present but non-replayable

## Resolved Design Decisions

The previous "Open Design Decisions" section is closed:

1. **Provenance location**: sparse side store keyed by piece identity. `Load` pieces share an implicit sentinel.
2. **Deleted text storage**: orphan pieces holding byte-range references; eviction releases them. Owned strings only for persistence payloads.
3. **Timestamps**: monotonic `u64` sequence is the ordering key. Wall-clock `SystemTime` is display-only and lazily materialized.
4. **Persisted history**: two-tier — Tier 1 metadata always, Tier 2 payloads bounded by `MAX_PERSISTED_PAYLOAD_BYTES`.
5. **Cross-file atomic time-travel**: not supported. Global history dispatches per-file.

## Open Items

- Validate the 2% aggregate fraction and clamp window against real session telemetry; adjust the formula if the auto-derived defaults feel wrong on common hardware profiles.
- Whether orphan-piece release should trigger add-buffer compaction proactively or lazily on save.
- Whether `revision_counter` lives on `TextDocument` or one level up at the workspace's per-buffer state.
- The application is currently focused on Windows - the probe should be accurate in that OS.
