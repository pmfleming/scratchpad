# Local Turso text and sessions

Scratchpad uses one local Turso 0.8.3 database for immutable live text, undo and
session recovery. There is no remote URL or temporary text spill file. Linux and
Windows both use standard WAL with `IoBackend::Syscall`. Experimental multiprocess
WAL is disabled; revisit it on a later Turso version only after the retained
overflow and crash-recovery probes pass.

Schema **13**, recovery export version **10**, is a breaking development format.
Unknown schemas are rejected without modification. There is no migration,
old-format reader, automatic JSON import, or fallback database. **Start new
session** is the explicit replacement action when storage is Unavailable.

## Text and metadata

`chunks` stores identities, digests and byte, character, line-feed, ASCII and
final-line-feed metadata. `chunk_bodies` stores deduplicated UTF-8 bodies of at
most 256 KiB. Each read verifies
the body against that metadata. Large UTF-8 files stream through UTF-8-adjusted
FastCDC (16/64/256 KiB). Small originals remain owned in RAM during editing;
non-UTF-8 files retain their RAM decoder, then hand large decoded text to chunks.
Inserted text has a mutable tail and seals finished 256 KiB chunks by reference.
Logical original/add byte addresses never change, including across divergent clones.

`document_text` retains original and sealed-add chunks, including history-only
text. `document_tail` stores bounded mutable-tail suffixes. Checksummed 64 KiB
`document_pieces` parts describe visible pieces and logical address lengths.
`history` stores typed entry metadata and selections; `history_spans` stores
ordered logical references, with a packed form for entries exceeding 256 spans.
`document_history` alone tracks Now and the selected tip. Save and restore do not
serialize undo text or replay the entire document.

`documents`, `session_state`, `layouts`, `layout_documents`, `layout_views`,
`tabs` and `closed_tabs` contain typed metadata. Pane structure remains JSON.
Layout IDs are stable; moving or closing a tab changes its ordering references,
without rewriting unaffected layout content. A cursor move updates its view.

## Threads and commits

One bounded actor owns writes. Two bounded reader lanes serve foreground and
background reads with independent connections, deadlines and panic containment.
Text pins hold shared RAM bodies, never SQL transactions. Cache locks are released
before I/O. Aggregate and per-owner budgets evict committed bodies only.

Healthy bulk producers pause at 32 MiB pending text, with at most one additional
chunk admitted under the shared admission lock. Ordinary typing never waits for
that queue. Hashing and deduplication run on the writer. Large paste, replacement,
indentation, clipboard extraction and history navigation run in background
preparation; accepted input is deferred in order until atomic publication.

Body transactions use `synchronous=NORMAL`, at most four chunks or 4 ms of batching
work. Session manifests use FULL durability after their body references commit.
A successful FULL manifest is the recovery acknowledgement. Failed or ambiguous
commits retain pending RAM text and invalidate comparison state. The 50 ms batch
limit is a **scheduling target**, by the user's explicit decision: an indivisible
I/O or checkpoint can exceed it. Measured overruns are retained in evidence; UI
responsiveness is assessed separately. No reader or UI target was relaxed.

UI persistence requests and final-view closes run asynchronously. A close keeps
its tab/view until the committed revision and layout still match; a newer edit
keeps the work open. Exit waits for accepted preparation/input and close intents,
then performs a final save, closes readers and checkpoints the writer.

Restore installs committed references and cached metadata before source checks.
It reads no chunk bodies until text is requested. Newly requested files are
adopted on the path lane before cold-tab publication; a missing committed text
manifest never causes an automatic reload of a newer source file. Dirty documents
observe external changes without importing replacement chunks. Clean replacements
and explicit reopen retain previous text as undo references.

Collection considers durable roots and live process handles, in bounded batches.
It cannot delete live-only, history-only, retained or quarantined text. Dead weak
registry entries are pruned. Reusable database pages are reported separately;
collection does not promise file shrinkage.

## Failure and recovery

Healthy, Failing and Unavailable health is readable without the writer queue.
Failure retains accepted text in RAM. A configurable failure-only RAM limit
counts pending bodies, tails, deferred input and large RAM-owned originals;
ordinary small originals and evictable cache are excluded. The automatic limit
is 25% of system RAM, at least 256 MiB, recomputed at startup. Typing continues
over the limit; additional bulk operations are refused atomically.

A persistent status segment reports failure, memory pressure and refusals.
Affected files show a red circle for unreadable or uncommitted edits, distinct
from a disk-problem square. Revision acknowledgements cannot clear newer edits.
A failed read faults the document and refuses mutation; scanners, search, save
and recovery export propagate the error.

Reset is available only for Unavailable storage. It closes all connections,
copies the complete database file set to a private backup, reopens old readers
against the backup, copies reachable chunks into a fresh store, and commits the
new manifest before publishing new references. Failure leaves accepted work on
its previous epoch with saving paused. Recovery export streams current and undo
text, including retained documents and closed layouts, into explicit JSON/text
artifacts. It is not an automatic import format.

## Reproducible validation

The implementation plan and measured evidence are in
[turso-live-text-storage-plan.md](turso-live-text-storage-plan.md) and
[the contract reports](../tools/turso-contract/STAGE-0.md), including stage-specific
reports and retained raw/compact artifacts. Run production resource probes with:

```sh
direnv exec . cargo run --release --locked --bin resource_probe -- --live-text-stage-five
direnv exec . cargo run --release --locked --bin resource_probe -- --live-text-stage-seven
direnv exec . cargo run --release --locked --bin resource_probe -- --live-text-baseline
```

The first covers 64 MiB and 2 GiB imports, verified reads and one-edit saves. The
second measures production input, logic, layout and tessellation while writing
against a populated 3 GiB database; it also injects a 200 ms writer pause. These
headless measurements exclude GPU presentation and native OS input delivery.
The third preserves the original resource workload names for comparison.
Native Windows runtime validation remains pending with the user's authorization.
Strict quality-policy failures and partial evidence must be reported as such;
passing compilation/tests alone does not satisfy the independent review gate.
