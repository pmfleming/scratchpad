# Rope Text Storage Plan

Date: 2026-04-19

## Decision

Scratchpad should adopt a rope-backed text representation as the canonical document storage model.

This is the best fit among a rope, piece table, or other chunked representation for the current codebase.

## Why Rope Is The Best Fit

Scratchpad is not just a text mutator. It is also a:

- line-counting editor
- search-heavy editor
- multi-view editor
- session-persisted editor
- highlight-rendering editor

Those characteristics matter because the storage model needs to support efficient reads, slicing, line access, and chunk iteration just as much as efficient edits.

A rope is the best fit here for five reasons.

### 1. Rope matches the dominant access patterns better than a piece table

The current app repeatedly needs:

- character-range addressing
- line counting
- line-oriented preview generation
- chunked scanning for search
- partial rendering in the future

A rope naturally supports these operations. A piece table is strongest when the main goal is cheap mutation history over original and add buffers, but it usually needs extra indexing structure to become efficient for line-based random access and repeated scans.

Scratchpad's pain is not only insert cost. It is also full-text scanning, search snapshots, and rendering. Rope addresses more of that surface directly.

### 2. Rope is a better foundation for large-file viewing and viewport rendering

The current editor path still assumes contiguous full-text access, but the long-term fix for large files requires:

- chunk iteration
- visible-range extraction
- line-to-char lookup without flattening the whole document

That future aligns better with a rope than with a piece table.

### 3. Rope keeps undo as a separate concern instead of entangling it with storage

The current undo model stores whole `String` snapshots. That is already too expensive for large buffers.

A piece table would make storage and history more tightly coupled, but Scratchpad still needs to solve search, rendering, and metadata separately. Rope allows the project to make document storage scalable first, then move undo to operation-based or slice-based history in a second step.

That separation is healthier for this codebase.

### 4. Rope is easier to align with Rust editor practice

In Rust, rope-backed editor cores are a more common and better-understood path than maintaining a custom piece-table-plus-index-tree implementation.

That matters for:

- lower implementation risk
- easier reasoning about correctness
- simpler benchmarking against established behavior
- less custom infrastructure to maintain

### 5. Rope gives the broadest improvement against the current bottlenecks

The current bottlenecks are not just editing performance. They include:

- full-file load and decode into a single string
- whole-document metadata scans
- full-text search snapshots copied to the worker
- whole-document layout through egui text editing
- whole-document undo snapshots

Rope does not solve all of these by itself, but it is the best storage substrate for fixing them in a coherent way.

## Why Not Choose A Piece Table

A piece table is a valid editor design, but it is not the best first move here.

Reasons not to choose it as the primary direction:

- Scratchpad needs efficient line and slice access more broadly than it needs original/add-buffer history semantics.
- A piece table still needs indexing layers for line lookup, search previews, and rendering.
- The current UI stack still expects contiguous text in key places, so a piece table would not avoid the widget migration problem.
- The repo does not already have the supporting infrastructure that makes a piece table especially attractive.

In short: a piece table would solve one important problem well, but rope is a better match for the full set of problems visible in the current code.

## Current Constraints In The Codebase

Before implementation starts, the migration should be framed around the actual constraints in the current code.

### 1. `TextDocument` is currently a single `String`

Current storage is centralized in:

- `src/app/domain/buffer/document.rs`
- `src/app/domain/buffer/state.rs`

Today the document model assumes:

- full contiguous text access
- in-place insert and delete over a `String`
- direct `&str` access for many consumers

### 2. The editor widget currently requires contiguous full-text access

The current editor path uses `egui::TextEdit` with `egui::TextBuffer`.

Relevant code:

- `src/app/ui/editor_content/text_edit.rs`

Important constraint:

- the layouter calls `buf.as_str()` and builds layout jobs over the whole document

That means a storage swap alone will not deliver the expected scalability gains. Large-document editing also requires a rendering and widget strategy that does not insist on whole-document flattening every frame.

### 3. Search currently copies full text into worker snapshots

Relevant code:

- `src/app/app_state/search_state/worker.rs`

Current behavior:

- `SearchTargetSnapshot` owns a `String`
- selection-scoped search builds substrings by collecting chars into a `Vec<char>` and then a new `String`

That is not compatible with large-buffer scaling.

### 4. Undo currently stores whole-text snapshots

Relevant code:

- `src/app/domain/buffer/document.rs`

Current behavior:

- undo state includes a full `String`

That means large edits remain expensive even if storage changes, unless undo is migrated too.

### 5. Session persistence currently serializes full buffer text

Relevant code:

- `src/app/services/session_store/mod.rs`

Current behavior:

- each persisted buffer is written out as full text bytes

This is acceptable at the persistence boundary, but the migration must ensure flattening happens only at save or export boundaries, not throughout editing.

## Goals

The migration should achieve these goals.

### Primary goals

- make large-file open and edit scalability materially better
- make large paste operations avoid full-document shifting costs
- make search and preview generation work from chunks rather than copied full strings
- stop whole-document undo snapshots from dominating memory usage
- create a path to viewport-based rendering for large documents

### Secondary goals

- preserve character-based coordinates for search and replace
- preserve session restore semantics
- preserve encoding-aware load and save behavior
- keep existing buffer, tab, and transaction concepts stable where possible

### Non-goals

- rewriting the entire app state layer in one pass
- changing search semantics during the storage migration
- shipping a custom syntax system as part of this work

## Target Architecture

The end state should look like this.

## 1. `TextDocument` becomes a facade over rope-backed storage

`TextDocument` should remain the public document abstraction used by `BufferState`, but internally it should no longer be a raw `String`.

Target responsibilities:

- own rope-backed text storage
- expose character-oriented edit APIs
- expose efficient slice and chunk iterators
- provide line and char lookup helpers
- flatten only at explicit boundaries

Suggested shape:

```rust
struct TextDocument {
    storage: RopeStorage,
    undo: DocumentUndoHistory,
    metadata: DocumentMetadataCache,
}
```

## 2. Metadata becomes incremental or chunk-aware

Line count, line endings, and artifact summary should stop requiring whole-document rescans after every edit.

The target state is:

- cheap metadata reads
- edit-driven metadata invalidation
- chunk-based rescans only where necessary
- optional deferred artifact analysis for very large buffers

## 3. Search stops owning copied `String` snapshots by default

Search should move from whole-buffer string snapshots to a plan based on:

- stable buffer revision identifiers
- rope slices or flattened chunk windows
- preview extraction from the document facade

The worker should receive only what it needs, not a full duplicate of every searched buffer.

## 4. The editor UI splits into small-document and large-document paths during migration

This is the most important practical migration decision.

Because `egui::TextEdit` currently expects full contiguous text access, the migration should not try to force rope into the current widget unchanged.

Instead:

- keep the current `egui::TextEdit` path temporarily for smaller documents
- add a rope-aware large-document path that renders visible lines from slices or chunks
- converge later once the rope-aware path is mature enough to replace the old one broadly

This phased approach reduces risk.

## 5. Undo becomes operation-based or slice-based

The current whole-text snapshot undo model must be retired for large-document correctness and memory discipline.

The replacement should store:

- edited range
- inserted text or rope slice
- deleted text or rope slice
- cursor state before and after the edit

That preserves current UX while avoiding full-buffer snapshots.

## Migration Plan

## Phase 0: Decision Spike And Benchmarks

Goal:

- validate the rope choice with a narrow design spike before broad refactoring

Work:

- choose the rope implementation to standardize on
- benchmark core operations against current `String` storage
- benchmark these workloads specifically:
  - load 32 MB, 128 MB, and 512 MB text
  - insert 8 MB and 64 MB into the middle of a 1 MB and 32 MB document
  - preview extraction around search matches
  - line lookup near the end of a large document
- confirm expected memory behavior under representative loads

Exit criteria:

- rope is confirmed as materially better on target workloads
- exact crate or internal implementation is chosen

## Phase 1: Introduce A Storage Abstraction Seam

Goal:

- decouple the rest of the app from raw `String` assumptions without changing behavior yet

Work:

- define a document-storage interface inside the buffer domain
- route current document reads through the facade instead of direct string access
- inventory all direct `buffer.text()` and `document.as_str()` assumptions
- isolate byte, char, line, and preview helpers behind document methods

Important rule:

- this phase should preserve current behavior and may still be backed by `String`

Exit criteria:

- app logic depends on `TextDocument` capabilities, not on raw `String` layout details

## Phase 2: Add Rope-Backed Storage Behind `TextDocument`

Goal:

- make rope the canonical document representation without changing editor UX yet

Work:

- implement rope-backed insert, delete, replace, slice, and line APIs
- preserve character-based coordinate behavior
- implement flatten-on-demand helpers only where still required
- keep a narrow compatibility layer for current callers that still need `&str`

Important rule:

- compatibility flattening must be explicit and temporary
- no hidden flatten-on-every-call fallback

Exit criteria:

- documents can be stored and edited as rope-backed content through existing domain APIs

## Phase 3: Move Metadata, Search, And Preview Logic Off Full Strings

Goal:

- remove the largest non-UI whole-buffer costs

Work:

- migrate line-count and line-ending analysis to chunk-aware processing
- make artifact detection incremental or deferred
- replace `SearchTargetSnapshot.text: String` with revision-aware target descriptors
- generate previews directly from document slices
- eliminate selection-search substring construction through full char collection

Exit criteria:

- search and metadata no longer require whole-buffer string copies for ordinary operation

## Phase 4: Introduce A Rope-Aware Editor Path

Goal:

- stop relying on full-document `egui::TextEdit` behavior for large buffers

Work:

- define a large-document editing and rendering path that works from visible slices
- render only the visible or near-visible line range
- map cursor movement and selection to rope character coordinates
- preserve search highlight rendering using visible-range translation
- decide the threshold or conditions for switching from legacy editor path to rope-aware path

Important rule:

- this phase is required for the migration to deliver meaningful capacity wins
- a storage swap without a rendering change is not sufficient

Exit criteria:

- large documents can be viewed and edited without flattening the entire document each frame

## Phase 5: Replace Whole-Text Undo Snapshots

Goal:

- make undo scale with edits rather than document size

Work:

- introduce operation-based undo records
- store deleted and inserted ranges as text slices or owned chunk data
- preserve cursor and selection restore behavior
- keep undo depth limits, but base memory usage on edit history rather than full copies

Exit criteria:

- undo cost is proportional to the change, not the full document size

## Phase 6: Update Persistence, Testing, And Large-File UX

Goal:

- complete the migration and make large-buffer behavior predictable to users

Work:

- ensure save and session persistence flatten only at the persistence boundary
- add large-file warnings or a dedicated large-file mode if needed
- add coverage for large-document search, replace, undo, session restore, and multi-view behavior
- refresh capacity probes and compare results against the current baseline

Exit criteria:

- the rope-backed editor path is production-ready
- capacity improvements are demonstrated with the existing measurement scripts

## Testing Plan

The migration should be validated at four levels.

### 1. Document-core tests

- insert and delete by character range
- Unicode boundary correctness
- line lookup correctness
- slice extraction correctness
- flattening correctness at persistence boundaries

### 2. Buffer and metadata tests

- line count remains correct after edits
- line-ending metadata remains correct after edits
- artifact flags remain correct after edits
- large-buffer edits do not trigger full rescans where they should not

### 3. Search and replace tests

- search results remain character-accurate
- replace ranges still apply in reverse-safe order
- preview generation remains correct across chunk boundaries
- selection-only search remains correct without substring copies

### 4. Capacity regression tests

- file-size ceiling improves from the current baseline
- paste-size ceiling improves from the current baseline
- tab-count ceiling does not regress due to buffer representation changes
- session restore remains acceptable with large buffers

### 5. Profiling and capacity suite additions

The migration test suite should explicitly add these measurement tracks:

- allocation profiling for large-file open
- allocation profiling for large paste into a large buffer
- working-set and page-fault tracking while scaling tab count
- real file-backed large-file tests, not only synthetic in-memory probes
- session persist and restore cost with hundreds or thousands of tabs

These should be treated as required regression instruments, not optional follow-up diagnostics.

## Risks And Mitigations

## Risk 1: Widget migration becomes harder than storage migration

This is likely.

Mitigation:

- explicitly separate storage migration from UI migration
- keep a temporary small-document compatibility path
- treat the rope-aware editor path as a first-class phase, not follow-up cleanup

## Risk 2: Hidden flattening keeps the app slow

This is the most likely way to miss the real benefit.

Mitigation:

- ban implicit whole-document flattening in hot paths
- log and benchmark every compatibility flattening call during migration

## Risk 3: Undo semantics regress

Mitigation:

- preserve cursor state and edit ordering in explicit undo tests
- move undo to operation-based storage before removing the current fallback path

## Risk 4: Search correctness regresses at chunk boundaries

Mitigation:

- add chunk-boundary search tests early
- validate previews, ranges, and replace coordinates against the existing implementation

## Success Criteria

The migration should be considered successful only if it improves the measured capacity ceilings and not merely the internal architecture.

Success should mean:

- large-file responsiveness improves meaningfully beyond the current 32 MB to 128 MB ceiling range
- paste responsiveness improves meaningfully beyond the current 8 MB to 64 MB ceiling range
- whole-buffer string copies disappear from hot search and edit paths
- full-buffer undo snapshots are removed
- large-document rendering no longer depends on full `as_str()` flattening every frame

## Bottom Line

Choose a rope-backed text representation.

For Scratchpad, rope is the best practice because it fits the dominant editor workload better than a piece table, provides a stronger base for line-aware and chunk-aware operations, and gives the cleanest path to fixing the current large-file, large-paste, and search-snapshot bottlenecks.

The key implementation truth is this: adopting rope alone is not enough. The plan must also remove full-document assumptions from rendering, search snapshots, and undo history. If those phases are executed together, rope is the right long-term foundation for the editor.