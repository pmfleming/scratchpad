# Unmeasured Capacity Issues

This file tracks capacity or performance issues found during code review that are not directly covered by the current probes. Add entries here when a change targets a real promise-path cost but the built-in measurements cannot isolate it yet.

## Measurement Coverage Added

Date: 2026-05-12

The six gaps from `docs/performance-lessons-report-2026-05-12.md` now have first-class resource-profile scenarios and dashboard coverage:

- `large_utf8_load_peak_memory` measures allocator/process high-water behavior during large UTF-8 load.
- `edited_buffer_search_preview_rendering` isolates preview rendering on edited, multi-piece buffers with many matches.
- `provenance_retained_memory` reports retained provenance entries and allocation pressure after long edit sessions with history-budget eviction.
- `anchor_heavy_view_editing` measures edit and resolution cost with scroll, cursor, selection, and search anchors.
- `fragmented_long_session_mutation` measures paste/cut/undo/redo on fragmented long-session buffers.
- `session_persist_cost`, `session_restore_cost`, and `startup_visible_restore_cost` now expose session-stage breakdown labels for snapshot capture, serialization, file I/O, manifest read/parse, and restore reconstruction.

The overview consumes `resource_profiles.summary.measurement_gaps_closed` and shows this as a performance headline metric.

## Search: Edited-Buffer Match Preview Rendering

Date: 2026-05-12

Former probe gap: the built-in search probes measured matching throughput over large text and many targets, but did not specifically measure rendering previews for matches in edited, multi-piece buffers.

Observed issue: `PieceTreeLite::previews_for_matches` used the contiguous text fast path for unedited buffers, but fell back to per-match `preview_for_match` on edited buffers. That fallback repeatedly called line lookup and bounded extraction for every match, turning preview rendering into `O(matches * leaves)` on fragmented buffers.

Measurement coverage: `edited_buffer_search_preview_rendering` builds edited fragmented documents, generates sorted match ranges, and times `previews_for_matches` separately from the search matcher.

## Large Files: Peak Memory During UTF-8 Load

Date: 2026-05-12

Former probe gap: the large-file load probes reported elapsed time and existing resource counters, but did not directly capture peak resident memory while a file was being read, decoded, inspected, and packed into piece-tree leaves.

Observed issue: the UTF-8 load path reads the whole file into a byte vector, converts that into one `String`, and then builds piece metadata from the completed string. This likely creates a peak-memory spike while both the byte vector and string are alive, but we do not yet have a probe that proves the size of that spike.

Implementation note: keep the existing full-buffer path unless a future peak-memory measurement proves a better option. A direct-to-`String` path avoided the separate byte vector but measured much slower on 2GB opens. A chunk-fed piece-tree builder was slower still because it gave up the established parallel piece build. Do not retry either as an unmeasured cleanup.

Measurement coverage: `large_utf8_load_peak_memory` records allocator high-water data and process memory samples while opening large multi-byte UTF-8 files.

## Large Text Mutation: Provenance Store Growth

Date: 2026-05-12

Former probe gap: the mutation and resource probes did not directly report provenance-store entry count or retained bytes after very long edit sessions.

Observed issue: every non-load add-buffer span records provenance metadata. Heavy typing, paste, and replace sessions could retain provenance entries that no live piece or history entry referenced anymore, so memory usage grew with session length rather than with useful undo/history state.

Implementation note: the store is now bounded and add-buffer compaction rewrites provenance for retained visible/history spans while dropping cold entries. `provenance_retained_memory` asserts the entry count and allocator impact after long edit runs plus history-budget eviction.
