# Hybrid Piece Tree Evolution Plan

Date: 2026-04-20

## Decision

Scratchpad should continue exploring the indexed hybrid path represented by `PieceTreeLite`, with the explicit goal of evolving it from a probe into a production-grade piece-tree document core.

This plan does not replace the rope direction immediately. It defines the work needed to determine whether the hybrid can become the most efficient practical design for Scratchpad.

## Why Continue The Hybrid Path

Phase 0c changed the decision surface in a meaningful way:

- the indexed hybrid outperformed `ropey` on most measured runtimes
- the indexed hybrid also maintained better allocation and peak-memory behavior across the workload set
- the remaining standout rope advantage is preview extraction, not the entire read-heavy surface

That is enough evidence to move from “interesting probe” to “serious implementation candidate.”

## End-State Goals

The production hybrid should:

- preserve piece-table semantics for efficient edit history
- provide logarithmic or near-logarithmic seek for offset and line navigation
- support character and line oriented editor operations without whole-document flattening
- keep memory usage proportional to edits rather than document size
- support viewport-oriented preview and rendering paths
- make undo operation-based rather than snapshot-based

## Target Architecture

The desired design is an indexed piece tree with these properties:

- append-only original buffer
- append-only add buffer
- balanced tree of internal nodes and leaves
- leaves containing small arrays of pieces rather than one piece per node
- cached subtree metadata for fast navigation

Required cached metadata:

- total bytes
- total chars
- total newlines
- piece count
- optional longest line or chunk hints if rendering needs it later

## Core Design Rules

### 1. Keep storage and navigation separate from UI flattening

Flattening should happen only at explicit boundaries:

- save
- export
- narrow compatibility shims
- debug or probe utilities

It should not happen implicitly during search, preview, or line navigation.

### 2. Use leaf arrays, not one piece per node

A production hybrid should avoid excessive pointer churn by storing multiple pieces per leaf. That keeps:

- tree height lower
- cache locality better
- splits and merges cheaper

### 3. Make metadata first-class

The hybrid only stays competitive if navigation does not rescan text repeatedly. Internal nodes and leaves need accurate incremental metadata updates on:

- insert
- delete
- split
- merge
- rebalance

### 4. Treat undo as structured edit history

The hybrid's biggest conceptual advantage over rope is not only storage shape. It is the ability to keep edit history close to piece semantics without full-document snapshots.

Undo records should capture:

- edited range
- inserted piece references or copied descriptors
- deleted piece references or copied descriptors
- cursor or selection before and after

## Implementation Phases

## Phase 1: Replace Probe Shortcuts With A Real Document-Core Prototype

Goal:

- turn the Phase 0c probe structure into a real reusable domain component

Work:

- move `PieceTreeLite` concepts out of the probe into a dedicated document-core module
- define explicit `Piece`, `Leaf`, and `InternalNode` types
- introduce a true tree root type rather than a top-level leaf vector
- separate storage mutation, navigation, and metadata update code paths

Exit criteria:

- the hybrid core exists as a domain module rather than only a benchmark binary

## Phase 2: Add Full Tree Balancing

Goal:

- replace probe-style leaf repacking with stable balanced-tree behavior

Work:

- implement internal-node fanout rules
- implement node split and merge behavior
- rebalance after inserts and deletes
- preserve subtree byte, char, and newline aggregates during rebalance

Exit criteria:

- offset and line seek work through balanced internal nodes
- long edit sessions do not degrade into probe-like repacking behavior

## Phase 3: Make Coordinates Character-Aware

Goal:

- support Scratchpad's editor and search semantics correctly

Work:

- track both byte and char aggregates in nodes and leaves
- define canonical coordinate rules for:
  - insert
  - delete
  - selection ranges
  - search match ranges
  - line and column reporting
- add Unicode correctness tests for multi-byte characters and combining cases

Exit criteria:

- the hybrid core can serve character-based editor operations without byte-only shortcuts

## Phase 4: Build Efficient Slice And Preview APIs

Goal:

- eliminate the remaining rope advantage on preview-style reads

Work:

- expose cheap range iterators over piece spans
- add line extraction APIs that avoid collecting whole-document intermediates
- add preview extraction helpers that walk only the local region around a match
- support bounded visible-range flattening for UI compatibility paths

Exit criteria:

- preview extraction is competitive with or better than rope in the benchmark set

## Phase 5: Introduce Operation-Based Undo

Goal:

- make the hybrid deliver its full historical-edit advantage

Work:

- replace snapshot-style undo with edit operation records
- store inserted and deleted piece sequences in undo entries
- support reverse application without full-text clones
- preserve cursor and selection restore behavior

Exit criteria:

- undo-heavy edit-history benchmarks clearly favor the hybrid in both time and memory

## Phase 6: Add Search And Metadata Integration

Goal:

- move the app's biggest non-UI whole-buffer assumptions onto the hybrid core

Work:

- route search snapshots through revision-aware descriptors instead of copied full strings
- route preview generation through piece-tree slices
- make line counting and line-ending metadata incremental
- ensure selection-scoped search does not rebuild full char buffers

Exit criteria:

- search and metadata no longer depend on whole-buffer flattening in ordinary operation

## Phase 7: Add Large-Document UI Path

Goal:

- make the hybrid matter in actual editor interaction, not only in core probes

Work:

- build visible-range extraction for the editor
- map cursor movement and selection onto hybrid coordinates
- render only visible or near-visible line ranges
- define fallback or threshold behavior for the legacy full-text path

Exit criteria:

- large documents can be viewed and edited without flattening the entire document every frame

## Efficiency Priorities

The hybrid should be optimized in this order:

1. Navigation cost
2. Edit locality
3. Preview extraction
4. Undo representation
5. Allocation pressure
6. Persistence-boundary flattening

This order follows the current evidence. The probe results already show good memory behavior. The remaining technical risk is concentrated in efficient navigation, preview APIs, and correct integration.

## Validation Plan

Each implementation phase should be validated against:

- the existing Phase 0, 0b, and 0c workload family
- many-small-edit descriptor churn tests
- Unicode and character-coordinate correctness tests
- search-preview correctness tests
- undo correctness tests
- large-document line lookup and visible-range extraction tests

The current probe binaries should remain as a stable baseline:

- [rope_text_storage_probe.rs](/D:/Code/scratchpad/src/bin/rope_text_storage_probe.rs)
- [piece_table_phase0b_probe.rs](/D:/Code/scratchpad/src/bin/piece_table_phase0b_probe.rs)
- [piece_tree_phase0c_probe.rs](/D:/Code/scratchpad/src/bin/piece_tree_phase0c_probe.rs)

## Risks

- character-aware metadata may reduce the apparent advantage seen in the byte-oriented probe
- balancing logic can become complex enough to offset some of the current runtime gains
- preview extraction may still require carefully designed bounded slice APIs
- integration cost with egui and existing search code may dominate core-structure gains if not staged carefully

## Recommendation

The next implementation step should be a real document-core prototype based on the Phase 0c indexed hybrid, with three immediate priorities:

- replace top-level leaf vectors with true internal nodes
- add char-aware metadata
- benchmark preview extraction after the new slice APIs are in place

If those three steps preserve most of the current Phase 0c advantage, the hybrid path becomes the strongest long-term storage direction for Scratchpad.
