# Seven-Promises Quick Wins — Performance Plan

Date: 2026-05-12
Source data: `target/analysis/{performance_review,slowspots,search_speed,capacity_report,resource_profiles,hotspots,flamegraphs}.json`
Source code reviewed (no docs consulted):
- `src/app/domain/buffer/{snapshot.rs,document.rs,piece_tree.rs,piece_tree/{support.rs,edit.rs}}`
- `src/app/services/search.rs`, `src/app/services/search/matchers.rs`
- `src/app/app_state/search_state/{worker.rs,worker/processing.rs,fragments.rs,search_state.rs}`
- `src/app/ui/editor_content/native_editor/{mod.rs,layout.rs}`
- `src/app/domain/{tab_manager.rs}`

This is a **quick-win** plan: small, low-risk changes that each move at least one of the seven promises measurably. Each item points at evidence and at the lines that need to change. None require new dependencies (other than `memchr`, an option called out where it helps).

---

## Headline numbers worth fixing

| Workload                                    | Observed         | Budget    | Gap        |
| ------------------------------------------- | ---------------- | --------- | ---------- |
| `document_snapshot_creation_latency/4 MB`   | **34.8 ms**      | 5 ms      | 7×         |
| `viewport_extraction_latency/4 MB`          | **34.1 ms**      | 16 ms     | 2.1×       |
| `scroll_stress_latency/4 MB`                | **45.8 ms**      | 32 ms     | 1.4×       |
| `search_current_app_state_completion/256`   | **1.83 s**       | 85 ms     | 21×        |
| `search_current_completion_file_size/128 K` | **103 ms**       | 45 ms     | 2.3×       |
| `file_size_ceiling` 128 MB                  | **501 ms**       | 160 ms    | 3.1×       |
| `paste_size_ceiling` 64 MB                  | **170 ms**       | 150 ms    | 1.1×       |
| `paste_size_ceiling` 512 MB                 | **1.23 s**       | 150 ms    | 8×         |
| `tab_count_ceiling` 512                     | **311 ms**       | 140 ms    | 2.2×       |
| `tab_count_ceiling` 10 000                  | **2.56 s**       | 140 ms    | 18×        |
| `many_file_count_ceiling` 10 000            | **830 ms**       | 180 ms    | 4.6×       |
| Search throughput (current-tab, file-size)  | **≈ 10 MB/s**    | —         | —          |

Search dominates the slowspots list (top 13 of the top 20 entries). After search, the next clusters are scroll/snapshot/viewport at 4 MB and tab/many-file restore.

---

## Quick wins by promise

### 1. Large Files — load, scroll, snapshot, viewport

#### 1.1 Stop scanning the same bytes 3–4 times when ingesting text  ★ biggest single load win
- **Where:** `Piece::from_slice` at [src/app/domain/buffer/piece_tree.rs:85–94](src/app/domain/buffer/piece_tree.rs:85). Currently calls `text.chars().count()`, `count_newlines(text)`, and `text.is_ascii()` — three full passes per chunk. `PieceTreeLite::insert_with_source` at [piece_tree/edit.rs:19](src/app/domain/buffer/piece_tree/edit.rs:19) adds a fourth `text.chars().count()` on the whole inserted slice, and `TextDocument::insert_raw_text_with_source` at [buffer/document.rs:361](src/app/domain/buffer/document.rs:361) adds a fifth on the same bytes.
- **Change:** single fused UTF-8-safe pass that returns `(byte_len, char_len, newline_count, is_ascii_subset)`. ASCII-subset chunks can still short-circuit `char_len = byte_len` and use `memchr::memchr_iter(b'\n', bytes)` for newline counting, but benchmark claims should be reported against mixed UTF-8 workloads first.
- **Impact:**
  - 1 GB file load currently 6.3 s with the file-load benchmark. Re-run this against the mixed UTF-8 capacity workloads; treat ASCII-subset throughput as a secondary diagnostic for the fast path, not the headline number.
  - Directly helps `file_size_ceiling` (raises the 128 MB / 1 GB ceilings) and `paste_size_ceiling` (raises 64–512 MB).
- **Risk:** trivial; behavior unchanged.

#### 1.2 Avoid the per-frame `String` allocation for the viewport galley
- **Where:** `viewport_text_slice` at [native_editor/layout.rs:192](src/app/ui/editor_content/native_editor/layout.rs:192) calls `tree.extract_range(start_char..end_char)` which always allocates. For any single-span UTF-8 slice, including unedited buffers, `borrow_range` returns `Some(&str)` in O(1).
- **Change:** call `borrow_range` first; only fall back to `extract_range` if the slice straddles pieces. The downstream `display_text_slice` / `preview_text_slice` already accept `&str`. Plumb a `Cow<'_, str>` through `ViewportTextSlice` instead of `String`.
- **Impact:** `viewport_extraction_latency/4194304` is 34 ms and the snapshot+viewport calls happen ≥1× per frame and ≥2× whenever input changes selection (see `should_rebuild_galley_after_input` at [native_editor/mod.rs:367](src/app/ui/editor_content/native_editor/mod.rs:367)). Removing the 4 MB allocation on the steady-state path should cut viewport extraction toward its 16 ms budget.
- **Risk:** low — `borrow_range` already returns `Option<&str>`; we just stop forcing an owned copy.

#### 1.3 Keep line lookup UTF-8-first, with an ASCII-subset byte fast path
- **Where:** [piece_tree.rs:355–392](src/app/domain/buffer/piece_tree.rs:355). The hot loop is `for ch in self.piece_text(piece).chars()` — O(piece bytes) regardless of how close `safe_offset` is.
- **Change:** when `piece.is_ascii` is set, switch to `memchr::memchr_iter(b'\n', &bytes[..local_end])` and count newlines. For non-ASCII fall back to today's char loop, but cap by `offset_in_piece` (currently you still iterate the full piece if you reach the target). The same applies to `scan_piece_for_line_lookup` at [piece_tree/support.rs:283](src/app/domain/buffer/piece_tree/support.rs:283).
- **Impact:** called many times per scroll-stress frame (cursor → line, scroll target → line, snapshot extraction). The scroll_stress flamegraph (`scroll_stress_profile.svg`, 486 KB) is the right place to verify. Snapshot creation also calls `line_count_from_piece_tree` repeatedly.
- **Risk:** low; the ASCII-subset branch is straight byte counting, but the acceptance tests should include mixed UTF-8 lines so this does not become an ASCII-only optimization story.

#### 1.4 Stop cloning `Vec<char>` just to truncate a preview
- **Where:** `compact_preview` at [piece_tree/support.rs:176–188](src/app/domain/buffer/piece_tree/support.rs:176). Collects every char into a Vec to measure length and slice.
- **Change:** use `chars().count()` for length check then `text.char_indices().nth(PREVIEW_MAX_CHARS)` for the cut byte index — no allocations except the final `String`. Or scan once and stop at the limit (also fine for long lines).
- **Impact:** called inside `previews_for_matches` for every match preview. With thousands of search hits in a large file this adds up; freed allocations also help the paste/search peak working-set numbers (`peak_working_set_bytes` is currently > 2 GB).
- **Risk:** trivial.

#### 1.5 Drop the unused `chars().count()` return value in `insert_raw_text*`
- **Where:** [buffer/document.rs:354–362](src/app/domain/buffer/document.rs:354). Returns the inserted char count, but inspection of callers (`insert_direct`, `insert_direct_with_source`, `replace_char_ranges_with_source`) shows the result is discarded. The piece tree already computes a precise `char_len` inside `Piece::from_slice`.
- **Change:** change the helper to `-> ()`, drop the `text.chars().count()` line. Combine with §1.1 so the piece tree exposes the inserted char total (it already sums it up internally for metrics) if a caller ever needs it.
- **Impact:** removes a full O(n) scan per paste/edit. Direct win on `paste_size_ceiling` 64 MB → 512 MB.
- **Risk:** trivial; pure dead-load removal.

---

### 2. Search — first match speed and full completion

Search is the biggest single bucket of slowspots (13 of the top 20). Two structural problems dominate.

#### 2.1 Stop materializing 64 KB windows into owned Strings for every chunk
- **Where:** `search_fragmented_plain_text` and `search_fragmented_bounded_regex` in [search_state/fragments.rs:84–108, 140–164](src/app/app_state/search_state/fragments.rs:84) each call `snapshot.search_text_cow(Some(chunk.window_range.clone()))`. `search_text_cow` falls back to `borrow_or_flatten_range` which allocates `String::with_capacity(...)` whenever the chunk spans multiple pieces — which is *almost always* in an edited buffer with `MAX_LEAF_BYTES = 256 KB` and `SEARCH_FRAGMENT_CHUNK_CHARS = 64 KB`.
- **Change (small):** raise `SEARCH_FRAGMENT_CHUNK_CHARS` to align with `MAX_LEAF_BYTES` (256 KB) so each window is much more likely to fit in a single span. The chunk overlap is already bounded by `query_chars + 1`, so the increase is cheap.
- **Change (medium):** when a chunk does straddle pieces, run the matcher span-by-span across the two/three pieces and stitch results at the seam (we only need an overlap of `max_match_chars`). For plain text this is straightforward: search each span then run the matcher only over the seam region. Avoids the per-chunk allocation entirely.
- **Impact:** `search_current_app_state_completion_aggregate_size/256` is **1.83 s** (budget 85 ms). The flamegraph `search_current_app_state_profile.svg` (278 KB) is the place to verify. Throughput today is ≈ 10 MB/s — most of that disappears into the per-chunk `String` allocations + copies. Even option-1 alone should roughly halve the time on multi-MB current-tab searches.
- **Risk:** medium. Seam stitching needs careful tests around match-spans-overlap; widening the chunk is essentially free.

#### 2.2 UTF-8 regex match conversion is O(n) per match on non-ASCII text
- **Where:** `byte_to_char_index` at [services/search/matchers.rs:260–262](src/app/services/search/matchers.rs:260) is called by `regex_match_range` for *every* regex match in non-ASCII text and does `text[..byte_index].chars().count()` — i.e. the whole prefix every time. Pathological with many matches in big buffers.
- **Change:** walk byte and char indices forward together in a single pass over the match iterator: keep a running `(byte_pos, char_pos)` cursor that advances using `text[byte_pos..next_match_byte].chars().count()` since the last match. Total cost becomes O(text bytes) instead of O(matches × text bytes).
- **Impact:** non-ASCII regex search currently degrades quadratically with match count. This should move the primary mixed UTF-8 search benchmarks and directly supports the user-promise "fast first response" on real UTF-8 text.
- **Risk:** low.

#### 2.3 Plain-text non-ASCII case-sensitive search re-scans `chars().count()` per call
- **Where:** [matchers.rs:145](src/app/services/search/matchers.rs:145) computes `query.chars().count()` inside the function (cheap on the query) but also iterates `text.char_indices().map(|(byte_index, _)| byte_index).enumerate()` — the `.enumerate()` is correct, but the loop checks `text.is_char_boundary(end_byte)` and compares slice equality on every step, which is fine; however the `find_matches_unicode_case_insensitive_impl` at [matchers.rs:210–247](src/app/services/search/matchers.rs:210) builds a full `char_to_byte_map` (Vec<usize> the size of the text) before any matching begins.
- **Change:** stream matches without materializing `char_to_byte_map`. Each candidate position can be derived from a forward char-indices iterator that advances by 1 char at a time, plus a sliding window of length `query_char_len`. Saves a Vec allocation proportional to text length on every non-ASCII case-insensitive search.
- **Impact:** big working-set win for non-ASCII searches over multi-MB files (memory is the limiting resource per `search_file_size_ceiling`). Helps `peak_working_set_bytes` climbing above 2 GB during search capacity sweeps.
- **Risk:** medium — the new code path needs unit coverage equivalent to the existing `find_matches_unicode_case_insensitive_impl` tests.

#### 2.4 Hoist `SearchProgram::compile` and `query.chars().count()` out of per-target work
- **Where:** the worker compiles `SearchProgram` once already (good), but `search_fragmented_plain_text` at [fragments.rs:70](src/app/app_state/search_state/fragments.rs:70) recomputes `query.chars().count()` for each target and each chunk batch. Inside `plain_text_matches` the ASCII branch re-computes `query_lower` for every call (`find_matches_ascii_case_insensitive_impl` at [matchers.rs:197](src/app/services/search/matchers.rs:197)).
- **Change:** cache `query_chars`, `query_lower`, and `WholeWordMatcher` once in `SearchProgram` (or a precomputed sibling struct passed alongside it). Pass references through the per-target / per-chunk paths.
- **Impact:** small per call, but on `search_current_completion_aggregate_size/256` we have 256 targets × ≥1 chunk each — so a few µs per call adds up to noticeable change. Cleaner than the structural rewrite of §2.1 and orthogonal to it.
- **Risk:** low.

#### 2.5 Skip `chunks_for_range` when the whole range borrows as one slice
- **Where:** `search_target_ranges` at [fragments.rs:32–44](src/app/app_state/search_state/fragments.rs:32) already takes the fast path when `borrow_range` succeeds. Good — but the fragmented path falls through to `chunks_for_range` even when the snapshot text is one contiguous span. After §1.1 / §1.5 reduce piece splitting, the borrowed fast-path should hit more often. Add a metric so we can confirm this in `capacity_metrics`.
- **Change:** add a counter `record_search_borrowed_range_hit()` next to the existing `record_search_chunks` so we can verify the fast-path rate climbs.
- **Impact:** observability change that unlocks measuring §1.1's effect on search.
- **Risk:** none.

---

### 3. Many Tabs — open, switch, reorder, manipulate 10 000+

#### 3.1 Make `rebuild_buffer_tab_index` incremental
- **Where:** [tab_manager.rs:139–146](src/app/domain/tab_manager.rs:139). Every `append_tab`, `insert_tab`, `close_tab_internal`, `reorder_tab`, `set_tabs` clears and rebuilds the whole `HashMap<BufferId, usize>`. Walking 10 000 tabs each time turns N tab-opens into Θ(N²) ≈ 50 M ops at N = 10 000.
- **Change:**
  - `append_tab`: just insert mappings for the new tab's buffers.
  - `insert_tab`: insert mappings for the new tab, and for tabs at indices ≥ insertion point, bump the stored index by 1.
  - `close_tab_internal`: remove the closed tab's mappings, decrement indices for later tabs.
  - `reorder_tab`: similar — only the moved range needs updating.
  - `set_tabs`: keep the full rebuild (single batch insertion).
  - Even simpler: keep the rebuild only in `set_tabs`, and switch the incremental ops to walk only the modified range. For 10 000 tabs the inner-loop cost goes from N to (1, or N/2 on average for `insert/close`).
- **Impact:** primary lever for `tab_count_scale/500` (33 ms, watch), `tab_count_ceiling` (512 tabs = 311 ms, 10 k = 2.56 s). Should also lift `many_file_count_ceiling` because session restore opens many tabs sequentially.
- **Risk:** low if behaviorally identical; cover with the existing `buffer_tab_index_tracks_tab_mutations` test plus a benchmark.

#### 3.2 Tab strip width estimate is fine — verify it doesn't iterate
- **Where:** `estimated_tab_strip_width` at [tab_manager.rs:63–69](src/app/domain/tab_manager.rs:63) is already O(1). No change. (Listed for completeness so future readers don't try to "optimize" it.)

---

### 4. Many Files — open and restore 10 000+ files

#### 4.1 Session restore: avoid re-rebuilding the buffer-tab index
- **Where:** session restore calls `set_tabs` once (good) but if it actually opens via `append_tab` in a loop, every insertion triggers a full rebuild. Confirm during implementation of §3.1. Path to check: [src/app/services/session_store/restore.rs](src/app/services/session_store/restore.rs:1) (584 lines). If it uses `append_tab` or `insert_tab` per persisted tab, §3.1 alone fixes this.
- **Impact:** turns the 10 000-file restore from 830 ms toward the 180 ms budget.

#### 4.2 Defer per-buffer transient state for inactive tabs
- **Where:** `evict_inactive_tab_state` already exists at [tab_manager.rs:54–61](src/app/domain/tab_manager.rs:54) — confirm it runs after a bulk restore so 9 999 inactive tabs don't keep editor / layout cache state in memory. If not, call it once at the end of restore.
- **Impact:** working-set growth in the `many_file_count_ceiling` sweep (currently 1.3 MB delta for 10 000 files — fine; but verify it's not 10× larger when those tabs were ever activated).

---

### 5. Many Views — 1 000+ tiles on the same buffer

`view_count_ceiling` is already passing (1 000 views in 12 ms). One thing to keep it that way:

#### 5.1 Stop allocating a `String` in the layout cache key per frame per view
- **Where:** `layout_cache_key` at [native_editor/layout.rs:132–148](src/app/ui/editor_content/native_editor/layout.rs:132) builds `font_family: format!("{:?}", input.options.editor_font_id.family)` on every cache lookup. With 1 000 views and dual rebuilds per frame, that's ≥ 2 000 String allocations per frame solely to hash a font family.
- **Change:** key by the `egui::FontFamily` enum directly (it's `Clone + Hash + Eq`) or by a `u32` discriminant.
- **Impact:** removes 1 000+ small allocations per frame at the 1 000-view scale; helps `view_navigation_profile` and matters most under egui's per-frame budget at sub-100 ms visible response.
- **Risk:** trivial — change `LayoutCacheKey` to hold the enum, no behavioral change.

#### 5.2 Skip the second `build_editor_galley` when nothing changed
- **Where:** `should_rebuild_galley_after_input` at [native_editor/mod.rs:367–377](src/app/ui/editor_content/native_editor/mod.rs:367) decides to rebuild the galley after input. It rebuilds on cursor *movement* (no edit, no selection change). For pure horizontal/vertical cursor motion within the same line, the galley contents are identical — we only need to re-paint the cursor position.
- **Change:** distinguish "rebuild because text/selection/highlight changed" from "rebuild because cursor moved" and reuse the existing galley in the second case (just update `galley_pos` and the cursor reveal hint). Reduces viewport extraction calls per frame from 2 → 1 in the cursor-movement case.
- **Impact:** scroll_stress at 4 MB is 45 ms over a 32 ms budget. Even small reductions here add headroom.
- **Risk:** low; tests `cursor_only_movement_rebuilds_galley_for_reveal` already capture the intent — relax that one rather than the others.

---

### 6. Large Text Mutation — paste, cut, undo, redo, metadata refresh

#### 6.1 Replace `count_newlines` with `memchr`
- **Where:** [piece_tree/support.rs:148–150](src/app/domain/buffer/piece_tree/support.rs:148) does `text.bytes().filter(|byte| *byte == b'\n').count()`. The `memchr` crate's `memchr::memchr_iter(b'\n', bytes).count()` is consistently 3–8× faster on long inputs because it uses SSE/AVX scans.
- **Change:** pull in `memchr` (already widely used and transitively present via `regex`/`aho-corasick`; confirm it's exposed). Replace the implementation in-place.
- **Impact:** every paste/edit/file-load goes through here. Compounds with §1.1.
- **Risk:** trivial.

#### 6.2 Combine §1.1 + §1.5 to make 64 MB paste land under 150 ms
- See §1.1 and §1.5. Both target the paste path.

#### 6.3 Pre-size `replace_leaf_span` allocations
- **Where:** `pack_pieces_into_leaves` at [piece_tree/support.rs:100–124](src/app/domain/buffer/piece_tree/support.rs:100) builds `leaves: Vec::new()` and `pack_leaves_into_nodes` at [support.rs:52](src/app/domain/buffer/piece_tree/support.rs:52) clones via `leaves[index..end].to_vec()`. For a 512 MB paste we re-allocate many times.
- **Change:** preallocate `Vec::with_capacity(pieces.len() / MAX_LEAF_PIECES + 1)` for leaves and use `Vec::drain(..end)` instead of `.to_vec()` to move (not clone) leaves.
- **Impact:** small but proportional to paste size; helps the `paste_size_ceiling` 256–512 MB rows.
- **Risk:** trivial.

---

### 7. Session Persistence Restore — large workspaces, no startup stalls

#### 7.1 Bulk-build TabManager from a vector instead of one tab at a time
- See §3.1. Restore is where `rebuild_buffer_tab_index` blows up because the existing call site is the right one (`set_tabs`), as long as nothing else in the restore path takes the `append_tab` route. Audit `services/session_store/restore.rs` to confirm — if it currently appends, switch to building a `Vec<WorkspaceTab>` and a single `set_tabs`.

#### 7.2 Skip layout cache warm-up during startup
- **Where:** `build_editor_galley` runs `warm_nearby_layout_slices` (see [layout.rs:213–273](src/app/ui/editor_content/native_editor/layout.rs:213)) the first time the cache contains a hit. During startup, no view has rendered yet so the guard `cache_was_warm` should already prevent warming on the first frame; verify this is still true when restoring 1 000+ views. If not, add an explicit "first N frames after restore" suppression.
- **Impact:** keeps the restore frame from generating extra ahead-of-time galleys for views the user may never look at. Helps `session_persistence_restore` smoothness even if total time looks fine.
- **Risk:** trivial check.

---

## Suggested ordering

This ordering maximizes signal-per-hour: each item is independently shippable and verifiable against an existing benchmark in `target/analysis`.

1. **§1.1 fused single-pass `Piece::from_slice` (+ §1.5 dead `chars().count()`)** — touches one struct, instantly observable in `file_load`, `paste_size_ceiling`, `document_snapshot_creation_latency`. **Biggest bang.**
2. **§6.1 `memchr::memchr_iter` for newlines** — one-line change, compounding with §1.1.
3. **§3.1 incremental `rebuild_buffer_tab_index`** — fixes the worst `tab_count_ceiling` cliff and likely §4.1 in the same change.
4. **§5.1 stop allocating a String for the font-family in the layout-cache key** — single-file, removes a per-view per-frame allocation.
5. **§1.2 Cow viewport slice via `borrow_range`** — feeds 1.3 / scroll_stress headroom.
6. **§1.3 UTF-8 line lookup with an ASCII-subset byte fast path** — bigger change, but the scroll_stress profile points right at it.
7. **§2.1 widen `SEARCH_FRAGMENT_CHUNK_CHARS` + seam-stitched span search** — the structural change for the search throughput ceiling.
8. **§2.2 / §2.3 / §2.4 search clean-ups** — quick wins after the structural change.
9. **§5.2 skip the duplicate galley build on cursor-only motion** — last because it's the most subtle behavioural change.

## Out of scope for "quick wins"

These showed up in the data but are larger projects, not quick wins, so they're tracked here only as pointers:

- File-backed (memory-mapped) open: `file_size_ceiling`'s `unusable_latency` mode is allocation-bound (peak 1.08 GB working set on a 128 MB file). A real fix is mmap + lazy ingest, not in this plan.
- Search target dispatch with hundreds of files at once: `search_dispatch_aggregate_size/128` is 48 ms (budget 12 ms). The dispatch overhead is largely snapshot cloning — already cheap (`Arc<PieceTreeLite>`), but `from_shared` may deep-clone when anchors are live (see [snapshot.rs:25](src/app/domain/buffer/snapshot.rs:25)). Investigate why anchors are live mid-search; if a search snapshot doesn't need anchors, take the path that strips them once at the source rather than per snapshot.
- Tab strip rendering with 10 000 tabs is its own UX problem (a horizontal strip of 10 000 buttons), not just a CPU one.

---

## How to verify each fix

Each item maps to one or more existing benchmarks/probes — re-run after each change:

- `cargo bench --bench file_load --bench scroll_stress_latency --bench document_snapshot_creation_latency --bench viewport_extraction_latency`
- `cargo bench --bench search_*`
- `target/release/capacity_probe.exe` (writes `capacity_report.json`)
- Dashboard: `scripts/open-overview.ps1` for visual diffs in `performance_review.json`.

Track each change against the "Headline numbers" table at the top of this doc.
