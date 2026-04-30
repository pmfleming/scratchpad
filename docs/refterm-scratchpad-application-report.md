# Applying Refterm Lessons to Scratchpad

This report translates the refterm review into concrete Scratchpad recommendations. It focuses on the existing Rust/egui editor, not on turning Scratchpad into a terminal renderer.

Scratchpad already has several good foundations:

- a piece-tree document model in `src/app/domain/buffer/piece_tree.rs`
- `DocumentSnapshot` and search-dispatch profiling work described in `docs/parallelism-performance-plan.md`
- `DisplaySnapshot` scaffolding in `src/app/ui/scrolling/display.rs`
- ASCII fast paths in both the piece tree and search/highlighting helpers
- benchmark and profiling scripts under `benches/` and `scripts/`

The refterm lesson is not "rewrite everything in C and D3D." The better reading is: move less data, shape less text, cache the expensive parts, and make the visible hot path boring.

## Current Scratchpad Friction

The native editor currently builds a full egui galley from the document text in `render_editor_text_edit`:

- `build_editor_galley` calls `buffer.document().text_cow()`.
- `text_cow` borrows the whole text only when the piece tree is contiguous; otherwise it falls back to `extract_text`.
- `highlighting::build_galley` builds a `LayoutJob` for the whole text.
- `DisplaySnapshot::from_galley` then derives row metadata from the full galley.

That means the app has a viewport-oriented scrolling API, but the primary layout artifact is still full-document. For small and medium files this is fine. For large files, it means scrolling, edits, selections, and search highlights can force too much text/layout work onto the UI frame.

The important code paths are:

- `src/app/ui/editor_content/native_editor/mod.rs`
- `src/app/ui/editor_content/native_editor/highlighting.rs`
- `src/app/ui/scrolling/display.rs`
- `src/app/domain/buffer/document.rs`
- `src/app/domain/buffer/piece_tree.rs`
- `src/app/services/search.rs`
- `src/app/app_state/search_state/`

## Recommendation 1. Make Visible-Range Layout the Primary Editor Path

Refterm renders a screen-sized cell buffer, not the entire scrollback as a fresh text layout each frame. Scratchpad should take the same idea at the editor level.

Target behavior:

- derive the visible logical/display row range first
- extract only that range from the piece tree, plus small overscan
- build layout for that slice
- paint the slice at the correct scroll offset
- keep whole-document row metrics separate from whole-document glyph layout

This would turn `DisplaySnapshot` from "metadata derived after full galley layout" into "the thing that lets us avoid full galley layout."

The likely shape:

1. Add a `ViewportTextSlice` built from piece-tree line metadata and scroll state.
2. Build an egui `LayoutJob` only for that slice.
3. Translate cursor/search/selection ranges from document offsets into slice-local offsets.
4. Keep a sparse row metric cache for scroll height, max line width, and logical-line lookup.
5. Fall back to full-galley behavior only for small files or while the slice cache is cold.

This is the highest-value refterm transfer.

## Recommendation 2. Cache Layout Runs Like Refterm Caches Glyph Runs

Refterm caches glyph rasterization by hashed glyph run. Scratchpad can cache text layout segments by a Rust-friendly key:

- document revision
- piece/line identity
- byte or char range
- font ID and font size
- wrap width
- highlight/search/selection generation
- theme colors that affect text formats

The goal is not to cache every glyph. In egui, the practical unit is likely a line, wrapped display row, or contiguous style run. The cache should answer: "Can this visible line be reused without rebuilding the `LayoutJob` segment?"

Useful first cache:

- cache plain line layout for unhighlighted visible rows
- invalidate by document generation, wrap width, and font ID
- overlay cursor/selection/search highlights separately where possible

That mirrors refterm's useful division: stable glyph/text content is cached; volatile cell/style state remains cheap to update.

## Recommendation 3. Keep the ASCII Fast Path Explicit

Refterm scans 16-byte chunks to find newline, escape, and high-bit bytes before using heavier parsing. Scratchpad already records `Piece::is_ascii` and uses ASCII shortcuts in `CharByteMap`, `char_at`, and search match conversion.

Extend that discipline:

- line slicing should avoid `chars()` when a piece is ASCII
- word-boundary helpers should use byte classification for ASCII spans
- highlight boundary conversion should reuse char/byte maps across adjacent visible lines
- search should prefer chunk-level ASCII paths before regex or Unicode-aware fallback
- viewport extraction should preserve piece spans instead of eagerly flattening to `String`

Do not start with SIMD. Start by ensuring ASCII paths stay allocation-free and avoid char iteration when byte offsets are valid. SIMD can come later behind measurement.

## Recommendation 4. Batch Work Across UI and Worker Boundaries

Refterm's pipe findings are portable: throughput suffers when fixed-cost handoffs happen too often. Scratchpad's equivalent boundaries are:

- search request construction
- worker dispatch
- file open/decode/install
- save/persist
- layout cache warming
- search-result preview generation

Batching rules for Scratchpad:

- dispatch search by stable document snapshots, not copied full strings
- group small files or line chunks into coarse work units
- avoid per-match UI updates; publish bounded result batches
- avoid per-line worker messages for layout or metadata
- preserve cancellation and stale-result rejection by revision/generation

This fits the existing search-dispatch profiling direction in `docs/parallelism-performance-plan.md`.

## Recommendation 5. Separate Text Storage, Display Records, and Paint

Refterm's renderer consumes `renderer_cell` records. It does not ask the semantic terminal parser to draw pixels directly.

Scratchpad should keep sharpening a similar separation:

- piece tree: canonical text and edit history
- document snapshot: stable read view for workers and UI preparation
- display model: row/line/wrap metrics and visible slices
- paint model: layout/cache records ready for egui painting
- interaction model: cursor, selection, anchors, and hit testing translated through display records

The current architecture has pieces of this already. The remaining risk is that the full egui `Galley` acts as both display model and paint model. That is convenient, but it makes "visible-only" and "incremental invalidation" harder.

## Recommendation 6. Treat egui Text Layout as an Expensive API Boundary

Refterm assumes DirectWrite is expensive and avoids calling it unnecessarily. Scratchpad should treat `fonts.layout_job(job)` the same way.

Measure and optimize:

- number of layout jobs per frame
- text bytes/chars submitted to egui layout per frame
- rows/glyphs generated outside the visible viewport
- time spent building `LayoutJob`
- time spent in `fonts.layout_job`
- cache hit rate for visible line/layout records

Add these to the existing measurement scripts before a large renderer rewrite. A visible-range layout path should prove itself with lower submitted text volume and lower frame stalls.

## Recommendation 7. Use Compact Display Records for Hot Metadata

Refterm's cells are tiny and regular. Scratchpad's equivalent does not need to be GPU cells, but it should be compact:

```text
DisplayRowRecord
  logical_line: u32
  char_range: Range<u32>
  y_top: f32
  height: f32
  wrap_index: u16
  flags: row contains search / selection / non-ascii / long line
```

These records would let scrolling, gutter painting, hit testing, and cursor reveal work without consulting a full galley for the entire document.

`src/app/ui/scrolling/display.rs` already has `row_tops`, `row_logical_lines`, and `row_char_ranges`. The next step is to produce those records without requiring a full-document galley first.

## What Not to Copy

Several refterm ideas should not be copied directly:

- `fast_pipe.h` is terminal-process plumbing, not a Scratchpad concern.
- D3D11-specific rendering does not fit the current eframe/egui stack without a much larger renderer decision.
- Low-level C allocation patterns should not replace safe Rust data structures unless a profile forces the issue.
- Refterm's whole-screen clear/rebuild approach is acceptable for a terminal grid, but Scratchpad should prefer visible-range and incremental invalidation for large documents.

## Suggested Implementation Sequence

1. Add measurement for text submitted to layout per frame.
   Track full text length, visible slice length, layout job count, and `fonts.layout_job` time.

2. Build a read-only visible-slice renderer behind a feature flag or size threshold.
   Start with plain text, no selection, no search highlights. Prove scroll correctness and row positioning.

3. Add selection, cursor, and hit-testing translation.
   Use document-to-slice offset mapping and keep existing full-galley path as fallback.

4. Add search highlights and active result reveal.
   Restrict highlight processing to visible ranges plus overscan.

5. Introduce a line/display-row layout cache.
   Cache by document generation, line/range, font, wrap width, and highlight generation.

6. Move cache warming off the UI path where snapshots make it safe.
   Keep UI mutation single-threaded; prepare immutable records in workers.

7. Retire full-document galley rendering for large files.
   Keep it for small files if it remains simpler and faster there.

## Success Criteria

The refterm-inspired work is successful when:

- opening and scrolling large plain-text files does not require full-document layout per frame
- visible rows render from bounded text slices
- layout submitted per frame is proportional to viewport size, not file size
- selection/search/cursor behavior remains correct across wrapped lines
- search dispatch and worker scanning avoid unnecessary full-string clones
- measurement reports show lower UI-thread frame stalls

## Bottom Line

Refterm's useful lesson for Scratchpad is discipline around the hot path. It keeps expensive text work behind caches, handles simple bytes cheaply, batches boundary crossings, and renders from compact display records.

For Scratchpad, the practical version is a viewport-first editor renderer backed by piece-tree slices and cached layout records. That would line up with the project's existing direction: performance and complexity metrics guiding a safer, simpler Windows text editor.
