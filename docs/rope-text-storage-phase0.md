# Rope Text Storage Phase 0

Date: 2026-04-20

Phase 0 is implemented as a decision-spike probe that compares contiguous `String` storage against a rope-backed candidate using the workloads called out in [rope-text-storage-plan.md](/D:/Code/scratchpad/docs/rope-text-storage-plan.md).

## Chosen rope implementation

The spike standardizes on `ropey` as the rope candidate to measure and carry forward into the migration.

Reasons:

- it is a mature Rust rope implementation with character and line oriented APIs
- it supports file-backed loading through `Rope::from_reader`
- it maps cleanly to the migration plan's insert, preview, and line lookup needs

## Probe entry point

Run:

```powershell
cargo run --release --bin rope_text_storage_probe
```

The probe emits JSON lines so results can be redirected into a file or diffed across runs.

Phase 0b now lives alongside this baseline in [piece-table-phase0b.md](/D:/Code/scratchpad/docs/piece-table-phase0b.md).

## Workloads covered

- file-backed load for `32 MB`, `128 MB`, and `512 MB`
- inserting `8 MB` and `64 MB` into the middle of `1 MB` and `32 MB` documents
- search preview extraction around a match near the end of a large document
- line lookup near the end of a large document

Each workload emits:

- one measurement event for `String`
- one measurement event for `ropey`
- one comparison event with elapsed-time and allocation ratios
- one final decision summary event across the whole run

## What to look for

Phase 0 is successful when the comparison output shows rope wins clearly on the mutation and read-heavy workloads that motivated the plan, especially:

- mid-document insert
- search preview extraction
- line lookup near the end of large buffers

Load behavior is still important, but it should be interpreted together with allocation and peak live memory, not just raw elapsed time.

## Observed baseline from the first full run

The first full local run on 2026-04-20 showed a split result rather than a blanket win:

- `ropey` was dramatically better for search preview extraction
- `ropey` was dramatically better for line lookup near the end of large buffers
- `String` remained better for file-backed load in both elapsed time and peak allocation
- large single inserts were mixed: `ropey` won clearly for `32 MB + 8 MB`, while `String` still won the other insert cases in the first run

That means Phase 0 now gives the project a concrete measurement baseline. It confirms rope as a strong fit for the search-heavy and line-oriented parts of the editor, while also making the remaining load and bulk-insert tradeoffs explicit.
