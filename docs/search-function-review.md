# Search Function Review

Review and improvement plan for search and replace in Scratchpad.

Date: 2026-05-06

This rewrite is based on three product assumptions:

1. Scratchpad is a pure text editor. Search should optimize open text buffers, not project-wide code intelligence.
2. Prefer single paths. Avoid special engines for encoding, file size, current tab vs all tabs, contiguous vs fragmented buffers, and similar cases unless measurement proves the common path cannot carry them.
3. When there is a tradeoff, choose the higher-capacity option: bigger files, more open tabs, more panes, more matches, more reliable cancellation, and less UI blocking.

---

## 1. Research Summary

The useful edge of modern editor search is not one magic algorithm. It is a stack:

- A scalable text buffer that can expose stable snapshots without copying the whole document.
- A fast literal/regex engine that works over byte slices and streams results.
- Chunking with overlap so large and fragmented buffers use the same path as small buffers.
- Prioritized scheduling so first useful results appear early, not only after every non-match has been scanned.
- Incremental result publication so the UI does not clone or render the world every frame.
- Replace planning that validates target revisions and matched text before touching buffers.

### Zed: Search Latency Is Scheduling, Not Just Throughput

Zed's 2025 project-search writeup is the most relevant current editor note. Their old pipeline had reasonable total throughput, but poor time-to-first-result because tasks that merely checked whether files might match could starve tasks that already had a known matching file. The fix was a prioritized concurrent pipeline: confirm and publish matches from known matching buffers before spending more scheduler time on new candidate scans.

The lesson for Scratchpad is direct: partial results are not enough if they are cumulative, delayed, or scheduled behind lower-value scans. The worker should bias toward visible progress:

1. Search the active/visible target first.
2. Publish first matches quickly.
3. Continue scanning other targets in deterministic order.
4. Avoid flooding the UI with every tiny partial update.

Source: https://zed.dev/blog/nerd-sniped-project-search

### VS Code: Capacity Starts In The Text Buffer

VS Code's piece-tree writeup remains the best public editor-buffer reference. The important point is not that every editor must copy VS Code's implementation; it is that large-file search depends on the buffer representation. A piece table/tree with line-break metadata avoids splitting text into a line object per row, avoids large concatenation traps, and supports efficient offset/line lookup from subtree metadata.

Scratchpad already has a piece tree and `DocumentSnapshot`, so the right direction is to push search closer to that structure rather than adding a separate large-file engine. Search should consume snapshot chunks from the piece tree, return char ranges, and keep preview/line metadata as derived output.

Source: https://code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation

### Regex Engines: Prefer Bounded, Predictable Automata

Rust's `regex` stack is a good fit for an editor because it avoids catastrophic backtracking. The lower-level `regex-automata` crate exposes multiple engines, including lazy DFAs that build transition tables incrementally and stay under a configured memory capacity. That model matches the product assumption: choose one high-capacity path with predictable bounds instead of a permissive regex mode that can hang the editor.

The current Scratchpad rule that regex search must have a bounded maximum match length is consistent with chunked search. Keep it. It makes overlap deterministic.

Source: https://docs.rs/regex-automata/latest/regex_automata/hybrid/index.html

### SIMD And Multi-Pattern Search Are Tools, Not The Architecture

`memchr`/`memmem`, Aho-Corasick, Teddy-style SIMD literal search, and Hyperscan-style multi-regex engines show where raw matching throughput can go. They matter most for large misses and many simultaneous patterns. Hyperscan is impressive, but it is not a natural first dependency for a cross-platform text editor search box: it is native, CPU-feature-sensitive, and built around compiled pattern databases.

The practical path is:

- Use `memmem` for literal case-sensitive search.
- Use Aho-Corasick or a small lowercase-prefilter strategy for ASCII case-insensitive search.
- Keep Rust `regex`/`regex-automata` semantics for regex mode.
- Treat heavier SIMD/multi-pattern engines as future measured upgrades, not as separate product paths.

Source: https://www.intel.com/content/www/us/en/developer/articles/technical/introduction-to-hyperscan.html

---

## 2. Current Search Shape

In-scope code:

| Area | Path |
| --- | --- |
| Core search API | `src/app/services/search.rs` |
| Matcher implementation | `src/app/services/search/matchers.rs` |
| Search session state | `src/app/app_state/search_state.rs` |
| Runtime orchestration | `src/app/app_state/search_state/runtime.rs` |
| Worker scheduling | `src/app/app_state/search_state/worker.rs` |
| Chunked search | `src/app/app_state/search_state/fragments.rs` |
| Results and replace helpers | `src/app/app_state/search_state/helpers.rs` |
| Replace execution | `src/app/app_state/search_state/replace.rs` |
| Search UI | `src/app/ui/search_replace/*.rs` |

The current implementation is already moving in the right direction:

- Search is snapshot-based, so worker threads do not mutate editor state directly.
- Cancellation uses a generation counter.
- Requests are coalesced when the user types quickly.
- Results can stream partially.
- Large/non-contiguous buffers can be searched in chunks.
- Replace-all validates target revision and matched text before applying edits.
- Search coordinates are character ranges, which match editor behavior.

The main issue is that the implementation still has several accidental forks and capacity leaks: regex compilation per chunk, separate contiguous vs fragmented behavior, per-partial cloning, per-call maps, and UI result limits that are display caps rather than a true virtual result model.

---

## 3. Target Architecture

### Principle A: One Search Pipeline

Every search should go through the same conceptual pipeline:

```text
SearchSession
  -> collect text targets
  -> build one SearchProgram for the query/options
  -> stream DocumentSnapshot chunks
  -> run SearchProgram over chunks
  -> merge ordered match ranges
  -> publish result deltas
  -> derive previews/highlights lazily
```

Small, contiguous documents may still hit fast internal branches, but those branches should be hidden inside the same pipeline. The caller should not choose a "large file path" or "fragmented path."

### Principle B: Compile Once, Scan Many

Create a per-request `SearchProgram`:

```rust
enum SearchProgram {
    Literal(LiteralProgram),
    Regex(RegexProgram),
}
```

It should hold:

- query mode
- case and whole-word options
- compiled regex, if needed
- maximum match length / chunk overlap
- reusable scratch capacity where practical

This removes repeated parsing/building and makes the engine easier to benchmark.

### Principle C: Bytes For Matching, Chars For Editor Coordinates

Matching engines want byte slices. The editor wants char ranges. That is fine, but conversion should be lazy and proportional to matches, not text length.

Recommended rule:

- Search chunks as UTF-8 bytes/str slices.
- Return byte ranges from the matcher.
- Convert only accepted matches into global char ranges using chunk-local metadata.
- For ASCII chunks, byte range equals char range.
- For non-ASCII chunks, use chunk metadata or on-demand counting, not a full `byte_to_char_map` for every search call by default.

This keeps Unicode correctness while avoiding per-chunk `Vec<usize>` allocation on misses.

### Principle D: Capacity-Oriented Scheduling

The worker should optimize both throughput and time-to-first-visible-result:

1. Scan the active buffer/view first.
2. Then scan other visible buffers in the active tab.
3. Then scan other open tabs.
4. Within each target, use the same chunk stream.
5. Publish early deltas, then throttle later UI updates.

This follows the Zed lesson: schedule known useful work ahead of broad candidate work. For Scratchpad, all targets are open buffers, so we do not need a project-file candidate phase. We still need priority between active text and background open tabs.

### Principle E: Replace Is A Plan, Not A Button Handler

Search and replace should share the same session model:

```rust
struct ReplacementPlan {
    generation: u64,
    query: String,
    replacement: String,
    options: SearchOptions,
    targets: Vec<ReplacementTargetPlan>,
    total_match_count: usize,
}
```

The plan should validate:

- target buffer identity
- target revision
- expected matched text or short match fingerprint
- writable state
- overlap/order invariants

Execution can still apply per buffer in reverse range order. The important thing is that UI commands do not directly infer replace behavior from a visible result row.

---

## 4. Findings And Plan

## 4.1 Build `SearchProgram` Once Per Request

Current issue:

- Regex validation and regex execution both compile/parse.
- Fragmented regex search recompiles for each chunk because `search_text_interruptible` receives only query/options.
- Regex max-match length is parsed separately from regex compilation.

Plan:

- Add a compiled `SearchProgram` before target scanning starts.
- Store bounded-regex metadata in the program.
- Pass `&SearchProgram` to `search_target_ranges`.
- Keep `search_text` as a simple public helper for tests and small call sites, implemented by constructing a temporary program.

Capacity win:

- Large regex searches scale by chunks scanned, not by chunks times regex compilation.
- The same engine serves current buffer and all open tabs.

## 4.2 Replace Per-Chunk Full Maps With Lazy Coordinate Conversion

Current issue:

- `byte_to_char_map`, `char_to_byte_map`, and `WholeWordMatcher` can allocate per chunk.
- Miss-heavy searches pay these costs even when there are no matches.

Plan:

- Make matcher internals return byte ranges plus enough local metadata to convert accepted matches.
- For whole-word checks, inspect neighboring characters from the text slice only at candidate match boundaries.
- Add chunk helper APIs on `DocumentSnapshot` or chunk structs for byte-to-char conversion.
- Reuse scratch buffers only behind the single pipeline, not as a separate file-size path.

Capacity win:

- Large Unicode files and large miss-heavy searches stop allocating per chunk as a baseline cost.

## 4.3 Use Proven Literal Search Primitives

Current issue:

- ASCII literal search uses hand-written byte loops.
- This is simple, but leaves performance on the table for long text and no-match scans.

Plan:

- Use `memchr::memmem::Finder` for case-sensitive literal search.
- Keep single-byte search on `memchr` where useful.
- For ASCII case-insensitive search, measure Aho-Corasick with ASCII case-insensitive config against the current first/last-byte filter.
- Keep Unicode case-insensitive in the same `SearchProgram` path, even if the internal algorithm differs.

Capacity win:

- Better throughput for big files and many tabs without making a separate "large file search."

## 4.4 Publish Result Deltas, Not Full Snapshots

Current issue:

- `SearchResultAccumulator::partial_snapshot` clones the full match list and visible groups.
- Many partials across many targets can become quadratic.
- UI state clones visible result groups per frame.

Plan:

- Change worker messages to either:
  - `SearchResultDelta { generation, new_matches, new_groups_or_entries, progress }`
  - and `SearchResultComplete { generation, final_status }`
- Store result groups behind `Arc<[SearchResultGroup]>` or keep an append-only result model with a generation.
- Throttle UI publication by time and match-count thresholds after the first visible result.

Capacity win:

- More tabs and more matches do not punish every partial update and every UI frame.

## 4.5 Make The Visible Result Limit A Virtualization Policy

Current issue:

- Results are capped at 200 visible entries.
- Navigation can cover more matches than the result list displays.
- The cap is useful for UI safety but not a coherent capacity model.

Plan:

- Keep a high or unbounded internal match list subject to memory budget.
- Virtualize result entries by visible row range.
- Generate previews lazily for visible rows plus a small overscan.
- Show explicit progress and truncation only if a real safety budget is reached.

Capacity win:

- The app can handle many matches without either hiding them arbitrarily or trying to render all previews.

## 4.6 Prioritize Active And Visible Text

Current issue:

- Target parallelism exists, but the plan should be explicit about target order and first-result latency.
- Parallel workers currently preserve final order through pending maps, but scheduling does not express a UX priority hierarchy as clearly as it could.

Plan:

- Order targets by:
  1. active view
  2. other visible views in the active tab
  3. other buffers in the active tab
  4. other open tabs
- Let active target stream partial matches first.
- Use worker queues that prefer finishing/publishing already-started target scans before starting lower-priority targets.

Capacity win:

- Huge all-tabs searches feel responsive even if total completion takes time.

## 4.7 Keep Regex Bounded, But Make Replacement Useful

Current issue:

- Regex matching exists, but replacement currently treats the replacement string literally.
- Users expect `$1`, `$2`, and named captures in regex replacement.

Plan:

- Extend `SearchProgram::Regex` to support capture expansion for replacement planning.
- Store enough per-match capture data during planning, or re-run the compiled regex against the matched slice during plan construction.
- Keep bounded-regex search for chunking. Do not add backtracking-only features that break predictability.

Capacity win:

- Adds expected editor functionality without weakening the high-capacity search contract.

## 4.8 Replace `matched_text: String` With A Smaller Validation Token

Current issue:

- Every `SearchMatch` stores owned matched text.
- This is only needed later for replace validation.

Plan:

- Store match length plus a short hash/fingerprint, or store matched text only inside a `ReplacementPlan`.
- On replace, re-extract the current target range and validate against the fingerprint or expected text.
- For regex capture replacement, construct expected replacement text during planning rather than during initial search display.

Capacity win:

- Large result sets use less memory when the user is only searching.

## 4.9 Unify Status And Freshness

Current issue:

- Search status, progress, freshness, dirty state, and replace eligibility can drift.

Plan:

- Make `SearchSession` the single source of truth:

```rust
enum SearchStatus {
    Idle,
    Searching { scanned_targets: usize, total_targets: usize },
    Ready,
    NoMatches,
    InvalidQuery(String),
    Stale,
    Error(String),
}
```

- Derive UI labels and replace enablement from this state.
- Mark results stale immediately when an underlying buffer revision changes after displayed generation.

Capacity win:

- More background work does not create ambiguous UI state.

## 4.10 Scope The Product: Open Text Buffers, Not Project Search

Current issue:

- Some previous plans discuss providerization and disk/project search.
- Under the pure text editor assumption, those are distractions unless the app later changes scope.

Plan:

- Make open text buffers the explicit search universe:
  - selection
  - active buffer
  - active tab
  - all open tabs
- Do not build project/disk search now.
- Do not build language-aware or symbol-aware search now.
- Keep the internal target collector simple enough that future disk search can be added later without shaping today's design around it.

Capacity win:

- Engineering effort goes into larger text capacity and better replace safety inside the actual product.

---

## 5. Recommended Implementation Phases

### Phase 1: Program And Pipeline

Goals:

- Introduce `SearchProgram`.
- Compile/validate once per request.
- Pass compiled program through worker and fragments.
- Preserve existing UI behavior.

Done when:

- Regex is not compiled per chunk.
- Plain text and regex use the same request/target/chunk pipeline.
- Existing profile binaries still run.

### Phase 2: Hot-Path Allocation Reduction

Goals:

- Remove full per-chunk char maps from miss-heavy paths.
- Replace `WholeWordMatcher`'s full `Vec<char>` with boundary inspection.
- Add `memmem` for case-sensitive literal matching.

Done when:

- Large ASCII and Unicode miss benchmarks show lower allocation and equal or better throughput.
- Whole-word behavior has edge-case tests.

### Phase 3: Delta Results And UI Virtualization

Goals:

- Publish deltas instead of cloned partial snapshots.
- Avoid cloning result groups each frame.
- Generate previews lazily for visible rows.

Done when:

- Searching many open tabs does not create O(n squared) partial-result cloning.
- The result list can represent more than 200 matches without rendering all previews.

### Phase 4: Priority Scheduling

Goals:

- Make active/visible target priority explicit.
- Publish first active-buffer results as soon as possible.
- Throttle later updates to protect the UI.

Done when:

- All-tabs search shows active-buffer matches quickly even when many other tabs are large.
- Worker progress includes scanned/total target counts.

### Phase 5: Replace Planning

Goals:

- Move replace through a `ReplacementPlan`.
- Support regex capture expansion.
- Store validation data in the plan, not every display match.
- Keep reverse-order per-buffer application.

Done when:

- Replace current and replace all use the same plan model.
- Stale or changed targets are blocked with a clear status.
- Regex replacement supports capture references.

### Phase 6: Capacity Tests

Goals:

- Add repeatable tests and benchmarks for bigger files, more tabs, and fragmented piece trees.

Suggested cases:

- 1 MB, 50 MB, and 250 MB single-buffer literal search.
- Many-line file with millions of short lines.
- All-open-tabs search across 10, 100, and 500 buffers.
- Fragmented piece tree after many edits.
- Regex bounded search across chunks.
- Cancellation while scanning a large miss.
- Replace all after intervening buffer edit should block.

Done when:

- Capacity regressions are visible before release.

---

## 6. Priority List

| Priority | Item | Why |
| --- | --- | --- |
| 1 | Compile one `SearchProgram` per request | Removes repeated regex work and creates the single pipeline foundation |
| 2 | Return byte ranges internally and convert accepted matches lazily | Cuts per-chunk allocation on large files |
| 3 | Replace whole-word `Vec<char>` with boundary checks | Makes whole-word affordable at scale |
| 4 | Use `memmem` for literal case-sensitive search | Better miss throughput with a proven primitive |
| 5 | Publish result deltas instead of full partial snapshots | Prevents cumulative clone cost across many targets |
| 6 | Cache/virtualize result groups and previews | Allows many matches without UI churn |
| 7 | Prioritize active and visible targets | Improves perceived performance on all-tabs searches |
| 8 | Move replace through `ReplacementPlan` | Makes replace safer and unlocks capture expansion |
| 9 | Add regex replacement captures | Expected search/replace functionality |
| 10 | Add capacity benchmarks | Keeps the high-capacity choice honest |

---

## 7. Explicit Non-Goals For This Plan

- Disk/project search.
- Language-aware search.
- AST/symbol search.
- Per-encoding search engines.
- A separate large-file search mode.
- Backtracking regex features that can hang the editor.
- A native SIMD dependency before `memmem`/Aho-Corasick paths are measured.

These can be revisited later, but they conflict with the current assumptions.

---

## 8. First Slice

The smallest high-value slice is:

1. Add `SearchProgram`.
2. Compile regex and bounded-match metadata once.
3. Pass `&SearchProgram` into `search_target_ranges` and chunk processing.
4. Keep public helper functions as wrappers.
5. Add tests proving regex compilation errors, bounded regex rejection, plain search parity, and chunked regex parity.

That slice does not change the product surface, but it aligns the code with the long-term shape: one text-search path with more capacity.
