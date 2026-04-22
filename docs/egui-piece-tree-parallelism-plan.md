# egui And PieceTree Integration Plan

Date: 2026-04-22

## Status Update

Core work in this plan is now complete.

Completed:

- Phase 1: local cursor and selection history replaced `egui` cursor ownership in the document layer
- Phase 2: immutable revisioned `DocumentSnapshot` exists
- Phase 3: search dispatch uses snapshots instead of owned whole-text payloads
- Phase 4: hot word-boundary and edit helpers were moved off whole-prefix and whole-suffix extraction
- Phase 5: large buffers have explicit viewport-window render and edit paths
- Phase 6: durable layout state no longer stores `egui::Galley`
- Phase 7: save, session persistence, startup restore, file open/decode, auto-reload, reopen-with-encoding, and restore-conflict compare now use background or snapshot-based boundaries where they sit on the live UI path

Remaining follow-ons are now optional optimization work rather than blockers for this plan:

- some focused large-buffer editing cases still fall back to the full-text path, especially where wrapped editing behavior still prefers the compatibility route
- the background I/O lane is currently bounded and centralized rather than a broader worker pool

The rest of this document is the implementation plan that drove the migration. Some "current friction" notes below describe the pre-migration state and are kept as historical context for why the changes were made.

## Purpose

This document translates the current parallelism and performance direction into a concrete implementation plan for the areas where `egui` still constrains deeper `PieceTree` adoption.

The goal is not to remove `egui` from Scratchpad.

The goal is to narrow `egui` to UI-facing responsibilities while moving editor semantics, document snapshots, and hot read paths further into project-owned code.

That is the path that makes parallelism materially useful instead of just moving cloned buffers onto worker threads.

## Decision

Scratchpad should keep `egui` for:

- windowing and frame orchestration
- painting and text shaping for the currently visible viewport
- clipboard and IME integration
- raw pointer and keyboard event collection

Scratchpad should not keep depending on `egui` for:

- canonical cursor and selection state in the document layer
- undo and redo selection records in document history
- full-document text ownership for search dispatch
- full-document layout as the default editor path
- viewport and preview logic that can be served directly from `PieceTree`

In short:

- `egui` remains the UI shell
- local editor and document code become the semantic source of truth
- `PieceTree` plus revisioned snapshots become the boundary for parallel work

## Why This Matters

Scratchpad already has meaningful local editor ownership:

- a native editor module
- local cursor and selection types
- direct `PieceTree` editing support
- local word-boundary and edit behavior

But the codebase still has several high-value coupling points where `egui` or whole-text assumptions blunt the benefit of that work.

The result today is:

- document state is not fully UI-framework-independent
- background search still starts from copied `String` payloads
- the active native editor path still flattens full document text before layout
- shared layout state still carries `egui::Galley`
- hot read helpers still allocate prefix and suffix strings on large documents

That means Scratchpad is already partly local, but not yet local enough for `PieceTree` to become the real center of the architecture.

## Current Friction Points

### 1. Document history still depends on `egui` cursor types

Relevant code:

- `src/app/domain/buffer/document.rs`

Current problem:

- `TextDocumentOperationRecord` stores `egui::text::CCursorRange`
- document-layer undo and redo APIs still expose `egui` types

Why it matters:

- the domain layer should not depend on UI-framework cursor models
- this keeps editor semantics from becoming fully local and reusable

### 2. Search still pays owned-text and cloned-tree dispatch costs

Relevant code:

- `src/app/domain/buffer/state.rs`
- `src/app/app_state/search_state/runtime.rs`
- `src/app/app_state/search_state/worker.rs`

Current problem:

- search requests still carry owned `String` text
- previews still carry a cloned `PieceTreeLite`
- full-text caching still ends in cloned `String` payloads

Why it matters:

- this is exactly the kind of pre-dispatch cost that weakens background execution
- it keeps search parallelism from scaling cleanly across buffers

### 3. The native editor still flattens full document text for active editing

Relevant code:

- `src/app/ui/editor_content/native_editor/mod.rs`
- `src/app/ui/editor_content/native_editor/highlighting.rs`

Current problem:

- active editor rendering still begins from `buffer.document().extract_text()`
- a full `egui::Galley` is still built from the whole text buffer

Why it matters:

- this keeps large-document editing tied to full-buffer layout cost
- it prevents the existing `PieceTree` windowing work from becoming the default path

### 4. Shared layout state still stores `egui` layout objects

Relevant code:

- `src/app/domain/buffer.rs`

Current problem:

- `RenderedLayout` stores `Arc<egui::Galley>`

Why it matters:

- layout state that survives beyond a frame is still too tied to `egui`
- the app keeps more UI-engine state in shared structures than it needs

### 5. Hot editor helpers still use whole-prefix and whole-suffix extraction

Relevant code:

- `src/app/ui/editor_content/native_editor/word_boundary.rs`
- `src/app/ui/editor_content/native_editor/editing.rs`

Current problem:

- word-boundary logic uses `extract_range(0..index)` and `extract_range(index..total)`
- outdent logic extracts large prefixes and suffixes
- cut and delete helpers still rely on owned extraction in hot paths

Why it matters:

- these paths are small individually but frequent
- they reduce the practical win from moving to `PieceTree`

## Architectural Direction

### 1. Make local editor types canonical

Scratchpad should standardize on local types such as:

- `CharCursor`
- `CursorRange`
- local edit-operation records

The conversion to and from `egui` types should happen only at the UI boundary.

### 2. Introduce revisioned immutable document snapshots

Scratchpad needs a `DocumentSnapshot` or equivalent immutable handle that:

- references a specific revision
- shares `PieceTree` backing storage cheaply
- is safe to move to workers
- exposes bounded extraction, spans, line lookup, and preview helpers
- carries revision identity for stale-result rejection

This is the most important bridge between deeper `PieceTree` adoption and parallelism.

### 3. Treat `egui` layout as viewport-local

`egui` should shape and paint only the current visible slice, not define the document model and not require full-buffer text for normal large-file interaction.

Small-document compatibility can stay simpler for a while.

Large-document behavior should use:

- piece-tree line windows
- overscanned visible spans
- local cursor and selection coordinates
- viewport-local layout only

### 4. Move read-heavy helpers onto bounded local scans

Hot helpers should prefer:

- piece iteration
- bounded line scans
- local neighborhood extraction
- span iteration

They should avoid:

- full-prefix extraction
- full-suffix extraction
- cloned whole-buffer previews

## Implementation Plan

## Phase 1. Remove `egui` cursor state from the document layer

Goal:

- make the document layer independent from `egui` selection and cursor types

Work:

- replace `egui::text::CCursorRange` in `TextDocumentOperationRecord` with local `CursorRange`
- update undo and redo APIs in `TextDocument` and `BufferState` to use local types
- keep any required `egui` conversion at UI-facing adapters only

Exit criteria:

- document and buffer undo history no longer depend on `egui`
- local editor semantics are canonical across editing flows

## Phase 2. Add immutable revisioned snapshots

Goal:

- create a cheap, shareable document boundary for worker-facing tasks

Work:

- introduce `DocumentSnapshot`
- move `PieceTree` backing storage behind shared ownership where needed
- expose snapshot APIs for:
  - bounded extraction
  - span iteration
  - line lookup
  - preview generation
  - revision identity

Exit criteria:

- search and save paths can hold snapshot handles instead of copied text
- snapshot creation is cheap enough to use per request

## Phase 3. Rebuild search request building on snapshots

Goal:

- remove whole-text and cloned-tree dispatch overhead from search

Work:

- replace `search_text: String` payloads with snapshot-backed search inputs
- replace `preview_tree: PieceTreeLite` cloning with snapshot preview access
- preserve generation-based cancellation and stale-result rejection
- keep the small-buffer fast path simple when truly cheaper

Exit criteria:

- search dispatch no longer requires full owned text for normal cases
- wide-scope search scales better across multiple buffers

## Phase 4. Convert hot editor helpers to piece-tree-native reads

Goal:

- stop spending the new storage model on avoidable temporary strings

Priority targets:

- word-boundary movement
- word deletion
- outdent and line-start inspection
- preview and selection reads where bounded scans are enough

Design rule:

- use local scans over nearby spans before extracting large temporary strings

Exit criteria:

- common cursor and word-edit paths no longer rely on whole-prefix or whole-suffix extraction

## Phase 5. Split editor rendering into explicit small-buffer and large-buffer paths

Goal:

- let the current native editor keep a practical compatibility mode while large documents move to viewport-local rendering

Work:

- keep the current simpler full-text path for smaller buffers
- promote visible-window rendering into the large-document default path
- shape only visible or overscanned text for large documents
- keep cursor and selection state in local coordinates, not `egui` state

Exit criteria:

- large-document editing no longer depends on flattening the whole document each frame

## Phase 6. Reduce shared layout dependence on `egui::Galley`

Goal:

- keep only the layout metadata the app really needs outside immediate paint

Work:

- audit `RenderedLayout`
- separate durable row and coordinate metadata from transient `egui` galley state
- avoid storing full `egui` layout objects when only row mappings or visible ranges are needed

Exit criteria:

- shared app/domain structures store local layout metadata by default
- `egui` galleys become frame-local or viewport-local implementation details where possible

## Phase 7. Expand snapshot-based worker pipelines

Goal:

- apply the same architecture to other expensive flows after search proves out the model

Targets:

- save
- session persistence
- restore
- file open and decode

Design rule:

- user intent and UI ordering remain on the UI thread
- snapshot consumption and blocking I/O move to bounded workers

Exit criteria:

- normal save, persist, restore, and open paths no longer depend on full live-buffer ownership on the UI thread

## What Should Stay In egui

These are not the problem and should stay there:

- pointer event collection
- keyboard event collection
- IME output plumbing
- clipboard integration
- final text shaping for the current viewport
- final painting and scroll interaction

Trying to replace these locally would add cost without solving the main performance issue.

## What Should Move Local

These should become fully project-owned:

- cursor and selection semantics
- undo and redo selection history
- word and line navigation rules
- visible-window extraction
- preview generation
- search snapshot inputs
- durable layout metadata needed by the app

## Priority Order

The best order of work is:

1. remove `egui` cursor types from document and buffer history
2. introduce revisioned immutable snapshots
3. convert search request building to snapshots
4. convert hot native-editor helper reads away from prefix and suffix extraction
5. make visible-window large-document rendering the default large-file path
6. reduce shared `egui::Galley` ownership outside immediate layout and paint
7. move save, restore, persistence, and open onto snapshot-based workers

This order matters because it removes architectural blockers before optimization layering.

## Success Criteria

This plan is successful when:

- the document layer no longer exposes `egui` cursor state
- search dispatch no longer depends on owned whole-text payloads
- hot editor helpers stop doing avoidable large temporary extractions
- large-document interaction uses viewport windows rather than full-document flattening
- `egui` remains a UI framework, not the owner of document semantics
- `PieceTree` and revisioned snapshots become the real basis for parallel work

## Recommendation

The next concrete implementation step should be:

1. decouple `TextDocument` and `BufferState` undo records from `egui`
2. introduce a revisioned `DocumentSnapshot`
3. convert search request construction and preview generation to snapshot-backed reads

That sequence gives the highest leverage and aligns directly with the existing parallelism and performance plan.
