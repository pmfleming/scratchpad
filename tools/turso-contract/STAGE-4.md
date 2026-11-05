# Stage 4: reference persistence and lazy restore

Schema 12 / export version 10 now uses chunk references throughout durable text
and history. `snapshots`, `blobs`, `parts`, and `history.entry` are gone. History
keeps the existing stable `(temp_id, seq)` composite identity, with composite
foreign keys for ordered spans. Entries above 256 spans use the measured packed
fallback (eight checked little-endian u64 fields per span); ordinary spans use
rows. Character lengths also travel with domain byte spans. Only navigation
positions are updated on undo/redo; derived flags are reconstructed.

`document_text` supplies durable chunk roots, including history-only, closed, and
quarantined text. Mutable add tails write UTF-8 suffix segments. Prefix hashes
identify divergent captures. Rollover publishes mappings and removes covered
segments atomically. Piece metadata uses 64 KiB parts with individual BLAKE3
checksums. Its header records logical address lengths; released whole chunks leave
holes without renumbering surviving spans. Forget history releases those chunks
while older clones retain their own handles and text.

The actor drains captured pending bodies in bounded NORMAL transactions before
preparing references, changed history rows, piece parts, and tail suffixes. The
FULL manifest transaction reads no text and hashes no tail payload. Failed
transactions invalidate comparison caches; immutable bodies already committed
remain independently readable and retryable. Collection scans/deletes at most
four candidates per batch and protects both durable roots and transient live
handles. Collection and body-write I/O overruns above the 50 ms scheduling target
are logged. Existing deduplicated bodies are compared against the newly accepted
bytes before their pending RAM copy can be released; corrupt or missing bodies
leave that copy pending and report a failure.

Restore builds document storage, pieces, and history topology from metadata. It
never replays the history route or reads chunk bodies to reconstruct a document.
Snapshots obtain line counts from piece metrics, including the last-piece LF
flag. Encoding and artifact analysis metadata round-trips in typed columns.
Startup returns committed text first; source-file observations run through the
background path lane after installation. Explicit retained-file reopen keeps its
existing reconciliation behavior. Read epochs remain usable by returned documents
after their SessionStore is dropped; explicit replacement closes readers.

Production captures contain references only. The exported-payload fixture helper
is compiled only for tests. Recovery export streams current text with verified
reads and derives its JSON history from references. The remaining export/lifecycle
integration is covered in stage 6. Large history-navigation preflights and resource
budgets receive their final workload checks in stage 7.

## Validation

All-feature library, integration, and documentation tests passed (683 library
passes, seven intentional ignores). All-target/all-feature Clippy passed with
warnings denied. Tests include zero body-read requests during multi-MiB restore
and snapshot creation; Unicode, CRLF, branches, exact replay and export; packed
span entries; append-only tail write counts; divergent captures; Forget history
with sparse mappings and old clones; live-only/history-only collection roots;
manifest rollback at multiple tables; damaged metadata and quarantine; typed
metadata, closed layouts, cold tabs, and incompatible schema protection.

The production-capture 10,000-tab measurement is retained in
`evidence/stage-4/metadata-measurement.log`:

- First save: 1,734 ms (stage 2: 1,008 ms); the first reference manifest adds work.
- Steady save: 84 ms (87 ms).
- Shell restore: 109 ms (121 ms).
- Startup ID reservation: 2 ms (4 ms).
- Close first tab after reopening: 437 ms (715 ms).

An initial measurement incorrectly reconstructed exported test fixtures on every
save. It now captures the same references as the app. Runtime fixes also removed
quadratic snapshot searches, bulk-prime empty history positions, and skip unchanged
history SQL. No performance budget was changed.

Stage 3's frozen RQL producer/verifier passed with stable source hashes. Its strict
gate remains failed for partial evidence and required human reviews; its
`test-failure` policy also reports partial execution evidence, with no reported
test failures. The compact report is in `evidence/stage-3/quality-summary.json`.
Stage 4’s fresh frozen producer/verifier passed with stable source hashes; the strict gate remains failed for the same partial evidence and three required reviews. Its compact report is `evidence/stage-4/quality-summary.json`. Native Windows runtime
validation remains pending under the user's authorization. Large-file originals
retain the temporary importer until stage 5; failure RAM limits, status, replacement,
and shutdown integration remain stage 6.
