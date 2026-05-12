# Unmeasured Capacity Issues

This file tracks capacity or performance issues found during code review that are not directly covered by the current probes. Add entries here when a change targets a real promise-path cost but the built-in measurements cannot isolate it yet.

## Search: Edited-Buffer Match Preview Rendering

Date: 2026-05-12

Current probe gap: the built-in search probes measure matching throughput over large text and many targets, but they do not specifically measure rendering previews for matches in edited, multi-piece buffers.

Observed issue: `PieceTreeLite::previews_for_matches` used the contiguous text fast path for unedited buffers, but fell back to per-match `preview_for_match` on edited buffers. That fallback repeatedly called line lookup and bounded extraction for every match, turning preview rendering into `O(matches * leaves)` on fragmented buffers.

Measurement needed later: add a search-preview capacity scenario that builds a large edited buffer with many pieces, generates hundreds or thousands of sorted match ranges, and times `previews_for_matches` separately from the search matcher.

## Large Files: Peak Memory During UTF-8 Load

Date: 2026-05-12

Current probe gap: the large-file load probes report elapsed time and existing resource counters, but they do not directly capture peak resident memory while a file is being read, decoded, inspected, and packed into piece-tree leaves.

Observed issue: the UTF-8 load path reads the whole file into a byte vector, converts that into one `String`, and then builds piece metadata from the completed string. This likely creates a peak-memory spike while both the byte vector and string are alive, but we do not yet have a probe that proves the size of that spike.

Implementation note: keep the existing full-buffer path unless a future peak-memory measurement proves a better option. A direct-to-`String` path avoided the separate byte vector but measured much slower on 2GB opens. A chunk-fed piece-tree builder was slower still because it gave up the established parallel piece build. Do not retry either as an unmeasured cleanup.

Measurement needed later: add a file-backed load scenario that records process peak RSS or allocator high-water mark while opening large UTF-8 files, including ASCII and multi-byte UTF-8 cases.

## Large Text Mutation: Provenance Store Growth

Date: 2026-05-12

Current probe gap: the mutation and resource probes do not directly report provenance-store entry count or retained bytes after very long edit sessions.

Observed issue: every non-load add-buffer span records provenance metadata. Heavy typing, paste, and replace sessions could retain provenance entries that no live piece or history entry referenced anymore, so memory usage grew with session length rather than with useful undo/history state.

Implementation note: the store is now bounded and add-buffer compaction rewrites provenance for retained visible/history spans while dropping cold entries. A future probe should assert the entry count and allocator impact after hundreds of thousands of small edits plus history-budget eviction.
