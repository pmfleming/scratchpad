# Single-Path Text Pipeline Plan

## Goal

Integrate the current large-file path into the normal file path so the editor has one primary text pipeline instead of a separate special-case mode.

The priority is large-file performance and scalability, even if that raises baseline cost for small files.

That means:

- optimize for large-file open, scroll, edit, and search first
- accept some extra indirection, deferred work, and higher constant overhead on small files
- do not use file-size thresholds, file-size routing, or file-size-specific budgets anywhere in the editor path

## Current Problem

The current design still splits normal files and large files in ways that increase complexity and correctness risk.

### 1. The UI chooses different render paths for large files

`src/app/ui/editor_content/mod.rs` currently routes large files into visible-window rendering through `should_prefer_visible_window()` and `should_prefer_focused_window()`.

That creates multiple behavior surfaces for:

- scrolling
- focus transitions
- selection and cursor movement
- wrap behavior
- layout extent calculation

This is already visible in `docs/scroll-bottom-investigation.md`, which documents how large files can move between full render, read-only visible-window render, and focused visible-window render.

### 2. File open is still fundamentally whole-file

`src/app/services/file_service.rs` still decodes the full file into a `String` before building `FileContent`.

Large files only get staged metadata refresh, not a fundamentally different install path. That means the current large-file path reduces some follow-up work, but it does not remove the core eager decode and full-content construction cost.

### 3. The document core is ahead of the rest of the pipeline

`src/app/domain/buffer/document.rs` already uses `PieceTreeLite`, which is the right direction. But the surrounding pipeline still assumes or prefers contiguous full-text access too often.

So the project currently has:

- a scalable document core
- a partly staged metadata path
- a separate large-file viewport path in the UI
- too many whole-document assumptions around open, layout, and feature readiness

That is the main source of complication.

## Decision

Scratchpad should move to a single viewport-first text pipeline for all text files.

All files should use the same core model:

1. staged or incremental document installation
2. viewport-slice rendering
3. incremental metadata where possible
4. capability-gated features instead of mode-gated files

Under this plan, file size does not participate in routing, scheduling policy, cache policy, or feature policy. The path is identical for every text file.

## Target Architecture

### 1. One document path for all files

Every opened text file should become the same kind of document object, backed by the same storage abstraction and the same range/query APIs.

Required properties:

- chunk or slice access without flattening the full document
- line lookup without whole-document rescans
- snapshot support for background work without cloning full text
- edit operations that do not require rebuilding contiguous strings

The existing piece-tree-based document is the likely near-term anchor, but the important point is architectural unification, not preserving every current call shape.

### 2. One view pipeline for all files

Every editor view should render from viewport extraction, not from a special-case large-file window path.

That means:

- visible-range extraction becomes the default path
- focused and unfocused views share the same rendering architecture
- scroll extent is derived from view-local layout state
- cache and prefetch behavior are chosen without any file-size-based branching

The current large-file visible-window path should be treated as a prototype for the default rendering model, not as a permanent parallel mode.

### 3. Capability flags instead of special-case mode

The editor should distinguish feature readiness from document identity.

Examples:

- metadata complete vs metadata still refreshing
- wrap-ready vs wrap-degraded
- search index ready vs search running directly from snapshots
- artifact analysis complete vs sample-based

Those are per-buffer or per-view capabilities. They are easier to reason about than a separate "large file" architecture.

### 4. Open should become install-first, analyze-later

The open path should prioritize time to first paint and time to first interaction.

Required behavior:

- detect encoding and binary rejection from a prefix
- decode in chunks or staged passes
- install document state before all metadata work finishes
- push non-critical analysis behind first paint

Small files may pay a little more scheduling overhead under this model. That is acceptable because the same open path must apply to every text file.

### 5. Expensive features should become incremental or deferred

Anything that currently rescans or flattens the full document should move toward one of these forms:

- incremental update
- background refresh
- bounded viewport/local query
- explicit persistence-boundary flattening only

This especially applies to:

- line counting
- artifact inspection
- search snapshots and previews
- undo history storage
- layout and wrap measurement

## Phased Plan

### Phase 1: Define the unified path contract

Goal: establish the architecture boundary before changing implementation details.

Work:

- define the document/query operations the UI is allowed to depend on
- define view-local layout and scroll-extent state as first-class concepts
- define feature capability states that replace the current special-case mode notion
- define where full flattening is still allowed, if anywhere
- define an explicit rule that no file-size-based branch, threshold, or budget is allowed inside the interactive path

Exit criteria:

- the codebase has a single documented editor pipeline for all text files
- the documented pipeline contains no file-size thresholds or size-based exceptions

### Phase 2: Unify file open around staged installation

Goal: stop treating open as full decode plus selective follow-up behavior.

Work:

- replace eager full-string-first assumptions in file open with staged document installation
- keep early prefix inspection for encoding and binary checks
- move metadata completion and artifact analysis behind document installation
- ensure disk-backed reopen, reload, and encoding-reopen use the same install path

Expected result:

- first paint happens before full metadata is complete
- open no longer depends on one monolithic decoded `String` in the primary path

### Phase 3: Make viewport rendering the default editor path

Goal: remove the split between full render and visible-window render.

Work:

- migrate editor rendering to consume viewport slices for every file
- compute scroll extent from view-local layout state in all cases
- remove file-size-based routing in `src/app/ui/editor_content/mod.rs`
- make focus transitions stay within the same rendering architecture

Expected result:

- no separate visible-window vs full-document render decision at all
- fewer focus, scroll, and extent bugs caused by path switching

### Phase 4: Move metadata and search to incremental models

Goal: prevent interaction from falling back to full rescans.

The same incremental model should apply to every file, not only to files above a threshold.

Work:

- make line-count updates incremental
- make artifact summaries incremental or explicitly background-refreshed
- move search worker inputs to snapshots, spans, or chunk iterators instead of flattened text copies
- ensure previews and match navigation can operate from bounded extraction APIs

Expected result:

- paste and edit cost is driven more by local change size than total buffer size

### Phase 5: Unify editing and undo around the same storage model

Goal: avoid reintroducing a separate path through edit history or mutation helpers.

Work:

- keep all mutations on the same document abstraction used for open and render
- reduce whole-text snapshot dependence in undo/redo where practical
- ensure save and persistence are the main allowed flattening boundaries

Expected result:

- no separate "editable normal path" and special-case path
- edit behavior remains consistent across file sizes

### Phase 6: Remove legacy special-case concepts

Goal: finish the migration by deleting the architectural split.

Work:

- remove special-case routing helpers and duplicate render logic
- replace special-case UI wording with capability or readiness wording where needed
- remove any remaining file-size thresholds or size-based tuning from the editor path

Expected result:

- the codebase has one primary text pipeline
- no remaining file-size thresholds, routing hooks, or size-based policy checks exist in the text pipeline

## Explicit Tradeoffs

This plan intentionally accepts some regressions or cost shifts on small files.

Accepted tradeoffs:

- small files may pay more per-open overhead because they use the same staged pipeline
- simple views may carry more layout indirection because viewport rendering becomes universal
- some metadata may appear slightly later even for small files
- some features may become capability-driven and asynchronously completed instead of immediately complete

These tradeoffs are acceptable because the current architecture already favors small-file simplicity too heavily and pushes large-file cost into correctness and maintenance complexity.

## Guardrails

The unification should not collapse into another hidden dual-path design.

Rules:

- do not keep one code path for small files and another for large files
- do not let focus state choose a different editor architecture
- do not require whole-document flattening on interactive paths
- do not tie feature availability to file-size labels when a capability state can express the real condition
- do not tune caches, prefetch, worker priority, or metadata policy from file size

## Success Criteria

The migration is successful when all of the following are true:

- file size no longer affects routing, scheduling, feature readiness, or cache policy in the editor path
- open reaches first paint without full metadata completion
- scroll and cursor movement stay on one stable path regardless of focus
- paste and edit costs scale primarily with local change size
- search, previews, and metadata refresh avoid routine whole-document copies
- small files still behave correctly even if their baseline latency rises somewhat

## Recommended Order Of Execution

1. define the unified path contract
2. unify staged open and document installation
3. make viewport rendering universal
4. make metadata and search incremental
5. clean up undo/persistence boundaries
6. delete legacy special-case-path concepts

This order keeps the project focused on one unconditional text pipeline instead of extending or renaming the current special-case mode.
