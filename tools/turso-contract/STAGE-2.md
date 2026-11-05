# Stage 2: typed metadata and shared chunk service

This is an intermediate development commit on the stages 2–4 branch. It uses
schema 12 / session export version 10 and fresh databases only. Schema 11 is
rejected explicitly; there is no migration or alternate persistence reader.
The existing snapshot/history paths remain until stage 4 replaces them together.

Document and workspace metadata is now typed. Only recursive pane trees remain
JSON in layouts, and quarantine remains a frozen JSON archive. Live and closed
workspaces share stable layout identities and separate document/view tables.
Paths are checked as UTF-8 instead of altered with lossy conversion. Appearance
settings and redundant format/view compatibility fields are removed from the
session model. Startup reserves runtime IDs with indexed typed queries. Usage
reports label typed metadata sizes as estimates.

Layout/document rows are compared directly. A cursor-only capture updates one
view row. Closing the first of three tabs into an archive updates no layout,
document, or view row; remaining layout IDs and the archived ID are preserved.
Bulk reads use one query per metadata table. Unrelated closed layouts are not
read during a warm save. Layout positions are deleted before reordered owners
are inserted, and unowned layouts are removed in the same commit.

The explicit `ChunkStoreHandle` has pending and committed shared handles, a
bounded aggregate/per-document cache, coalesced verified reads, and two reader
threads with separate bounded queues. The foreground deadline is 250 ms; the
background deadline is five seconds. Reader panics disconnect private replies
and cannot unwind the caller. No cache or chunk-state lock is held during I/O.

The existing database actor alone drains chunk writes. Wakes are nonblocking and
coalesced. It writes at most four chunks or 4 ms of work before checking queued
jobs again, records successful batches over the approved 50 ms scheduling target,
and retries failures with 100 ms–5 s backoff. Only committed bodies become
evictable. Failed/ambiguous writes retain accepted bodies in RAM. Chunk commits
use NORMAL and restore FULL before manifests; a failure to restore FULL blocks
future writes. Successful batches update the inventory's expected change count.
Chunk-only databases do not require a session marker, and chunk collection stays
off until document roots exist in stage 4.

## Evidence

All-feature tests passed, including 672 library tests (eight intentional ignores),
integration tests, and doctests. Regression coverage includes pending/committed/
evicted/pinned Unicode, digest damage, a failed body transaction followed by an
idempotent retry, FULL restoration, reader panic isolation, and queued work
following at most one four-chunk batch. All-target Clippy and formatting are run
at the commit boundary. Native Windows runtime validation remains pending.

`evidence/stage-2/metadata-measurement.log` records the full real schema/model with
10,000 empty tabs: initial save 1008 ms, steady save 87 ms, shell restore 121 ms,
startup reservation/open 4 ms, and closing the first tab after reopen 715 ms.
These include the unchanged old text/history path and are not directly comparable
to stage 0's structural metadata prototype. Stage 4 must remove history-payload
work before judging final save cost.

Fresh RQL completed against immutable commit `d769a41`, with matching source
hashes. Measurement, verification, and audit/feature tool-version discovery commands passed.
The strict gate remains failing on partial analysis, existing hotspot findings,
and three pending required human reviews. No new chunk-store/document-row/layout
module exceeded a hotspot limit. The compact summary is in
`evidence/stage-2/quality-summary.json`; the complete local archive is
`target/quality-runs/live-text-stage-2-frozen`. No budgets were raised or findings
suppressed.
