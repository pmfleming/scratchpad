# Search Function Review

Review of the search subsystem against best practice, speed, utility, and functionality. Code was not modified — this is an analysis report only.

Date: 2026-05-06

---

## 1. Scope & Method

In-scope code (~4,000 lines across 12 files):

| Area | Path |
| --- | --- |
| Core matchers / API | `src/app/services/search.rs`, `src/app/services/search/matchers.rs` |
| Workspace state | `src/app/app_state/search_state.rs` |
| Orchestration | `src/app/app_state/search_state/runtime.rs` |
| Background worker | `src/app/app_state/search_state/worker.rs` |
| Chunked search | `src/app/app_state/search_state/fragments.rs` |
| Helpers / accumulator | `src/app/app_state/search_state/helpers.rs` |
| Replace flow | `src/app/app_state/search_state/replace.rs` |
| UI | `src/app/ui/search_replace/{mod,controls,results,state}.rs` |
| Profiling / benches | `src/profile.rs`, `benches/search_speed.rs`, `src/bin/profile_search_*.rs` |

Findings below cite paths and line numbers from the current `master` (`a9b93a00`). They are observations from reading the code; none have been benchmarked as part of this review.

---

## 2. Architecture Summary

The search subsystem follows a clean layered shape:

1. **Pure matcher layer** (`services/search`) — stateless `find_matches` / `search_text` functions that take a `&str`, query, and `SearchOptions`, returning char-based ranges. Supports plain text and bounded regex, with an optional `should_continue` interrupt callback.
2. **Search session state** (`app_state/search_state.rs`) — durable workspace search session: query, options, scope, active match, status, freshness, replace plan, replace-all confirmation. Communicates with the worker through an mpsc channel and an `AtomicU64` generation counter.
3. **Background worker** (`worker.rs`) — single dedicated `std::thread`. Drains the request channel to coalesce burst typing, then either runs single-threaded with partial result streaming, or fan-outs across up to 4 target threads via `thread::scope`. A second nested layer (`fragments.rs`) shards a single buffer into 64 KB-character chunks and parallelises across up to 4 chunk workers.
4. **Result accumulator** (`helpers.rs`) — incrementally builds `SearchResultGroup`s, capped at 200 displayed entries; computes line/column/preview via the `DocumentSnapshot::previews_for_matches` API.
5. **UI** (`ui/search_replace`) — non-modal floating overlay with find pill, optional replace pill, scope chips, mode/case/whole-word toggles, and a virtualised-style scrollable result list with collapsible per-file groups.

This is a sound shape. The rest of the report focuses on what could be tightened.

---

## 3. Strengths

- **Generation-based cancellation** (`AtomicU64` shared between UI and worker) is the right primitive — lock-free, cheap to poll, easy to reason about across thread::scope joins. (`worker.rs:128`, `fragments.rs:185`).
- **Request coalescing** in the worker drains the channel and uses only the latest request (`worker.rs:60-63`). Eliminates pile-up when the user types quickly.
- **Two-tier parallelism** — across targets *and* across chunks within a target — with sensible caps and minimum-work thresholds (`SEARCH_TARGET_PARALLELISM_MIN_TARGETS=4`, `INTRA_BUFFER_PARALLELISM_MIN_CHUNKS=4`).
- **Specialised matcher fast paths**: ASCII case-sensitive, ASCII case-insensitive single-byte, ASCII case-insensitive multi-byte (with first/last byte filter), and Unicode case-insensitive fallback (`matchers.rs:6-41`).
- **Bounded-regex enforcement** via `regex_syntax::parse(...).properties().maximum_len()` (`search.rs:117-132`) — gives the chunked path a deterministic overlap window and rejects pathological regexes up front.
- **Throttled interrupt checking** (`INTERRUPT_CHECK_INTERVAL = 1024` steps; `matchers.rs:82-111`) — keeps cancellation responsive without adding an atomic load to each character.
- **Zero-copy on contiguous documents** — `piece_tree.borrow_range` is tried first (`fragments.rs:26`), avoiding a `String` allocation for the common case.
- **Reverse-order replacement targets** in `helpers::build_replacement_targets` keep offsets stable without re-mapping.
- **Replace-all guardrails** — `target_revision` plus matched-text re-extraction validate that nothing has shifted between scan and apply (`replace.rs:235-272`).
- **Partial result streaming** during long single-threaded scans gives the UI early matches.
- **Char-based ranges throughout** — Unicode-safe, integrates cleanly with the piece tree.
- **Selection-only scope auto-default with `SearchScopeOrigin`** — the UI can distinguish user-pinned scope from auto-selected, and surface that in tooltips (`controls.rs:445-455`).

---

## 4. Speed Findings

Findings ordered roughly by expected impact.

### 4.1 Regex is recompiled on every chunk (medium-to-high impact)

In fragmented (large or non-contiguous) regex search, every chunk passes through:

```
fragments.rs:147  search::search_text_interruptible(window_text, query, options, ...)
search.rs:113      regex_search_interruptible(...)
search.rs:182      compile_supported_regex(query, options)   // RegexBuilder::build()
```

Same for plain-text but plain-text recompilation is essentially free; regex compilation isn't. For a 1 MB buffer with ~16 chunks at 64 KB chars, the regex is compiled 16 times per scan, plus once during `validate_search_query` and again during the initial `regex_search_interruptible` call.

**Fix**: compile once at the top of `process_search_request_with_partials` (or per target) and pass `&Regex` through to the matcher layer. The matcher API already has a `collect_regex_matches` that takes `&Regex`; expose an entry point that skips recompilation.

### 4.2 `byte_to_char_map` / `char_to_byte_map` allocate per call (medium)

Both helpers (`matchers.rs:369-387`) allocate a `Vec<usize>` sized to the text length on every Unicode/regex search invocation. In fragmented mode this happens **per chunk**, on top of the regex recompilation above. A 64 KB-char chunk costs ~256 KB of `Vec<usize>` allocation each.

**Fix options** (in increasing intrusiveness):

1. Skip the maps entirely when `text.is_ascii()` already proved the chunk is ASCII (the regex path does this; the case-sensitive Unicode path doesn't avoid `byte_to_char_map` even for ASCII because it only checks `text.is_ascii()` once at the top).
2. Convert Match positions on demand using `text[..byte_offset].chars().count()` for the small number of matches actually found — typically far fewer than the chunk's char count.
3. Reuse a thread-local scratch `Vec<usize>` across chunks of the same target (truncate-and-reuse).

### 4.3 Naive ASCII substring scan (medium)

`find_ascii_case_sensitive_matches` and `find_ascii_case_insensitive_multi_byte_matches` (`matchers.rs:278-367`) loop over every starting byte position with a first/last byte filter. The codebase already depends on the `regex` crate, which transitively pulls in `memchr` (and `aho-corasick`). Both expose vectorised substring search:

- `memchr::memmem::Finder` for case-sensitive single-pattern.
- A single-pattern `aho_corasick::AhoCorasick` with `MatchKind::LeftmostFirst` and ASCII case-insensitive options for the case-insensitive ASCII path.

Either is significantly faster than the byte-at-a-time loop for long texts, especially on misses. On modern x86_64, `memmem` saturates a memory channel in many cases.

### 4.4 `WholeWordMatcher` allocates a full `Vec<char>` of the document (medium)

`WholeWordMatcher::new(text, true)` collects every char in the chunk text into a `Vec<char>` (`matchers.rs:434-454`) so it can index `chars[start - 1]` and `chars[end]`. For a 64 KB-char chunk that's ~256 KB of char storage each, recreated per chunk. The matcher is only consulted at match positions, of which there are usually very few.

**Fix**: walk forward/backward from the match position to find adjacent characters using `text[..byte_index].chars().next_back()` and `text[byte_index..].chars().next()`. No per-chunk Vec, O(1) amortised per match.

### 4.5 Result-vector cloning per partial snapshot (low-to-medium)

`SearchResultAccumulator::partial_snapshot` (`helpers.rs:81-89`) clones the full `matches` vec and `result_groups` every time a target finishes. The matches vec grows monotonically, so emitting `N` partials over `N` targets is O(N²). The 200-result UI cap limits the visible groups, but `matches` itself is unbounded and each entry holds three owned `String`s.

**Fix options**:
1. Send incremental deltas (only the new matches since last partial) and let the UI append.
2. Skip emission unless the match count has grown by some threshold (e.g. ≥ 64 new matches *and* ≥ 50 ms since last partial).
3. Wrap matches in `Arc<[SearchMatch]>` and only re-Arc when published.

### 4.6 `clear_search_highlights` walks every tab (low)

`runtime.rs:387-404` iterates `self.tabs_mut()` clearing highlights from every view in every tab. Highlights are only ever applied to the active tab (`apply_search_highlights` at `runtime.rs:343-358`). Each search refresh therefore touches every view's `clear_search_highlights_for_release`.

**Fix**: track which tab/view ids currently hold highlights and clear only those, or scope the loop to the active tab.

### 4.7 `result_groups.to_vec()` on every UI frame (low)

`SearchStripState::from_app` (`state.rs:65`) clones the full `result_groups` once per frame the search dialog is open, even when nothing has changed. With 200 entries holding three `String`s each (`buffer_label`, `tab_label`, `preview`), this is real allocation churn during idle frames.

**Fix**: hold an `Arc<[SearchResultGroup]>` in `SearchState` and clone the Arc instead of the contents, or maintain a generation counter the UI consults to decide whether to refresh.

### 4.8 Duplicate `INTRA_BUFFER_PARALLELISM_CAP` constant (cosmetic, but easy to drift)

Defined twice — `worker.rs:19` and `fragments.rs` (via `INTRA_BUFFER_PARALLELISM_MIN_CHUNKS`). Fold into one module's `pub(super) const` and reuse.

### 4.9 Target-parallelism gate at 4 (worth re-measuring)

`SEARCH_TARGET_PARALLELISM_MIN_TARGETS = 4` (`worker.rs:18`) means a "Current Tab" search across 2–3 large files won't parallelise across targets — though intra-buffer parallelism may still kick in. With the partial-result streaming benefit, single-threaded is reasonable for small target counts; this is worth revisiting once the bench in `benches/search_speed.rs` is rerun.

---

## 5. Best-Practice / Code-Quality Findings

### 5.1 `process_search_request_with_partials` has duplicated branches

The single-threaded and parallel branches in `worker.rs:117-241` repeat the `partial_emit` / `latest_generation` / "skip emission on last target" pattern. The two branches could share a common closure that drives a target stream, yielding an `Iterator<Item = TargetSearchOutcome>` that the partial-emission loop consumes uniformly.

### 5.2 `SearchState` exposes too much

`pub(crate)` fields cover all of the channels, atomics, and active-match cache (`search_state.rs:199-226`). Most callers only need read methods; turning the channel/atomic/`previous_active_match` into private fields would clarify the API surface and prevent ad-hoc mutation from runtime/replace modules. The methods on `impl ScratchpadApp` already manipulate state via accessors — internal access could mirror that.

### 5.3 `SearchStatus::Searching` and `SearchProgress::searching` overlap

Two boolean-ish indicators of "still working" (`search_state.rs:55-72`, `search_state.rs:127-134`). UI consumers must check both. Pick one canonical signal — likely `SearchStatus` — and derive `SearchProgress.searching`.

### 5.4 Two regex parses per query

`validate_search_query` calls `compile_supported_regex` (which calls `RegexBuilder::build`) and the runtime later calls `compile_supported_regex` again. `regex_max_match_chars` parses with `regex_syntax::parse` *separately*, so a single query that reaches the matcher gets parsed three times: validation build, bounds check, and matcher build. Fold into one parse + build pair, store the resulting `(Regex, max_match_chars)`.

### 5.5 `previous_active_match` vs `active_match_index` reset logic

`begin_request` snapshots `previous_active_match`, then `apply_search_result` consumes it only on non-partial results (`runtime.rs:99-111`). Across multiple partial snapshots the snapshot remains correct, but the dual "either index or stored match" representation is easy to get wrong. A `MatchIdentity` newtype or always-stored-match approach would simplify the reasoning.

### 5.6 `selection-only` scope without selection sets an Error status, but not a typed error

`runtime.rs:157-165` builds a free-form `String` error. The codebase already has `SearchError` as an enum for the matcher; consider a parallel typed enum for orchestration errors so UI/test code can match on variants instead of strings.

### 5.7 `matched_text: String` per `SearchMatch`

Used only by `validate_search_match_for_replace` for staleness. For huge result sets this is wasted memory. A short hash/length plus on-demand re-extraction at replace time would be cheaper, since replace is a much rarer operation than scan.

### 5.8 Tests / benches

There are integration benches (`benches/search_speed.rs`) and profiling binaries (`src/bin/profile_search_*.rs`), but no unit tests in the search modules themselves. The matcher fast paths in particular (ASCII vs Unicode, single-byte, whole-word boundary edge cases at start/end of text) are precisely the sort of code where small tests pay off. Suggested cases:

- ASCII / Unicode parity for the same query and text.
- Whole-word at index 0 and at `text.len()`.
- Multi-byte ASCII case-insensitive where first/last byte filter is hit by spurious matches.
- Bounded-regex rejection (e.g. `a*`, `(?:foo)+`).
- Generation-based cancellation: feed a large text and assert `should_continue → false` returns `None` quickly.

---

## 6. Utility & Functionality Findings

### 6.1 No regex back-references in replacement (significant)

`replace_char_ranges_with_undo` substitutes the replacement string verbatim (`replace.rs:42`, `replace.rs:140-156`). In regex mode users typically expect `$1`, `$2`, `${name}` to expand. The `regex` crate's `Captures::expand` provides this directly.

### 6.2 No "Replace and Skip" / "Find Next without replace"

Replace-current already auto-advances. There is no command to skip the current match without modifying it. `Down`/`Up` work as long as the find input has focus, but a dedicated "Skip" affordance in the replace pill is conventional.

### 6.3 No search history

The find input has no recall of prior queries (no `Up`/`Down` history on an empty query). Most editors persist a small ring; combined with the existing per-session search state this would be inexpensive.

### 6.4 200-match display cap with no "show more" affordance

`SEARCH_RESULT_LIMIT = 200` (`helpers.rs:10`) is a sane default, but the UI never tells the user there are more, beyond the `"X of Y matches"` text in the summary line (`results.rs:76-81`). The Next/Previous buttons still navigate beyond the visible 200, which is the right behaviour, but a "Show more" or a virtualised list past 200 would close the surprise gap.

### 6.5 Unicode case-insensitive uses `char::to_lowercase` only

`matches_unicode_case_insensitive` (`matchers.rs:399-404`) lowercases both sides. Full Unicode case folding (e.g. `unicode-case-mapping` or `caseless` crate) handles edge cases like German `ß ↔ SS` and Turkish dotted/dotless I correctly. For an editor aimed mainly at code, this is low priority but worth a TODO.

### 6.6 No multi-buffer atomic undo

Replace-all across multiple buffers issues per-buffer `replace_char_ranges_with_undo` (`replace.rs:289-313`). Each buffer's undo is independent, so a partial failure mid-plan leaves a mixed state, and undo requires N undos in N buffers. The product plan calls out atomic replace-all as a goal — this is the largest gap relative to the plan in `docs/search-replace-plan.md`.

### 6.7 Selection-only scope freezes on initial selection

When the user opens search with a selection, `SearchScope::SelectionOnly` is set and `active_search_selection_range()` is consulted (`search_state.rs:340-352`). If the user then changes selection without reopening, the search range follows because `collect_search_targets` reads the live selection — but after the first match navigation, the cursor moves *into* a single match, so the selection collapses, and subsequent edits/refreshes will hit the "selection-only without selection" error. This is plausibly intended but not obvious; a small UI breadcrumb ("Locked to selection [0..N]") would help.

### 6.8 Scope cycling has no keyboard shortcut

Scope chips are mouse/Tab targets only (`controls.rs:116-133`). `Alt+1..Alt+4` or `Ctrl+Tab` while the search dialog is focused would close the keyboard-first UX gap mentioned in the product plan.

### 6.9 No replace preview

The plan (`docs/search-replace-plan.md` §4) calls for a preview of replacements. The current implementation paints the replacement string inline in the editor via `set_search_replacement_preview` (`runtime.rs:355`), which is good — but the result list itself does not show the post-replace preview side-by-side. For multi-buffer replace-all a confirmation surface that lists `<file>: N matches → <preview>` would make the "press again to confirm" flow more trustworthy.

### 6.10 No progress indication on long searches

`SearchStatus::Searching` is binary. For a long `AllOpenTabs` search, a "scanned X of Y files" or even a spinner with file name would close a UX gap. The infrastructure is ready: `worker.rs` already streams partial results per target.

### 6.11 No "find in path" / disk search

Limited to open tabs. Possibly intentional (this is a scratchpad-style app, not an IDE), but worth flagging as an explicit non-goal in the plan if so.

### 6.12 No diff/dry-run mode for Replace All

The "press again to confirm" pattern is fine for small replacements but offers no preview for large ones. A diff hover or dry-run summary panel would lift trust on cross-buffer replaces.

### 6.13 No keyboard shortcut to cycle Replace All visibility

`Ctrl+H` opens search-with-replace, `Ctrl+F` opens search alone, but inside the dialog there's no shortcut to toggle the replace pill — only the heading button (`controls.rs:245-273`).

---

## 7. Prioritised Improvement List

| # | Category | Item | Rough effort |
|---|----------|------|--------------|
| 1 | Speed | Compile regex once per request, not per chunk | S |
| 2 | Functionality | Regex `$1`/`${name}` back-references in replacement | S |
| 3 | Speed | Replace naive ASCII scans with `memchr::memmem` (case-sensitive) and `aho-corasick` (case-insensitive) | M |
| 4 | Speed | Eliminate per-chunk `byte_to_char_map`/`Vec<char>` allocations (lazy adjacency walks, on-demand byte→char conversion) | M |
| 5 | Functionality | Atomic multi-buffer undo for Replace All | M |
| 6 | Best practice | Add unit tests for matcher fast paths and cancellation | S |
| 7 | Speed | Stream partial results as deltas (`Arc<[Match]>` or append-only) instead of cloning full vectors | M |
| 8 | Speed | Cache `result_groups` for the UI via `Arc<[…]>` instead of `to_vec()` per frame | S |
| 9 | Best practice | Single source of truth for "searching" state (drop redundant flag) | S |
| 10 | Best practice | Single source for `INTRA_BUFFER_PARALLELISM_CAP`; deduplicate worker dispatch branches | S |
| 11 | Functionality | Search history (per-session ring) | S |
| 12 | Functionality | Keyboard shortcuts to cycle scope and toggle replace pill | S |
| 13 | UX | "Show more" / virtualised result list past 200 entries | S |
| 14 | UX | Per-target progress on long searches (`scanned N of M`) | S |
| 15 | Speed | Scope `clear_search_highlights` to the active tab | S |
| 16 | Best practice | Tighten `SearchState` field visibility; type orchestration errors | S |
| 17 | Functionality | Drop `matched_text` from `SearchMatch`; re-extract at replace time | S |
| 18 | Functionality | Full Unicode case folding for case-insensitive search | M |
| 19 | UX | Diff/dry-run preview for cross-buffer Replace All | M |

S = under a day for a familiar engineer; M = multi-day.

---

## 8. Quick-Wins Worth Doing First

Three changes that together remove most of the avoidable allocation/CPU on the hot path and don't touch architecture:

1. **One regex compile per request** (item 1 above). Smallest diff, biggest predictable speed-up on any non-trivial regex search of a multi-chunk buffer.
2. **`memmem::Finder` for the ASCII case-sensitive path** (subset of item 3). Pulls only an existing transitive dep into scope; replaces a hot loop with a SIMD finder. Especially impactful on `AllOpenTabs` over many small files.
3. **Drop the per-chunk `WholeWordMatcher` Vec** (subset of item 4). Replace with adjacency lookups. Lifts the whole-word toggle from "noticeably slower" to "free".

These three are independent of each other and of any architectural changes, and they preserve the existing public API of `services::search`.
