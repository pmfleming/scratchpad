# Hybrid Storage Probe Report

Date: 2026-04-20

## Purpose

This document consolidates the text-storage probe work across:

- Phase 0 rope baseline
- Phase 0b flat piece-table prototype
- Phase 0c indexed piece-tree prototype

Its goal is to document:

- what was tested
- how the probes were run
- what each phase actually measured
- what conclusions are safe to draw from the current data

## Probe documents

- [rope-text-storage-phase0.md](/D:/Code/scratchpad/docs/rope-text-storage-phase0.md)
- [piece-table-phase0b.md](/D:/Code/scratchpad/docs/piece-table-phase0b.md)
- [piece-tree-phase0c.md](/D:/Code/scratchpad/docs/piece-tree-phase0c.md)

## Probe binaries

- [rope_text_storage_probe.rs](/D:/Code/scratchpad/src/bin/rope_text_storage_probe.rs)
- [piece_table_phase0b_probe.rs](/D:/Code/scratchpad/src/bin/piece_table_phase0b_probe.rs)
- [piece_tree_phase0c_probe.rs](/D:/Code/scratchpad/src/bin/piece_tree_phase0c_probe.rs)

## Common workload matrix

Each probe measured the same core workload family unless noted otherwise:

- file-backed load for `32 MB`, `128 MB`, and `512 MB`
- mid-document insert of `8 MB` and `64 MB` into `1 MB` and `32 MB` base documents
- search preview extraction around a match near the end of a large document
- line lookup near the end of a large document

Additional workloads by phase:

- Phase 0: none beyond the common matrix
- Phase 0b: undo-heavy edit history with repeated inserts followed by reverse-order undo
- Phase 0c: same undo-heavy edit-history workload as Phase 0b

## How the probes were run

Commands:

```powershell
cargo run --release --bin rope_text_storage_probe
cargo run --release --bin piece_table_phase0b_probe
cargo run --release --bin piece_tree_phase0c_probe
```

Each probe emits JSON lines with:

- per-storage measurements
- per-workload comparisons
- a final decision summary

Metrics captured:

- elapsed time
- allocated bytes
- peak live bytes
- allocation, deallocation, and reallocation counts

## Implementations compared

### Phase 0

- `String`
- `ropey`

### Phase 0b

- `String`
- `ropey`
- `PieceTableLite`

`PieceTableLite` characteristics:

- piece-table semantics
- original and add buffers
- flat `Vec<Piece>` descriptor storage
- no indexed tree

### Phase 0c

- `String`
- `ropey`
- `PieceTreeLite`

`PieceTreeLite` characteristics:

- piece-table semantics
- original and add buffers
- indexed leaves
- cached byte and newline metadata per leaf
- byte-oriented probe assumptions
- still not a full balanced production piece tree

## Observed results

### Phase 0: rope versus contiguous string

Documented in [rope-text-storage-phase0.md](/D:/Code/scratchpad/docs/rope-text-storage-phase0.md).

Observed outcome:

- `ropey` was dramatically better for search preview extraction
- `ropey` was dramatically better for near-end line lookup
- `String` remained better for file-backed load
- large inserts were mixed

Safe conclusion:

- rope is strongly validated for read-heavy line-aware operations
- rope is not a blanket improvement over contiguous string on every workload

### Phase 0b: flat piece table versus rope

Documented in [piece-table-phase0b.md](/D:/Code/scratchpad/docs/piece-table-phase0b.md).

Observed outcome:

- `PieceTableLite` beat `ropey` on allocation volume across the measured set
- `PieceTableLite` beat `ropey` on peak memory in almost every workload
- `ropey` still dominated preview extraction
- `ropey` still dominated line lookup
- `ropey` also stayed faster on undo-heavy edit history

Safe conclusion:

- flat piece-table semantics are genuinely attractive for memory behavior
- a flat descriptor list is not competitive enough on elapsed time
- Phase 0b did not justify changing the default rope recommendation

### Phase 0c: indexed piece tree versus rope

Documented in [piece-tree-phase0c.md](/D:/Code/scratchpad/docs/piece-tree-phase0c.md).

Observed outcome:

- `PieceTreeLite` beat `ropey` on elapsed time in `11/14` workloads
- `PieceTreeLite` beat `ropey` on allocation volume in `14/14` workloads
- `PieceTreeLite` beat `ropey` on peak live memory in `12/14` workloads
- `ropey` remained fastest for both search preview extraction workloads
- `PieceTreeLite` became fastest for all measured line-lookup workloads
- undo-heavy edit history became mixed rather than rope-dominated

Safe conclusion:

- indexing the piece descriptors materially changes the outcome
- the hybrid path is now performance-credible rather than merely memory-efficient
- preview extraction remains the most visible remaining rope advantage in the current probe set

## Cross-phase interpretation

The probe sequence tells a consistent story:

1. Rope was a strong baseline because it handled read-heavy editor workloads well.
2. A flat piece table improved memory behavior but was too slow to challenge rope.
3. An indexed piece-tree-style hybrid closed or reversed much of the runtime gap while preserving better memory behavior.

That means the key variable was not “rope versus piece table” in the abstract. It was whether the piece-table path had strong enough indexing and metadata support.

## Current recommendation

Based on the probes alone:

- `ropey` remains the strongest mature off-the-shelf implementation
- the indexed hybrid is now strong enough to justify deeper design and implementation work
- the next decision should be driven by production constraints, not by assuming the hybrid is still speculative

Those production constraints include:

- Unicode and character-coordinate correctness
- real undo structure
- multi-level balancing behavior under long edit sessions
- integration cost with search, preview, rendering, and persistence

## Open questions

- Does the indexed hybrid keep its advantage once it becomes fully character-aware rather than byte-oriented?
- Can preview extraction be optimized enough to erase rope's remaining lead on that workload?
- Does a real balanced tree keep the same gains once node splitting, merging, and rebalancing are fully modeled?
- How much complexity is acceptable relative to the maturity and simplicity of `ropey`?

## Exit criteria for moving beyond probes

The hybrid path should move from probe to implementation candidate when all of the following are true:

- the indexed design is specified clearly enough to implement without probe shortcuts
- character and line semantics are defined precisely
- undo records are operation-based rather than snapshot-based
- the search and preview path can use the hybrid without flattening whole documents
- the implementation plan includes validation against the same workloads captured here
