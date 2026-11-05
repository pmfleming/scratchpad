# Stage 1: temporary backing and fallible text access

Standard WAL is unchanged. This stage deliberately retains database schema 11 and
recovery-export version 9. Native Windows runtime validation remains pending under
the user's authorization; it is a release acceptance item.

The original and add caches now share aggregate and per-document budgets. Chunk
loads coalesce, perform positioned I/O without cache/state locks held, and verify
length, BLAKE3, and UTF-8. Text pins keep bodies alive through eviction. A failed
spill retains its accepted body in RAM. The next stage replaces the temporary
backing scaffold with the shared chunk service.

Viewport layout, edits, history replay, scanning, hashing, analysis, and disk save
have fallible read boundaries. A failed viewport read paints a read-only error
outside the document. Failed edits/reloads preserve text, history position, dirty
metadata, and the existing save destination. Open history groups reuse their
retained operation record instead of performing a new backing read after mutation.

Compaction, unused provenance, live persisted flags, and redundant live history
visible generations are removed. History offsets and lengths are u64; a regression
test covers an offset above u32::MAX without a multi-gigabyte allocation. Stable
layout IDs travel through capture and cold hydration. The current wire format is
kept until the next stage. Startup no longer adopts appearance settings from the
session. Disk observation hashes decoded text without adopting/importing it.

## Responsiveness evidence

`evidence/stage-1/resource.jsonl` contains three 64 MiB production-window search
scans, three typing samples without a save, and three typing samples during a
50 MiB session save, each with 128 edits and the frame's health query. The producer
was the stage-1 resource probe before the final reload/export correctness fixes;
those fixes do not change the measured health-query path.

The three saves took 623.374, 108.784, and 92.998 ms. Their maximum edit plus health
query times were 0.057, 0.070, and 0.187 ms. The stage-0 producer measured 644.462,
110.081, and 94.486 ms on that path because the health query queued behind the
save. The shared atomic health latch removes that dependency. These are component
measurements, not complete rendered-frame latency measurements. End-to-end frame
checks and the final workload matrix remain stage 7 acceptance requirements.

The 50 ms writer budget remains a scheduling target. Kernel I/O and engine
checkpoint overruns are recorded rather than interpreted as a failed correctness
gate. The UI must not wait on writer completion.

## Validation

The full all-feature suite passed, including 667 library tests, integration tests,
and doctests (intentionally ignored tests remain ignored). All-target/all-feature
Clippy passed with warnings denied; formatting, the Linux target check, and the
Turso lock-identity check passed. The Windows target is not installed, so its
cross-check was explicitly skipped. Audit passed with eight existing allowed
unmaintained/dependency warnings. All 43 resource baseline samples passed,
including files through 2 GiB; the complete events are in `evidence/stage-1/baseline.jsonl`.

Fresh RQL is also running. The initial attempt overlapped correctness fixes and
cannot constitute stable-source evidence. A separate immutable stage-1 checkout
is used for the final run so implementation can continue. Its local archive is
`target/quality-runs/live-text-stage-1-frozen`. RQL's known strict evidence/review
gaps must be reported; no policy limits are raised or suppressed.
