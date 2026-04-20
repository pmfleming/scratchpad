# Piece Table Phase 0b

Date: 2026-04-20

Phase 0b adds a comparison probe for a piece-table-style storage model alongside the existing `String` and `ropey` Phase 0 baseline.

## Probe entry point

Run:

```powershell
cargo run --release --bin piece_table_phase0b_probe
```

The probe emits JSON lines for:

- `String`
- `ropey`
- `PieceTableLite`

## Scope

The Phase 0b probe covers the same workload shapes as Phase 0:

- file-backed load for `32 MB`, `128 MB`, and `512 MB`
- inserting `8 MB` and `64 MB` into the middle of `1 MB` and `32 MB` documents
- search preview extraction around a match near the end of a large document
- line lookup near the end of a large document

It also adds one extra Phase 0b workload:

- undo-heavy edit history with repeated inserts followed by reverse-order undo

## Important limitation

`PieceTableLite` is intentionally a narrow prototype:

- it uses a piece-table layout with original and add buffers
- it stores descriptors in a `Vec<Piece>`
- it does not implement a weighted B-tree or production piece tree

That means this probe is useful for checking whether piece-table semantics improve edit-history behavior enough to justify deeper work, but it is not a final verdict on a fully indexed piece-tree design.

## How to interpret it

If `PieceTableLite` clearly beats `ropey` on undo-heavy history and stays competitive on insert-heavy workloads, that is evidence the extra complexity may be worth a real indexed prototype.

If `ropey` still dominates preview and line-lookup workloads while the piece-table prototype only wins on undo, the result argues for keeping rope as the default direction unless the project decides undo-history cost is the primary bottleneck.

## Observed baseline from the first full run

The first full local run on 2026-04-20 showed a mixed but useful result:

- `PieceTableLite` beat `ropey` on allocation volume for every measured workload
- `PieceTableLite` beat `ropey` on peak live memory for almost every workload
- `ropey` remained decisively faster for search preview extraction
- `ropey` remained decisively faster for near-end line lookup
- `ropey` also stayed faster on the undo-heavy edit-history workload, even though the piece-table prototype used less memory

That means Phase 0b does not overturn the rope direction. It does show that piece-table semantics are genuinely attractive for memory behavior, but the narrow prototype did not buy enough elapsed-time improvement over `ropey` on the combined workload mix to justify changing the current recommendation.

Phase 0c now follows this with an indexed hybrid probe in [piece-tree-phase0c.md](/D:/Code/scratchpad/docs/piece-tree-phase0c.md).
