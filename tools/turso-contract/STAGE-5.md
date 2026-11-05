# Stage 5: streaming imports

Large UTF-8 adoption now runs UTF-8-adjusted CDC (16/64/256 KiB) directly into
explicit session-store handles on the path lane. There is no temporary text
file. Each chunk receives piece metrics; one streaming inspector computes the
decoded digest, line endings, controls, and encoding metadata. Normal import
intake waits at the 32 MiB pending watermark, with cancellation checked before
reads/publication and during capacity waits. Stage 6 supplies degraded RAM
admission and lifecycle cancellation. Invalid UTF-8 refuses adoption atomically.
Other encodings retain the existing RAM decoder, as scoped by the plan.

Initial history and full-piece reference ranges use stored character/byte
metrics without payload reads. Runtime history no longer hashes entire payloads;
recovery exports still compute their integrity fingerprints. Owned originals use
CDC on their first worker-side durability capture. Retained reopen compares a
streamed digest before importing a changed source. Dirty or unchanged observations
create no chunks. Original prefetch now uses the store readers. The old temporary
backing/cache implementation was removed; test faults inject an unreadable range,
while physical database corruption remains covered by chunk-service tests.

The production 64 MiB / 2 GiB probe is retained in `evidence/stage-5/import.log`:

| Measurement | 64 MiB | 2 GiB |
| --- | ---: | ---: |
| First visible-window paint | 4.13 ms | 2.87 ms |
| Complete import | 179 ms | 6.03 s |
| Verified random read p99 | 0.680 ms | 0.549 ms |
| Maximum verified read | 0.898 ms | 0.859 ms |
| Peak pending bodies | 524,287 B | 1,572,863 B |
| One edit and manifest commit | 4.61 ms | 11.56 ms |

The fixture repeats Unicode text, so deduplication is intentionally strong; these
are not worst-case disk-throughput results. The edit appends an 11-byte UTF-8 tail
and hashes zero new sealed bodies; original chunks are not rewritten. The first
paint uses the same bounded production preview path as the baseline, before full
hydration. Final resource comparisons and non-repeating workloads remain stage 7.
Library tests cover source replacement after import, bounded cache/pins, unchanged
large-file observations, cancellation, invalid CDC input, Unicode/CRLF, and exact
history. Native Windows execution remains pending by user authorization.

Stage 4's frozen quality producer/verifier passed, but the strict gate failed on
partial evidence and three required human reviews. No failing test or reliability
finding is reported; those policies remain incomplete due to partial execution
evidence. The compact report is `evidence/stage-4/quality-summary.json`. No policy
or performance budget was raised or suppressed.

Stage 5 validation passed: the complete all-feature release test suite (683 library
passes, seven existing ignores, integration and documentation tests), formatting,
and all-target/all-feature Clippy with warnings denied. Fresh quality evidence is
captured from the stage commit; the strict review/evidence gate remains a separate
acceptance condition.
