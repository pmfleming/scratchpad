# Refterm Performance Review

This report reviews [cmuratori/refterm](https://github.com/cmuratori/refterm) for why it is quick, with source inspected locally under `target/external/refterm`.

The short version: refterm is quick because it keeps the hot path simple. It moves compact terminal-cell records, caches expensive glyph work, scans ordinary bytes in wide batches before falling into slower parsing, and renders the screen as a regular tile grid. The project README is explicit that refterm is not meant to be maximally optimized; it is a "modern minimum" reference implementation for a reasonable terminal renderer.

## Source Reviewed

- `README.md`
- `faq.md`
- `fast_pipe.h`
- `refterm_example_terminal.c`
- `refterm_example_d3d11.c`
- `refterm_example_source_buffer.c`
- `refterm_example_glyph_generator.c`
- `refterm_glyph_cache.c`
- `refterm.hlsl`

## What "Quick" Means Here

Refterm is not a full terminal and not a polished editor. Its speed comes from avoiding several classes of unnecessary work:

- it does not call a text renderer once per character
- it does not rebuild complex per-character objects for every frame
- it does not run Unicode shaping on ASCII text
- it does not allocate freely in the render loop
- it does not require the CPU to draw every glyph every frame

That distinction matters. Refterm is quick in the paths it was designed to stress: high-throughput terminal output, multicolor text, large scrollback, and frequent redraws of a monospace grid.

## 1. Rendering Is a Cell Grid, Not a Stream of Text Draw Calls

The central renderer abstraction is a compact `renderer_cell` containing a glyph index, foreground color, and background color. The terminal state is converted into an array of these cells, and the renderer uploads that array once per frame.

In `refterm_example_d3d11.c`, `RendererDraw` maps a dynamic cell buffer, copies the terminal cells into it, binds the cell buffer and glyph texture, then renders either through a compute shader or a single full-screen draw. The important lines are:

- `SetD3D11MaxCellCount` allocates a structured dynamic buffer sized by `Count * sizeof(renderer_cell)`.
- `RendererDraw` maps the constant buffer and cell buffer with `D3D11_MAP_WRITE_DISCARD`.
- the cell data is uploaded with two `memcpy` calls to preserve ring-buffer ordering.
- the actual draw is one compute dispatch or one `Draw(..., 4, 0)`.

That is the core performance lesson: once glyphs are in an atlas, the frame is mostly "copy compact cell state, shade pixels." It avoids issuing a text-layout or glyph-rendering call for each run or character on screen.

## 2. Glyph Generation Is Cached Behind Stable Hashes

Windows text APIs are treated as expensive. Refterm does use DirectWrite, but it tries not to call it repeatedly for glyph runs it has already seen.

The glyph path is:

- compute a hash for the glyph run
- look up the run in `FindGlyphEntryByHash`
- if missing, measure/rasterize via DirectWrite
- transfer the resulting tile into the GPU glyph texture
- store the GPU tile index in the terminal cell

Relevant source:

- `refterm_glyph_cache.c`: `FindGlyphEntryByHash`, `UpdateGlyphCacheEntry`
- `refterm_example_glyph_generator.c`: `GetGlyphDim`, `PrepareTilesForTransfer`, `TransferTile`
- `refterm_example_terminal.c`: `ParseWithUniscribe`, `ParseLineIntoGlyphs`

The cache is deliberately simple, but it changes the cost model. Common text does not repeatedly pay the full shaping/rasterization cost; it pays a hash lookup and cell write.

## 3. ASCII and Control-Code Scanning Get a Fast Path

The v2 README calls out one real parser optimization: line and VT parsing checks 16-byte blocks for control codes before running the slower parser.

In `ParseLines`:

- `_mm_loadu_si128` reads 16 bytes at a time.
- the batch is compared against newline and escape bytes.
- high-bit bytes are tracked as "complex" text.
- only when a control byte is found does the code fall back to detailed handling.

This is not a sophisticated parser. That is the point. Most terminal output is ordinary text, so refterm makes the ordinary case cheap and reserves expensive parsing for the uncommon case.

## 4. The Source Buffer Is a Large Ring

Refterm keeps output in a large source buffer rather than continually allocating strings. `Terminal->PipeSize` is set to `16 * 1024 * 1024`, and `AllocateSourceBuffer` builds a backing region for scrollback/input storage.

Incoming data goes through:

- `GetPipePendingDataCount`
- `GetNextWritableRange`
- `ReadFile`
- `CommitWrite`
- `ParseLines`

The important behavior is that reads fill a writable slice of an existing buffer, then parsing records line metadata over that buffer. The data itself is not repeatedly copied into a sequence of short-lived strings.

## 5. Input Throughput Depends on Batch Size

The FAQ and README separate renderer speed from pipe speed. Refterm originally used `fast_pipe.h` to bypass conhost, but the v2 README says later testing found conhost can be within roughly 10% of the fast-pipe path when it receives large writes.

That finding is useful beyond terminals: call count can dominate byte count. If a subsystem has a high fixed handoff cost, small writes or small tasks can destroy throughput even when the total data volume is reasonable.

For Scratchpad, the direct conhost lesson is not relevant. The portable lesson is: batch work at subsystem boundaries, especially for file IO, search dispatch, UI-to-worker handoff, and layout preparation.

## 6. The Main Loop Is Event-Aware and Latency-Aware

The terminal thread waits on pipe handles and window messages, processes pending input, updates the terminal buffer, lays out visible lines, and draws. It also checks D3D frame latency before continuing to consume more pipe data.

The loop is not architecture astronautics. It is careful sequencing:

- wait until there is either input, process output, or a blink timeout
- drain available process output into existing storage
- parse line metadata incrementally as bytes arrive
- rebuild the screen cell buffer
- submit one compact render pass

This keeps foreground work understandable and measurable.

## 7. Why It Beats Slower Terminal Renderers

Refterm's own FAQ argues that slower terminal renderers often lose because they do too much general-purpose work in the hot path, especially allocation-heavy modern C++ object churn and repeated DirectWrite calls for tiny runs.

The reviewed source supports that diagnosis. Refterm's hot path is less abstract:

- raw arrays for cells and line metadata
- explicit buffers
- simple integer-packed colors and flags
- cache lookup before glyph generation
- batched byte scanning
- one render submission model for the whole grid

That is why a small reference implementation can be several orders faster than a feature-rich terminal on specific throughput tests.

## 8. Limits and Caveats

Refterm is not a general model to copy blindly.

- It is a terminal renderer, not a text editor.
- It is Windows/D3D11-specific.
- It uses low-level C and platform APIs that do not map directly to Scratchpad's Rust/egui stack.
- It intentionally omits many production terminal concerns.
- Several comments in the source call out rough edges, over-clearing, naive layout, and slow DirectWrite paths.

The valuable part is not the exact API choice. It is the repeated pattern: keep the common case compact, cached, batchable, and visible to measurement.

## Takeaways

The main performance ideas worth carrying forward are:

1. Render from compact display records, not repeatedly from the full semantic text model.
2. Cache expensive text shaping/layout/rasterization work behind stable keys.
3. Detect plain ASCII/simple text quickly and bypass heavier Unicode logic.
4. Batch work across subsystem boundaries.
5. Keep hot-path data structures flat enough that copying and scanning are predictable.
6. Treat third-party text/layout APIs as expensive until measurement proves otherwise.
7. Measure renderer throughput separately from input/IO throughput.

Refterm is quick because it protects the hot path from unnecessary richness. That is the lesson Scratchpad can use.
