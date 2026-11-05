# Stage 6: degraded operation and lifecycle

Manifest failures now participate in store health alongside body failures. Open
failures expose an explicit RAM-only, Unavailable store; accepted chunks remain
readable without a spill file. Writer protection latches publish Unavailable
before replying, and reader disconnection faults its owner and marks the store
Unavailable. Per-owner read faults also reach documents after background reads.
Canonical restored handles validate metadata and remain scoped to their owner.

History settings add an automatic failure RAM limit: 25% of total RAM with a
256 MiB minimum, using 2 GiB if discovery fails. Startup recomputes automatic
values and clamps explicit overrides; the UI permits 64 MiB through total RAM.
Changes apply immediately. The degraded count includes pending sealed bodies,
mutable tails, deferred text events, and large RAM-owned originals; committed
cache and ordinary small originals are excluded. Bulk preparation and import
check admission; typing continues above the limit. Unpublished abandoned chunks
are reaped without dropping any chunk still owned by a document or operation.

A persistent storage segment follows Status history, including in Settings and
empty workspaces. It truncates at narrow widths and exposes cause/usage/limit on
hover. State/pressure transitions are recorded in Session history. Affected
buffers use a red circle through shared tab chrome; disk problems remain squares.
Capture acknowledgements carry weak tree/revision identities, so an older save
cannot clear a newer edit. Initial restored revisions are acknowledged without
loading bodies. Healthy frames avoid scanning every tab unless a reader reports
a new fault.

Start new session is gated by Unavailable, including a running writer latch.
Reset closes readers and writer, backs up the complete file set, then reopens the
old read epoch against the backup. Live and history-only chunks are copied in
bounded batches; old handles are remapped only after the new FULL manifest
commits. Cold captured documents are hydrated as references before replacement.
A failed backup reopens the previous readers; retired-writer reset can be retried.
Failed reset keeps saving paused and accepted text reachable. Editor events
arriving during reset are deferred for replay afterward.

Exit waits for prepared/deferred accepted input, stops further import intake,
performs the final manifest, closes readers, and checkpoints. Failure keeps the
window open where retry is possible. An explicitly reused SessionStore can reopen
its readers after shutdown. Ordinary document handles can still outlive their
store, as established in stage 4.

Recovery export includes open buffers, retained documents and closed layouts.
Current text and history JSON are streamed with at most one chunk escaped at a
time; the manifest is written last. Damaged/quarantined or incompatible sources
fail explicitly. Recovery JSON identifies its BLAKE3-u64-le fingerprint algorithm;
this is an explicit export, never an automatic import or migration path.

Validation passed on Linux: 691 library tests, seven existing ignores, the full
all-feature integration/documentation suite, formatting and all-target/all-feature
Clippy with warnings denied. Added cases cover admission/continued typing, stale
acknowledgements, read faults, a large live/history reset, failure/readability/retry,
streamed Unicode JSON/replay, retained export, and the narrow Settings/empty status
bar. Final large-action responsiveness and scaffold cleanup remain stage 7. Native
Windows runtime tests remain pending by the user's authorization.

The stage 5 frozen quality producer/verifier passed with stable source hashes.
The strict gate remains failed for partial evidence and three required human
reviews, with additional reported threshold findings retained in the compact
report. No test failure or reliability finding is reported by that run. See
`evidence/stage-5/quality-summary.json`; no rule or budget was relaxed.

The subsequent stage 6 frozen quality run also passed producer/verification,
tool-version discovery for feature, audit and coverage tools with unchanged source
hashes. These discovery commands alone are not feature or security scans. Its strict
policy gate remains failed: 14 partial-evidence artifacts, three required human
reviews and 39 reported threshold findings. No actual test failure or reliability
finding is reported. See `evidence/stage-6/quality-summary.json`; the policy and
thresholds remain unchanged.
