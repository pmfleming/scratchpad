# Incremental Turso session experiment

This supersedes the storage/import design in `turso-migration.md`. Schema 3 is a
breaking experiment: no JSON import, no schema-1/2 upgrade, no fallback session
root. Unsupported databases fail closed and are left intact; move the old session
directory aside explicitly to start fresh. Ordinary saved text files are unchanged.
The offline recovery command still exports human-readable text and JSON, but that
export is not an automatic import format.

## What is split, and why

- Tab/view/layout metadata remains JSON. It is read/written together. No path,
  name, dirty-state or disk-time columns were added.
- `buffer_ids(id)` owns the lifecycle of history and snapshots, including clean
  disk buffers with undo history **but no redundant text snapshot**.
- `history(temp_id, seq, global_seq, entry)` stores one independently changing
  history entry. `seq` is the stable document-local entry **ID**, not its position
  in the vector. `global_seq` changes on coalescing. Its index reserves the global
  sequence above all stored histories on open, including still-cold tabs, without
  decoding their payloads. The entry retains its existing serialized form.
- `snapshots(id, bytes, digest)` checks the complete recovered byte stream.
- `blobs(digest, body)` stores BLAKE3-addressed bodies;
  `parts(temp_id, part, digest)` stores ordered references.

These are STRICT tables on the pinned **Turso** engine. Foreign keys are enabled,
with cascades from buffer identity to history/snapshots and snapshots to parts.
Parts reference blobs; an index on `parts.digest` supports reference-based
collection. FK enforcement and STRICT types are tested against Turso itself,
not a SQLite substitute. We retain FULL synchronous transactions. There is no
SQLite-compatibility layer or migration machinery.

## Save/read behavior

History is compared by complete entry value, not just ID/fingerprint/global_seq:
undo/redo flags, coalescing, selections, and payload-budget stripping can all
change an existing row. Only changed entries are serialized/upserted. Prefix and
suffix removals use range deletes; interior removals are handled too. Caches are
updated only after commit and invalidated after a failed transaction. A reopened
writer primes history comparison once from stored rows. Cold tabs preserve their
rows without materializing history; hydration loads the selected tab's rows.

FastCDC v2020 consumes the snapshot's piece stream using a bounded reader
(16 KiB minimum, 64 KiB target, 256 KiB maximum). Boundaries do not depend on
piece-tree segmentation or character offsets. Chunks may split UTF-8 scalars;
reconstruction validates the full length, hash, and UTF-8. Existing part digests
avoid rebinding/reinserting unchanged bodies and rewriting unchanged references.
New bodies are inserted only when their digest is absent, also deduplicating
across tabs. Stale parts are trimmed transactionally. Garbage collection is:

```sql
DELETE FROM blobs WHERE digest NOT IN (SELECT digest FROM parts);
```

Quarantined snapshots retain their parts and history, even after their live tabs
close. Their bodies therefore remain GC roots. Shared corrupted bodies are never
silently overwritten. Text references, history, metadata and collection commit
or roll back as one transaction.

This is **incremental I/O, not incremental CPU**: every changed snapshot is still
scanned/hashed, and capture still exports history payloads. The worker retains
one comparison copy of warm history. Reducing capture/copy costs or avoiding the
full scan requires a later revision-aware document API, not more SQL columns.

## Performance lens workload

The project-owned `resource_probe` now emits `large_dirty_buffer_save`, declared
in `benches/measurement_manifest.json`. The sibling performance lens registers
its evidence requirements and review scale target. It is part of the full
resource suite, not just an isolated benchmark:

- One initially 50 MiB unsaved/dirty buffer with deterministic non-repeating ASCII.
- 1,024 actual undo entries, 16 MiB persisted payload budget, explicitly fixed
  rather than dependent on host RAM. Subsequent edits exercise payload trimming.
- Nine one-character saves, alternating near-front, middle, and end.
- Initial commit, edits, and fixture construction excluded; capture, serialization,
  FULL synchronous commit and any automatic checkpoint included in save timing.
- Per-save `process_resources.write_bytes` measures process-accounted write I/O
  (including WAL/checkpoint activity), **not physical device traffic or file-size
  growth**. Missing OS counters remain null and do not close the lens evidence gap.
- Reopened logical content/history correctness is checked after the samples.

Run independently of the expensive full resource sweep:

```sh
cargo build --release --bin resource_probe
python3 scripts/measure-large-dirty-save.py --output target/save-experiment/run-1
```

This launches three fresh processes by default and retains all nine samples per
process, stderr, executable SHA-256, host metadata and per-launch medians. Use a
new output directory each time. Use the same fixture revision, engine, durability,
host/filesystem and build profile for comparisons. `--large-dirty-save` is also a
direct `resource_probe` flag.

### Local observations (not portable budgets)

Initial staged measurements, release build, same host:

| Stage | Median save | Median process write bytes/save |
|---|---:|---:|
| Whole history + fixed-offset chunk rewrites | ~465 ms | ~216 MB |
| History rows only | ~260 ms | ~154 MB |
| History rows + CDC/deduplicated bodies/reused references | ~50 ms | ~156 KB |

The first two stages each used one launch/nine saves; the final stage used three
launches/nine saves each (49.1–50.8 ms medians, 155,648-byte medians). These are
local directional observations, not statistical significance or CI thresholds.
The per-tab/state serialized metadata shrank from ~18 MB to 718 bytes. Raw local
artifacts are under `target/turso-incremental-save/`; rerun rather than treating
these numbers as universal performance claims.

Tests cover append/update/prune/clear/reopen history, lazy cold hydration, clean
file history, rollback/retry, global-sequence reservation, Unicode and empty text,
front/middle/end edits, duplicate tabs, GC, quarantine preservation, foreign keys,
STRICT typing, disk full, and acknowledged commits surviving abrupt exit.
