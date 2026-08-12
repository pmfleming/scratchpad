# Editor Layer Restructuring Plan

Status: planning only  
Scope: internal architecture  
Implementation status: not started

## 1. Purpose

Restructure Scratchpad into four clear internal layers:

1. **Editor core** — document storage, edits, history, anchors, and logical view state.
2. **egui editor** — rendering, input, gutter, scrolling, and editor presentation state.
3. **egui workspace** — split panes, tile layout, tile management, and workspace interaction.
4. **Scratchpad app** — files, tabs, search UI, settings, sessions, commands, and desktop integration.

The immediate goal is better ownership and dependency direction inside Scratchpad. This plan does not commit the project to publishing a component, creating multiple Cargo packages, supporting other UI toolkits, or stabilizing an external API.

## 2. Desired Dependency Direction

The intended dependency flow is:

**Scratchpad app → egui workspace → egui editor → editor core**

The app may also use lower layers directly where appropriate. Dependencies must not point upward:

- Editor core must not depend on egui, eframe, workspace code, or Scratchpad application code.
- egui editor must not depend on `ScratchpadApp`, tabs, file services, session state, or application command dispatch.
- egui workspace must not depend on search runtime, file services, session persistence, or settings controllers.
- Scratchpad app remains the integration point and may depend on all three lower layers.

During migration, temporary re-exports may preserve existing paths, but they must not become permanent reverse dependencies.

## 3. Scope

### Included

- Establishing internal module boundaries.
- Moving editor-neutral types out of UI modules.
- Separating editable document state from file and disk state.
- Separating logical view state from egui rendering state.
- Moving complete viewport scrolling behavior into the egui editor layer.
- Separating pane and tile behavior from Scratchpad tab behavior.
- Replacing direct lower-layer access to `ScratchpadApp` with focused inputs and returned outcomes.
- Preserving existing tests, probes, benchmarks, and performance reporting.
- Updating internal documentation and repository maps after the structure is stable.

### Not included

- Publishing editor crates.
- Promising a stable public component API.
- Supporting non-egui UI frameworks.
- Designing a plugin system.
- Introducing a generic text-model trait without a demonstrated requirement.
- Redesigning Scratchpad’s editor or workspace UI.
- Changing editor behavior, shortcuts, or Windows compatibility intentionally.
- Changing session file formats unless an unavoidable migration is separately approved.
- Moving the Performance Lens or dashboard into this repository.
- Adding features merely because the code has been separated.

## 4. Architectural Principles

### 4.1 Preserve one editor behavior path

Small files, large files, split views, search-decorated views, and restored views should continue through the same editor behavior path. Separation must not create a second simplified component path.

### 4.2 Move ownership before generalizing

The first objective is correct dependency ownership. Keep concrete Scratchpad-proven document and view types initially. Traits, extension APIs, and public abstractions can be considered later if real consumers require them.

### 4.3 Separate semantic state from presentation state

Core state describes text, positions, ranges, edits, and stable anchors. egui state describes pixels, rectangles, layout caches, galleys, focus responses, and repaint behavior.

### 4.4 Return outcomes instead of reaching upward

Lower layers should report what occurred or what was requested. The Scratchpad app decides how to translate those outcomes into application commands, dirty-state updates, search refreshes, or session changes.

### 4.5 Keep persistence owned by the app

Core runtime types do not automatically become persistence formats. Scratchpad-owned session data-transfer types should remain the compatibility boundary and translate to and from runtime state.

### 4.6 Measure throughout the restructure

A move is not complete merely because it compiles. Existing behavior, frame cost, memory use, scrolling performance, and large-file capacity must remain measured.

## 5. Target Layer Responsibilities

## 5.1 Editor Core

### Owns

- Piece-tree storage.
- Text document state.
- Document snapshots and text slices.
- Character, line, and document-offset queries.
- Edit transactions and replacement operations.
- Undo and redo history.
- History coalescing and history budgets that are intrinsic to editing.
- Stable document anchors and anchor ownership.
- Cursor and selection value types.
- Logical cursor navigation and word-boundary rules where they do not require UI layout.
- Logical view state, including the document association and anchored cursor state.
- Mutation summaries needed by higher layers.

### Does not own

- `egui` or `eframe` types.
- Pixel positions, rectangles, viewport dimensions, or galleys.
- File paths, canonical paths, save operations, or file watchers.
- Encoding detection and disk freshness policy.
- Tabs, pane headers, or tile chrome.
- Search controls or search-worker coordination.
- Scratchpad settings controllers.
- Application command dispatch.

### Boundary rule

Editor-core tests must be able to construct and edit a document without constructing an egui context or `ScratchpadApp`.

## 5.2 egui Editor

### Owns

- The complete editable viewport.
- Text layout and visible-slice extraction for rendering.
- Keyboard and mouse input translation.
- IME preedit, commit, and candidate geometry integration.
- Cursor, selection, and text painting.
- Gutter and line-number rendering.
- Vertical and horizontal scrolling.
- Scrollbars, reveal behavior, and scroll intents.
- Drag-selection edge autoscroll.
- Layout caches and display snapshots tied to egui layout.
- Pixel viewport metrics.
- egui focus and interaction state.
- Generic visual decorations such as marked ranges and replacement previews.
- Editor visual options supplied by the app.
- A focused editor outcome describing changes and requests.

### Does not own

- `ScratchpadApp`.
- Workspace tabs or the tab strip.
- File opening, saving, renaming, or reloading.
- Search query execution or result orchestration.
- Session capture and restore formats.
- Application settings mutation.
- Split-pane structure or tile headers.

### Boundary rule

The egui editor must be renderable when given editor-core document/view state and editor presentation inputs, without receiving a `WorkspaceTab` or `ScratchpadApp`.

## 5.3 egui Workspace

### Owns

- Pane-tree structure used to place editor views.
- Split axes, paths, ratios, and directional relationships.
- Pane insertion, removal, movement, and repair rules.
- Workspace rectangle traversal.
- Split-divider rendering and resizing.
- Tile activation and workspace focus movement.
- Tile frames, borders, and split previews.
- Tile headers insofar as they represent workspace views.
- Tile-level drag and drop behavior.
- Workspace actions such as activate, close, split, move, and resize.

### Does not own

- Scratchpad’s top-level tab strip.
- File buffers or disk metadata.
- File and session services.
- Search state refresh.
- Settings persistence or settings controllers.
- Direct `AppCommand` dispatch.

### Boundary rule

The workspace receives descriptions and state from the host, arranges editor views, and returns workspace actions. It does not mutate the full application directly.

## 5.4 Scratchpad App

### Owns

- `ScratchpadApp` and frame orchestration.
- Open-file records and buffer-to-file association.
- Paths and canonical path keys.
- Dirty state and disk freshness.
- Encoding, BOM, newline, and save-risk metadata.
- Artifact analysis and application-specific display decisions.
- Top-level tabs, tab selection, tab combining, and tab overflow.
- Search and replace query state, workers, progress, and controls.
- Conversion of search results into editor decorations.
- Settings storage and mutation.
- Session capture, restore, and compatibility.
- File services, file watchers, and background I/O.
- Application commands, shortcut routing, dialogs, status UI, and window chrome.
- Translation of editor and workspace outcomes into application behavior.

### Boundary rule

Application behavior remains here rather than being pulled into lower layers merely to make rendering convenient.

## 6. Current Architectural Seams

## 6.1 `BufferState` combines core and application concerns

`src/app/domain/buffer/state.rs` currently combines:

- `TextDocument` and editing history.
- File name and path.
- Canonical path identity.
- Dirty state.
- Encoding and line-ending metadata.
- Disk state and freshness.
- Artifact summaries.
- Display flags.
- Active selection.

The target separation is:

- A core editable document containing text, history, revision, and anchors.
- A Scratchpad-owned open-document record containing identity, file metadata, disk state, and application analysis.
- Per-view selection in logical view state rather than a document-global UI selection, except where a derived active selection is temporarily required for compatibility.

This boundary should be introduced incrementally because editing currently feeds metadata refresh, history events, dirty state, and save behavior.

## 6.2 `EditorViewState` combines logical and egui state

`src/app/domain/view.rs` currently includes:

- Buffer association.
- Cursor ranges and anchored ranges.
- Search ranges.
- Scroll anchors and intents.
- Display snapshots.
- Layout caches.
- egui rectangles for IME output.
- Pixel-space metrics.
- Focus state.

The target separation is:

- **Core view state:** identity, document association, cursor, selection, stable anchors, and logical navigation state.
- **egui editor state:** layout cache, display snapshot, pixel metrics, focus, IME geometry, and render-lifetime state.
- **App-supplied decorations:** search matches, active result, and replacement preview semantics.

Logical document anchors may live in editor core. Pixel scroll resolution and viewport metrics belong in egui editor.

## 6.3 Complete scrolling is currently partly tile-owned

Important viewport behavior currently lives in:

- `src/app/ui/editor_area/tile.rs`
- `src/app/ui/editor_area/tile/scroll_frame.rs`
- `src/app/ui/editor_area/tile/scroll_input.rs`
- `src/app/ui/editor_area/tile/autoscroll.rs`
- `src/app/ui/scrolling/`

The egui editor boundary should include this complete scrolling path. The workspace should allocate a tile body rectangle and invoke the editor viewport; it should not own editor scroll truth.

## 6.4 Editor-area rendering reaches into the whole app

`src/app/ui/editor_area/mod.rs` currently:

- Receives `ScratchpadApp`.
- Refreshes search state.
- Reads and mutates settings.
- Traverses the active tab’s pane tree.
- Dispatches workspace commands.
- Finalizes buffer mutations.

These responsibilities must be separated rather than moved together. Workspace traversal belongs in egui workspace; editor rendering belongs in egui editor; search refresh, settings mutation, command dispatch, and mutation finalization remain in Scratchpad app.

## 6.5 `WorkspaceTab` combines documents and pane layout

`WorkspaceTab` currently contains both a collection of buffers and a workspace layout. The target model keeps Scratchpad’s tab as the owner of application document associations while the workspace layer owns view placement and pane behavior.

Combining tabs and pruning application buffers remain Scratchpad behaviors. Pane insertion, removal, resizing, and directional movement belong to the workspace layer.

## 6.6 Search semantics are embedded in view/render state

Current view state names search highlights and replacement previews directly. The target split is:

- Scratchpad app owns search semantics and result lifecycle.
- Editor core may provide generic anchored-range facilities when ranges need edit stability.
- egui editor renders neutral decorations supplied for the current view.

This prevents the editor layer from depending on the search subsystem while preserving efficient, anchor-aware rendering.

## 6.7 Core editing types currently live under native editor UI

Document code currently imports cursor and operation types from native editor modules. Cursor ranges, edit records, and other neutral editing values should move downward first, reversing this dependency before larger modules move.

## 7. Proposed Internal Module Shape

The initial restructure should remain within the existing Cargo package. A possible destination is:

- `src/editor_core/`
- `src/egui_editor/`
- `src/egui_workspace/`
- `src/app/`

This is an internal organization, not a commitment to separate packages or public exports.

Temporary compatibility modules or re-exports may be used during migration. The final layout should make the dependency direction visible from paths and module visibility.

## 8. Indicative Current-to-Target Map

| Current area | Intended owner | Notes |
| --- | --- | --- |
| `domain/buffer/piece_tree` | Editor core | Storage, queries, anchors, and rebalancing |
| `domain/buffer/document` | Editor core | Text and history mutation boundary |
| `domain/buffer/history` | Editor core | Keep persistence DTO decisions separate |
| `domain/buffer/snapshot` | Editor core | Neutral document snapshots |
| Neutral cursor/edit types in `native_editor/types` | Editor core | Move before document modules depend on them |
| Neutral parts of `native_editor/editing` | Editor core | UI event interpretation remains in egui editor |
| Neutral word-boundary logic | Editor core | Only if independent of galley/display rows |
| `domain/view/anchors` | Editor core | Logical cursor/range anchoring |
| `domain/view/layout_cache` | egui editor | Rendering cache |
| Display snapshots used for painted rows | egui editor | Keep core snapshots distinct from display snapshots |
| `ui/editor_content/native_editor` | egui editor | Layout, input translation, painting, IME |
| `ui/editor_content/gutter` | egui editor | Part of complete viewport |
| `ui/scrolling` | egui editor | Pixel scroll behavior and viewport bridge |
| Tile scroll frame/input/autoscroll | egui editor | Move out of tile ownership |
| `domain/panes` | egui workspace | Pane model and mutation |
| Workspace layout portions of `domain/tab/layout_state` | egui workspace | Separate from Scratchpad tab ownership |
| `ui/editor_area/divider` | egui workspace | Split geometry and resizing |
| Editor-area pane traversal | egui workspace | Exclude search strip and app orchestration |
| Tile frame, border, focus, and split preview | egui workspace | Use host-supplied descriptions/options |
| Standard text-edit context actions | egui editor | Undo, cut, copy, paste, delete, select all |
| Tile and file context actions | Workspace or app | Classify by semantic owner |
| File metadata portions of `BufferState` | Scratchpad app | Path, format, disk, dirty and freshness state |
| `WorkspaceTabBuffers` | Scratchpad app | Application document association |
| `TabManager` and tab UI | Scratchpad app | Top-level tabs remain app-specific |
| Search runtime and search/replace UI | Scratchpad app | Supply neutral decorations to editor |
| Services, sessions, settings, dialogs | Scratchpad app | No lower-layer dependency |

The map is indicative. Each move should be validated by responsibility and dependency rather than performed solely by filename.

## 9. Migration Strategy

## Phase 0: Baseline and Inventory

### Work

- Record the current module dependency map around buffers, views, editor rendering, scrolling, panes, tabs, and commands.
- Identify reverse dependencies from domain code into UI code.
- Inventory persistent session types that refer to buffer, view, and pane state.
- Inventory every editor and workspace call site that receives `ScratchpadApp` or `WorkspaceTab`.
- Record the current test suite status.
- Capture a performance baseline using Scratchpad’s existing probes, benches, and Scratchpad Performance Lens.
- Record the Scratchpad commit, Performance Lens commit, configuration, platform, build profile, and measurement artifact location.

### Deliverables

- Dependency inventory.
- Persistence-impact inventory.
- Test baseline.
- Performance baseline.
- Explicit list of behavior that must remain unchanged.

### Exit criteria

- Baseline results are reproducible.
- High-risk dependency cycles are identified.
- No source movement has started without a known destination owner.

## Phase 1: Establish Internal Boundaries

### Work

- Introduce the conceptual top-level module boundaries inside the current package.
- Write short ownership documentation for each boundary.
- Define and enforce the intended dependency direction during review.
- Use transitional re-exports so moves can remain small.
- Avoid changing runtime behavior or APIs and file structure in the same step where possible.

### Deliverables

- Visible internal layer structure.
- Temporary migration map for old and new module paths.
- Dependency-rule checklist for reviews.

### Exit criteria

- New code has an unambiguous destination layer.
- Temporary paths are identified as temporary.
- Tests and baseline measurements still run.

## Phase 2: Move Neutral Editing Types into Editor Core

### Work

- Move character cursor and cursor-range types downward.
- Move selection range helpers downward.
- Move edit-operation and operation-record value types downward.
- Move neutral document position and length types downward.
- Remove document imports from native editor UI modules.
- Keep egui-specific cursor geometry and galley cursor conversion in egui editor.

### Deliverables

- A UI-independent vocabulary for edits, ranges, and positions.
- Correct dependency direction between document and renderer.

### Exit criteria

- Editor core does not import cursor or edit types from UI modules.
- Core type tests run without egui setup.
- History serialization behavior remains unchanged.

## Phase 3: Consolidate the Core Document

### Work

- Move piece-tree storage, text document, core snapshots, and edit history under editor core.
- Establish one conceptual mutation path for typing, paste, deletion, cut, replacement, undo, and redo.
- Keep revision tracking and anchor updates within the core mutation boundary.
- Separate intrinsic text behavior from Scratchpad file-format policy.
- Decide explicitly where preferred newline insertion policy is supplied, without moving encoding and save-risk policy into core.
- Provide mutation facts needed by Scratchpad metadata refresh without exposing application state to core.

### Deliverables

- Standalone core document model.
- Clear mutation and history ownership.
- Defined mutation result consumed by higher layers.

### Exit criteria

- A document can be edited, queried, undone, and redone without egui or app construction.
- Existing piece-tree and history tests pass.
- No unexplained change appears in edit throughput or memory measurements.

## Phase 4: Separate Open-File State from the Core Document

### Work

- Separate file identity and disk metadata from the editable document.
- Keep paths, canonical paths, dirty state, format metadata, disk state, freshness, and save-risk analysis in Scratchpad app.
- Preserve stable buffer/document IDs required by tabs, views, sessions, and background work.
- Keep session persistence DTOs in the app and translate to runtime core documents.
- Preserve dirty-state and metadata-refresh timing after editor mutations.
- Avoid changing file open, save, reload, conflict, or restore behavior.

### Deliverables

- Core document with no file-system responsibility.
- Scratchpad-owned open-document record.
- Explicit translation between persisted and runtime document state.

### Exit criteria

- Editor core has no file-service or canonical-path dependency.
- File and session tests pass unchanged in behavior.
- Unsaved and restored document behavior is preserved.

## Phase 5: Split Logical View State from egui View State

### Work

- Place cursor, selection, stable range anchors, and document association in core view state.
- Place layout cache, display snapshot, pixel metrics, focus state, and IME geometry in egui editor state.
- Separate logical scroll anchoring from pixel viewport resolution where practical.
- Keep anchor release and lifecycle ownership explicit when views close or documents are replaced.
- Replace search-specific state in the generic view boundary with app-supplied decoration state.
- Preserve active-selection behavior while moving toward selection being fundamentally per-view.

### Deliverables

- Core logical view state without egui types.
- egui presentation state with clear invalidation rules.
- Explicit lifecycle for document anchors owned by views and decorations.

### Exit criteria

- Core view state contains no egui rectangles, vectors, galleys, or focus responses.
- Closing, splitting, restoring, and replacing views do not leak anchors.
- Cursor, navigation, selection, and IME tests pass.
- View-navigation and allocation measurements remain within the accepted baseline range.

## Phase 6: Form the Complete egui Editor

### Work

- Bring native editor rendering, gutter, scrolling, scrollbars, reveal behavior, and selection autoscroll into one layer.
- Move tile-owned editor scroll preparation and finalization into the editor viewport boundary.
- Define focused editor inputs: document, core view, egui view state, visual options, input options, viewport, and decorations.
- Define an editor outcome that reports document changes, focus changes, selection changes, and host-level requests.
- Remove direct access to `ScratchpadApp`, `WorkspaceTab`, settings controllers, and app command dispatch.
- Keep standard text editing shortcuts and IME behavior inside the editor interaction boundary.
- Keep app shortcuts scoped and interpreted above the editor.
- Preserve visible-slice layout, anchor-aware scrolling, layout caching, and capacity behavior.

### Deliverables

- Complete egui editor viewport boundary.
- Editor outcome consumed by the app/workspace integration.
- Consolidated ownership of scrolling and rendering state.

### Exit criteria

- The editor can render without a `ScratchpadApp` or `WorkspaceTab` argument.
- Gutter, editor body, scrolling, selection autoscroll, and IME remain one behavior path.
- Editor interaction and scrolling tests pass.
- Frame, typing, paste, scrolling, viewport-extraction, memory, and capacity measurements show no unexplained regression.

## Phase 7: Form the egui Workspace

### Work

- Move pane-tree ownership and pane mutation into the workspace layer.
- Move rectangle traversal, divider rendering, tile geometry, borders, and split previews into the workspace renderer.
- Separate workspace view descriptions from Scratchpad buffer records.
- Replace direct command dispatch with returned workspace actions.
- Separate standard editor context actions, workspace tile actions, and Scratchpad file actions by owner.
- Keep the search strip, top-level tab strip, tab combining, and application menus outside the workspace.
- Preserve divider ratios, directional movement, split placement, focus, and restored pane repair.

### Deliverables

- Pane/workspace model.
- egui workspace renderer.
- Workspace action model interpreted by Scratchpad.

### Exit criteria

- Workspace rendering does not require search runtime, file services, session services, or settings controllers.
- Split, close, activate, resize, move, and restore behavior remains unchanged.
- Split stress, tile layout, multi-view frame cost, and workspace navigation measurements show no unexplained regression.

## Phase 8: Make Scratchpad the Explicit Integration Layer

### Work

- Translate editor outcomes into dirty-state updates, metadata refresh, history notifications, and repaint requests.
- Translate workspace actions into existing Scratchpad commands or direct app orchestration.
- Supply editor and workspace visual options from Scratchpad settings.
- Convert search results and replacement previews into neutral editor decorations.
- Keep search refresh scheduling in the app.
- Keep focus requests coordinated with tabs, dialogs, search controls, and window regions.
- Keep file/session services operating on Scratchpad-owned records and core document snapshots.
- Remove temporary lower-layer access to the whole app.

### Deliverables

- Thin, explicit adapters between Scratchpad and lower layers.
- Clear ownership of mutation finalization and command routing.
- No hidden upward dependencies.

### Exit criteria

- Lower layers never dispatch `AppCommand` directly.
- Lower layers do not mutate settings or search runtime.
- Full application tests, startup tests, file tests, session tests, settings tests, and tab tests pass.
- Full Performance Lens reports show no unexplained regression.

## Phase 9: Cleanup and Boundary Enforcement

### Work

- Remove transitional re-exports and migration shims.
- Reduce visibility of internal implementation details.
- Update repository maps and architecture documentation.
- Audit names so `buffer`, `document`, `view`, `workspace`, and `tab` have distinct meanings.
- Audit for upward imports and whole-app parameter leakage.
- Review clone and allocation patterns introduced during separation.
- Confirm probes and benches still compile against intentional measurement surfaces.

### Deliverables

- Final internal module layout.
- Updated documentation.
- Dependency audit report.
- Final behavior and performance comparison.

### Exit criteria

- The dependency direction is clean and documented.
- Temporary shims are removed or have a separately documented reason to remain.
- Measurement coverage is not reduced.
- Session compatibility and editor behavior remain intact.

## Phase 10: Optional Post-Restructure Evaluation

After the internal architecture has remained stable, the project may separately evaluate:

- Whether any layer should become a Cargo workspace crate.
- Whether the egui-facing layer should depend directly on `egui` rather than using eframe’s re-export.
- Whether any API should become public.
- Whether examples or external component packaging are worthwhile.

None of these is an outcome required by this plan.

## 10. Performance Measurement Plan

Scratchpad already has a dedicated performance-measurement repository:

- [Scratchpad Performance Lens](https://github.com/pmfleming/scratchpad-performance-lens)

The existing repository boundary in `docs/measurement-tools.md` remains authoritative.

## 10.1 Ownership

Scratchpad continues to own measurement targets that must compile against application internals:

- `src/bin/capacity_probe.rs`
- `src/bin/frame_metrics.rs`
- `src/bin/resource_probe.rs`
- `src/bin/profile_*.rs`
- `benches/search_speed.rs`
- `benches/frame_budget.rs`
- Related benchmark target data

Scratchpad Performance Lens continues to own Scratchpad-specific measurement production and synthesis, including:

- Search speed reports.
- Slowspot and speed-efficiency reports.
- Capacity reports.
- Resource profiles.
- Frame metrics.
- Performance review synthesis.
- Flamegraph indexing.

The dashboard and run orchestration remain in their existing repository. Performance tooling is not moved into editor core, egui editor, or egui workspace.

## 10.2 Baseline capture

Before implementation begins:

- Run the existing Scratchpad probes and benches in the intended build profile.
- Run the Performance Lens `measure all` workflow.
- Retain generated artifacts under the established analysis location.
- Record the Scratchpad and Performance Lens revisions.
- Record configuration, platform, renderer, file scenarios, and relevant environment settings.
- Note normal measurement variance before setting regression thresholds.

The baseline should include, where available:

- Editor frame time.
- Scroll stress.
- Typing and paste stress.
- View navigation.
- Viewport extraction.
- UI render frames.
- Split stress and tab/tile layout.
- Resource and memory use.
- Large-file and many-view capacity.
- Search behavior affected by document or decoration changes.

## 10.3 Per-phase gates

| Restructuring phase | Primary measurement focus |
| --- | --- |
| Neutral types and core document | Edit throughput, history, snapshots, allocation, memory |
| Open-file/document separation | Snapshot, save-source extraction, background-work inputs |
| View-state separation | Navigation, anchor behavior, allocations, view lifecycle |
| egui editor formation | Frame budget, typing, paste, scrolling, viewport extraction, IME repaint behavior |
| egui workspace formation | Split stress, tile layout, multi-view frame cost, directional navigation |
| Scratchpad integration | Full capacity, resources, frame metrics, search, session-heavy scenarios |
| Cleanup | Full Performance Lens run and baseline comparison |

For every gate:

1. Run focused probes during local iteration.
2. Run the matching Performance Lens category before completing the phase.
3. Compare against the Phase 0 baseline and the immediately preceding phase.
4. Investigate material regressions before proceeding.
5. Distinguish repeatable regression from measurement noise.
6. Document intentional trade-offs rather than silently accepting them.

## 10.4 Measurement-surface compatibility

Module movement may break probe imports even when runtime behavior is unchanged. The plan must therefore:

- Treat probes and benches as first-class internal consumers.
- Use short-lived re-exports when necessary to keep measurements available during moves.
- Deliberately update measurement targets when ownership changes.
- Coordinate report-input changes with Scratchpad Performance Lens rather than dropping fields or scenarios.
- Keep generated measurement artifacts outside product source modules.

## 10.5 Performance acceptance

The restructure is not complete until:

- Existing measurement scenarios still run.
- Scratchpad Performance Lens can consume the resulting artifacts.
- No unexplained editor, scrolling, memory, large-file, or split-workspace regression remains.
- Any intentional trade-off is recorded with the measured cost and architectural benefit.

## 11. Test Strategy

## 11.1 Editor-core tests

Cover without egui:

- Insert, delete, replace, cut, undo, and redo.
- Edit coalescing.
- Multi-range replacement rules used by replace-all.
- Piece-tree anchors across edits.
- Cursor and selection anchor recovery.
- Line and offset queries.
- Word boundaries and Unicode behavior where core-owned.
- Snapshots and revisions.
- History budgets and persistence translation.

## 11.2 egui-editor tests

Cover:

- Keyboard and mouse input translation.
- Cursor movement and selection extension.
- Visible-slice boundaries.
- Page and vertical navigation.
- Gutter alignment.
- Scrollbar, wheel, reveal, and drag-autoscroll behavior.
- Wrapped and unwrapped layout.
- Selection and decoration painting inputs.
- Focus locking and shortcut scoping.
- IME preedit, commit, focus loss, and geometry publication.
- Cache invalidation after text, selection, decoration, font, and viewport changes.

## 11.3 Workspace tests

Cover:

- Split insertion and placement.
- Split removal and tree collapse.
- Divider resizing and ratio clamping.
- Directional movement and focus.
- View ordering.
- Restored-layout repair.
- Tile activation and close actions.
- Split preview geometry.
- Multiple views of the same document.

## 11.4 Scratchpad integration tests

Cover:

- Editor mutation to dirty-state and metadata refresh.
- Open, save, reload, and external-change behavior.
- Session capture and restore.
- Search result decoration and navigation.
- Replace operations and history.
- Tab combining and buffer pruning.
- Settings application.
- Shortcut routing between editor and app.
- Dialog and search focus transitions.

## 12. Persistence and Compatibility Strategy

- Keep Scratchpad session DTOs in the app layer.
- Do not serialize egui layout caches, galleys, focus responses, or temporary IME geometry.
- Continue to restore stable buffer IDs, view IDs, cursor state, pane layout, and necessary logical scroll state.
- Translate old persisted structures to the new runtime separation inside Scratchpad restore code.
- Avoid changing serialized names or shapes merely to match runtime module names.
- Add migration only if the old shape cannot be reconstructed without loss.
- Verify unsaved buffers and multiple views of one document particularly carefully.

## 13. Commands, Shortcuts, and Context Menus

### Editor-owned

- Text movement and selection commands.
- Insert, delete, cut, copy, paste, undo, redo, and select all.
- IME text handling.
- Standard editor context actions.

### Workspace-owned

- Activate view.
- Split view.
- Close view request.
- Move or resize tile.
- Workspace focus traversal.

### Scratchpad-owned

- Open, save, save as, reload, and rename.
- Top-level tab commands.
- Search and replace commands.
- Settings and dialogs.
- File-location and encoding actions.
- Translation from configurable app shortcuts to editor/workspace actions.

Shortcut handling must preserve the current rule that app bindings do not steal expected text-editing behavior while the editor has focus.

## 14. Decorations and Search Boundary

Search remains an app service and UI feature. The renderer should receive a neutral decoration description containing only what is necessary to paint ranges and identify an active decoration.

If decorations must remain attached to text across edits:

- Editor core supplies generic anchored-range capability.
- Scratchpad app owns what each range means.
- egui editor owns how a decoration style is painted.

Replacement preview follows the same split: Scratchpad computes replacement semantics, while egui editor paints a neutral preview representation.

## 15. Focus and IME Boundary

- Core view state records logical cursor and selection, not OS or egui focus.
- egui editor owns widget focus, focus locking, IME preedit, and candidate geometry publication.
- Workspace coordinates active tile and traversal between views.
- Scratchpad coordinates editor focus with tabs, search controls, dialogs, settings, and window chrome.
- Focus requests should be explicit outcomes or inputs, not hidden whole-app mutations.

## 16. Risks and Mitigations

### Risk: behavior changes hidden inside module moves

Mitigation:

- Separate moves from behavior changes.
- Keep commits and review units narrow.
- Use baseline interaction tests and performance measurements at each phase.

### Risk: excessive abstraction

Mitigation:

- Keep concrete document and view models.
- Do not introduce a generic document trait as part of the restructure.
- Add abstractions only where an existing dependency must be inverted.

### Risk: temporary state duplication becomes permanent

Mitigation:

- Mark compatibility fields and re-exports with an intended removal phase.
- Track one source of truth for document, cursor, anchors, and scrolling.

### Risk: anchor leaks during view separation

Mitigation:

- Define ownership and release behavior before moving view lifecycle code.
- Test close, split, document replacement, restore, and search-decoration cleanup.

### Risk: persistence regression

Mitigation:

- Keep persistence DTOs app-owned.
- Test old session fixtures and unsaved content.
- Avoid coupling serialized shape to new runtime module paths.

### Risk: scrolling regression

Mitigation:

- Move the complete scrolling path as a unit after its inputs are separated.
- Do not split scroll truth between workspace and editor.
- Run scroll, viewport, frame, and large-file probes immediately after the move.

### Risk: extra cloning or allocation at layer boundaries

Mitigation:

- Prefer borrowed access and focused mutable ownership internally.
- Measure before changing data-transfer shape.
- Use Performance Lens and allocation/resource probes to validate the result.

### Risk: probes stop compiling and coverage is lost

Mitigation:

- Treat probes as supported internal callers during migration.
- Update or temporarily re-export their required surfaces.
- Do not mark a phase complete with missing measurement scenarios.

### Risk: workspace and tab concepts remain confused

Mitigation:

- Define workspace as pane/view placement inside one Scratchpad tab.
- Keep top-level tab selection, combining, ordering, and overflow in the app.

## 17. Review Checklist for Each Migration Step

- Does the moved item have one clear owner?
- Does the dependency point downward?
- Did any lower layer gain access to `ScratchpadApp` for convenience?
- Did egui geometry enter editor core?
- Did file or session policy enter the editor layers?
- Is there still one source of truth for the state being moved?
- Are anchor lifecycle and cache invalidation explicit?
- Did serialization behavior change?
- Do relevant tests still pass?
- Do relevant probes and Performance Lens reports still run?
- Is any performance difference understood?
- Is the transitional path scheduled for removal?

## 18. Completion Criteria

The restructuring is complete when:

1. Editor core can be tested without egui, eframe, workspace, or app construction.
2. Core document and view state contain no egui geometry or rendering caches.
3. egui editor owns the complete viewport, including gutter and scrolling.
4. egui editor does not receive `ScratchpadApp` or `WorkspaceTab`.
5. egui workspace owns pane and tile behavior but not tabs, files, or search runtime.
6. Lower layers return outcomes rather than dispatching application commands.
7. Scratchpad app owns files, tabs, search UI, settings, sessions, and integration policy.
8. Existing session and unsaved-work behavior remains compatible.
9. Existing editor, workspace, file, session, settings, and startup tests pass.
10. Existing probes and benches continue to run.
11. Scratchpad Performance Lens continues to consume the measurement artifacts.
12. No unexplained performance or capacity regression remains.
13. Temporary migration shims have been removed or separately justified.
14. No decision about public packaging or external component support is required.

## 19. Recommended First Implementation Slice

When implementation is approved, the safest first slice is:

1. Capture behavior and Performance Lens baselines.
2. Introduce internal layer boundaries without changing behavior.
3. Move cursor, selection, and edit-operation value types into editor core.
4. Remove the first document-to-UI reverse dependencies.
5. Stop and validate tests, probes, and measurement reports before moving document ownership.

This creates useful dependency improvement while avoiding an early high-risk move of scrolling, workspace rendering, or persistence state.
