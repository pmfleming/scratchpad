# Piece Tree Phase 0c

Date: 2026-04-20

Phase 0c adds an indexed hybrid probe that keeps piece-table semantics but replaces the flat descriptor list with leaf-indexed piece storage.

## Probe entry point

Run:

```powershell
cargo run --release --bin piece_tree_phase0c_probe
```

## Prototype shape

`PieceTreeLite` is still a probe, but it is materially closer to the hybrid design we actually care about:

- original and add buffers, piece-table style
- leaf-indexed storage instead of one flat `Vec<Piece>`
- cached byte and newline metadata per leaf
- logarithmic leaf seek by byte or line prefix
- localized inserts into affected leaves, with repacking when needed

It is not yet a full production piece tree:

- no multi-level balanced B-tree nodes
- no persistent node sharing
- no piece-range undo objects yet
- byte-oriented and ASCII-oriented probe assumptions

## Workloads covered

- file-backed load for `32 MB`, `128 MB`, and `512 MB`
- inserting `8 MB` and `64 MB` into the middle of `1 MB` and `32 MB` documents
- search preview extraction around a match near the end of a large document
- line lookup near the end of a large document
- undo-heavy edit history with repeated inserts followed by reverse-order undo

## How to interpret it

Phase 0c is the fairer test of the hybrid idea than Phase 0b.

If Phase 0c narrows the elapsed-time gap with `ropey` while preserving the better memory profile of piece-table semantics, that is evidence the hybrid path deserves deeper implementation work.

If `ropey` still dominates preview, line lookup, and edit-history runtime even after indexing the piece descriptors, then rope remains the stronger default direction and the piece-tree path should be treated as a specialized alternative rather than the mainline plan.

## Observed baseline from the first full run

The first full local run on 2026-04-20 showed a materially different picture from Phase 0b:

- `PieceTreeLite` beat `ropey` on elapsed time in `11/14` measured workloads
- `PieceTreeLite` beat `ropey` on allocation volume in `14/14` workloads
- `PieceTreeLite` beat `ropey` on peak live memory in `12/14` workloads
- `ropey` still remained the fastest option for both search preview extraction workloads
- `PieceTreeLite` became the fastest option for all measured line-lookup workloads
- undo-heavy edit history was mixed: `ropey` won the smaller `8 MB` base case, while `PieceTreeLite` narrowly won the `32 MB` base case

That means Phase 0c is the first probe that makes the indexed hybrid look genuinely competitive rather than merely memory-efficient. The result is still not a blanket replacement decision, because the prototype remains byte-oriented and probe-scoped, but it is strong enough to justify deeper work on the hybrid path.
