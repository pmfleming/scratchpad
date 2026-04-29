# Refterm Fast Read And Output Report

Date: 2026-04-29

## Purpose

This report explains how `cmuratori/refterm` reads and displays large streams quickly, and what parts of that design are worth adapting to Scratchpad's Rust editor architecture.

The short version: refterm is fast because it batches aggressively, keeps terminal output as byte ranges for as long as possible, records cheap line metadata, renders through a fixed-size cell buffer plus glyph cache, and avoids per-character operating-system or DirectWrite calls in the hot path. It is not primarily fast because of a fancy file loader. The file-output examples are simple, but they make the most important point: large contiguous reads and writes matter more than small API differences.

## Upstream Sources Reviewed

- `README.md`: describes refterm as a simple tile renderer, UTF-8-only, and a "modern minimum" performance baseline.
- `faq.md`: separates terminal speed into renderer speed and Windows pipe speed, and explains why large writes through conhost are crucial.
- `splat.cpp`: uses `fread`/`fwrite` with a 64 MiB buffer to dump file bytes to stdout.
- `splat2.cpp`: uses Win32 `CreateFileA`, `ReadFile`, and `WriteFile` with a 64 MiB buffer.
- `refterm_example_source_buffer.c`: implements the scrollback source buffer as a double-mapped circular buffer.
- `refterm_example_terminal.c`: reads pending pipe bytes in batches, parses lines with SIMD scanning, rebuilds visible cells from line metadata, and renders only the terminal screen.
- `refterm_example_d3d11.c` and `refterm.hlsl`: upload a compact cell array and render text as cached glyph tiles on the GPU.

## What Refterm Optimizes

Refterm has two related but distinct fast paths.

### 1. File-to-output throughput

The `splat` tools are intentionally simple:

- Allocate one large 64 MiB buffer.
- Open the file in binary mode.
- Read one large chunk at a time.
- Write the same chunk directly to stdout.
- Avoid line-by-line processing, formatting, decoding, and per-byte work.

`splat.cpp` does this through stdio. `splat2.cpp` does it through Win32 handles. The important commonality is not stdio versus Win32; it is that both paths move large contiguous blocks and make very few calls into the OS or conhost.

Refterm's README notes that on Windows, conhost can be within roughly 10 percent of the fast-pipe path if it receives large writes. The FAQ explains the earlier performance trap: many small writes, especially through text-mode stdio and VT processing, make conhost look much slower than it has to be.

### 2. Terminal display throughput

The actual refterm renderer handles command output, not arbitrary editor files. Its throughput comes from a pipeline that avoids doing expensive work per character on every frame:

1. Read all currently pending pipe bytes in one batch.
2. Append them to a fixed-size source buffer.
3. Parse line boundaries and VT-relevant markers into small metadata records.
4. Keep the raw bytes in scrollback instead of expanding everything into owned line strings.
5. For each frame, rebuild only a bounded number of visible terminal lines into a fixed cell grid.
6. For simple ASCII characters, use pre-rasterized direct glyph tiles.
7. For complex Unicode runs, hash and cache rendered glyph tiles.
8. Upload a compact `renderer_cell` array to the GPU and let a shader expand cells into pixels.

That design changes the main cost from "call text rendering for each run or character" into "copy a compact cell buffer and draw one screen-sized pass."

## Core Techniques

### Large I/O Batches

Refterm's file examples use a 64 MiB transfer buffer. The terminal path uses `PeekNamedPipe` to discover pending output, then reads that amount into the scrollback buffer with one `ReadFile` call where possible.

The lesson is simple: do not accidentally turn one large file or stream into thousands of tiny reads, writes, allocations, or UI messages.

### Double-Mapped Circular Buffer

`refterm_example_source_buffer.c` allocates scrollback as a circular buffer, then maps the same memory twice back-to-back. That means a range that wraps around the end of the ring can still be addressed as one contiguous span.

This matters because the rest of the parser can consume `source_buffer_range { AbsoluteP, Count, Data }` without constantly checking for wraparound. It is a performance trick, but it is also a simplification trick.

### Byte-First Line Metadata

Refterm stores scrollback bytes and keeps a separate ring of `example_line` records:

- first byte position
- one-past-last byte position
- whether the line contains complex characters
- starting glyph properties

The display path can reconstruct visible lines by seeking through metadata, not by owning one string per line or reparsing all previous output.

### SIMD Control-Code Scanning

Line parsing scans 16 bytes at a time looking for newline, escape, and high-bit bytes. Most command output is simple ASCII, so the parser can skip over large plain regions quickly and only fall into detailed parsing when it sees a control byte.

This is a useful idea for Rust too: the common text case should be scanned with byte-oriented routines such as `memchr`/`memchr2`/`memchr3`, or with an equivalent SIMD-backed crate, before paying full Unicode or formatting costs.

### Fixed Cell Buffer

Refterm does not ask DirectWrite to draw arbitrary text every frame. It maps terminal content into a fixed `DimX * DimY` cell array containing:

- glyph index
- foreground color
- background color and flags

Rendering then becomes a bounded screen operation. Even if scrollback is huge, one frame only uploads and draws the visible cell grid.

### Glyph Cache And Direct ASCII Path

ASCII printable characters are pre-rasterized into reserved glyph slots. More complex Unicode runs are hashed and cached in a GPU texture. The expensive glyph-generation path runs on cache miss, not for every frame.

For an editor, this maps less directly because egui currently owns text layout, but the principle still matters: shape/cache reusable display rows or glyph runs, and make ASCII/plain-text lines cheap.

### Frame-Latency Throttling

Refterm waits on input handles and the swapchain frame-latency object. It does not spin and redraw blindly when no new data is present. It also tries to avoid getting far ahead of the renderer.

For Scratchpad, the analogous rule is to avoid rebuilding full-document layout every frame when the document, wrap width, font, and viewport have not changed.

## What Is Not Directly Transferable

Refterm is a terminal renderer, not a full text editor.

- It assumes UTF-8 command output; Scratchpad supports multiple input encodings and BOM preservation.
- Its scrollback is lossy and fixed-size; Scratchpad must preserve complete editable documents.
- Its display model is terminal cells; Scratchpad needs proportional UI integration, selections, cursor navigation, search highlights, wrapping, save fidelity, and edit history.
- It uses D3D11 directly; Scratchpad is currently built on `eframe`/`egui`.

The correct adaptation is therefore architectural, not literal. Scratchpad should adopt the same batching and viewport-first discipline while keeping its document/editor semantics.

## Scratchpad Current State

Scratchpad already has several pieces that point in the right direction:

- File opens run on a background I/O lane.
- File decoding is streaming chunk-by-chunk instead of a single `read_to_string` call.
- `PieceTreeLite` stores text as chunked pieces with cached byte, char, and newline metrics.
- Editor scroll state is view-local.
- The UI has `DisplaySnapshot`, `ViewportSlice`, and `PublishedViewport` concepts.
- The native editor paints a visible slice galley when possible and falls back to full-galley paint on degradation.

The largest remaining mismatch with refterm's model is that the native editor still builds a whole-document egui galley before extracting a viewport slice. That means large-file display still pays a whole-document layout cost up front, even when only a small region is visible.

There is also a secondary load-path issue: `read_document_with_encoding` streams decoded chunks, but each chunk is inserted into `TextDocument` through repeated piece-tree mutation. That is better than blocking the UI thread, but for large initial loads the fastest path should build the initial piece tree directly from decoded chunks or from one accumulated original buffer without edit-style insertion overhead.

## Rust Implementation Direction

### Phase 1: Establish Baselines

Before changing architecture, measure these separately:

- raw file read throughput
- decode throughput by encoding
- initial document construction time
- line metadata construction time
- time to first visible paint
- time to full metadata readiness
- scroll latency after load
- memory peak for 100 MiB, 500 MiB, and 1 GiB text files

This mirrors refterm's distinction between pipe throughput and renderer throughput. A single "open file time" number will hide the real bottleneck.

### Phase 2: Optimize Initial Load Construction

Keep the current background I/O design, but add a faster initial-build path:

1. Read raw bytes in larger chunks than 16 KiB. Start with 1 MiB or 4 MiB and benchmark.
2. Decode into reusable output buffers.
3. Accumulate decoded chunks as original immutable storage.
4. Build `PieceTreeLite` leaves directly from those chunks.
5. Compute line counts and artifact summaries while bytes/chars pass through the loader.

For UTF-8 files that pass prefix inspection, consider a specialized fast path:

- read the file into `Vec<u8>` with large buffered reads or memory mapping
- validate UTF-8 once
- convert to `String` without extra copies where safe
- build the original piece tree from the resulting storage

For non-UTF-8 encodings, keep streaming decode, but avoid edit-style insertion during initial construction.

### Phase 3: Add Byte-Oriented Line Indexing

Refterm's line table is the biggest conceptual fit for Scratchpad.

Add a document-level line index that records enough to find viewport text without laying out the whole document:

- logical line start char
- logical line start byte when available
- newline style/length
- flags for ASCII-only, tab/control presence, ANSI/control artifacts, and non-ASCII text
- optional approximate display width hints

This can initially be built in the background after open, then updated incrementally after edits. It should not replace the piece tree; it should be a navigation/index layer over it.

### Phase 4: Render From Visible Lines First

The most important UI change is to invert the current layout order.

Current hot path:

1. Extract or borrow the whole document text.
2. Build a whole-document egui galley.
3. Build a display snapshot from that galley.
4. Extract a visible character range.
5. Build a smaller galley for painting.

Target hot path:

1. Use scroll position and row/line index to estimate the visible logical line range.
2. Extract only those piece-tree spans plus overscan.
3. Build an egui galley for that slice.
4. Publish viewport metadata from the slice.
5. Refine row mapping as wrapping information becomes available.

This follows refterm's core rule: huge scrollback/document state exists, but the frame renderer only handles the bounded visible region.

### Phase 5: Cache Display Rows Or Line Galleys

Once visible-first rendering exists, cache at the line or display-row level:

- key by document revision, line range, wrap width, font id, tab width, and highlight revision
- keep ASCII/plain lines on a very cheap path
- invalidate only affected lines after edits
- preserve cursor and selection overlays separately where possible

This is the editor equivalent of refterm's glyph cache: expensive layout should happen because text or styling changed, not merely because a frame occurred.

### Phase 6: Optional Memory Mapping

Memory mapping can help large UTF-8 loads, but it should be introduced only after the visible-first pipeline is in place.

Recommended Rust options:

- `memmap2` for cross-platform file mapping
- `memchr` for fast newline/control-byte scanning
- `bstr` only if byte-string ergonomics become valuable

Memory mapping is not a substitute for viewport rendering. Mapping a 1 GiB file and then building a 1 GiB galley is still the wrong shape.

### Phase 7: Consider A Terminal-Like Cell Backend Only As A Separate Experiment

A D3D/wgpu terminal-cell renderer could be very fast for plain monospace text, but it would be a major frontend change. It should not be the first implementation step.

If explored later, define it as a rendering backend experiment:

- fixed monospace cell grid
- direct glyph atlas
- foreground/background color buffers
- separate cursor/selection overlay
- fallback to egui for dialogs and chrome

That path may be attractive for very large plain-text inspection mode, but it is more invasive than improving loading and visible-first layout.

## Concrete Recommendations

1. Add a benchmark family named `large_file_first_paint` that records file read, decode, document build, first viewport paint, and full metadata completion as separate timings.
2. Add `memchr` and use byte scanning for newline/control detection in load and metadata passes.
3. Replace initial-load repeated `insert_direct` calls with a bulk `TextDocument::from_chunks` or `PieceTreeLite::from_chunks` builder.
4. Build a persistent logical line index over the piece tree.
5. Change the native editor to build the first galley from a visible line estimate, not from the whole document.
6. Keep `DisplaySnapshot`/`PublishedViewport`, but make them outputs of the visible slice path rather than products of whole-document layout.
7. Cache visible line/display-row layout by document revision and viewport parameters.
8. Keep full metadata and encoding compliance refreshes in the background; do not block first paint on complete-document analysis.

## Risks And Tradeoffs

- Visible-first rendering with word wrap needs careful scroll anchoring because display row count is not known globally until enough lines are measured.
- Non-UTF-8 encodings still require decode before editable text exists; the fast path should specialize UTF-8 without regressing other encodings.
- A line index must be updated correctly after edits, or status bar, gutter, search preview, and cursor navigation will disagree.
- egui's galley model may limit how much can be cached without deeper integration.
- Memory mapping has platform and file-lifetime edge cases, especially on Windows if files are edited externally.

## Suggested Success Criteria

The refterm-inspired work should be considered successful when Scratchpad can:

- open a large UTF-8 text file to first visible paint without laying out the full document
- keep UI responsive while full metadata and compliance checks continue in the background
- scroll through large files with frame cost proportional to visible rows plus overscan
- preserve current editing, search, encoding, BOM, and save semantics
- show benchmark evidence that time-to-first-paint and scroll latency improve independently of raw file-read speed

## Bottom Line

Refterm's speed comes from doing less work at the point of display: large I/O batches, byte-first parsing, compact metadata, bounded visible rendering, and caching of expensive glyph work.

For Scratchpad, the best Rust implementation is not a literal port of refterm's D3D terminal renderer. It is a staged move to bulk initial document construction, fast byte scanning, a first-class line index, and true visible-first galley construction. That preserves the editor's richer semantics while adopting the part of refterm that matters most: never make the frame renderer pay for the whole file when the user can only see one viewport.