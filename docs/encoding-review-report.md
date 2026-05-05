# Encoding Review Report

This review covers the current encoding path in Scratchpad, with emphasis on open, save, metadata tracking, and user-facing controls.

## Current State

The project has a solid base for encoding-aware editing:

- `src/app/services/file_service.rs` detects BOMs first, falls back to `chardetng`, rejects likely binary files, and decodes through `encoding_rs`.
- `src/app/domain/buffer/analysis.rs` stores encoding name, BOM state, line-ending metadata, encoding source, ASCII-subset state, and decoding-warning state in `TextFormatMetadata`.
- `src/app/services/file_service/write.rs` writes UTF-8, UTF-16LE, UTF-16BE, and legacy encodings from `DocumentSnapshot` spans without flattening the whole document first.
- `src/app/ui/status_bar.rs` exposes encoding and line-ending state, and `src/app/ui/dialogs/encoding.rs` supports reopen/save with a selected encoding.

The main remaining work is not "add encoding support" from scratch. The project already has that. The next improvements should focus on round-trip correctness, ambiguity handling, and tests.

## Documentation Implementation Status

This report is the working checklist for encoding reliability work. Keep this section current as the implementation changes.

| Area | Current status | Doc owner note |
| --- | --- | --- |
| Encoding-aware open | Implemented | BOM detection, heuristic detection, binary rejection, and explicit reopen-with-encoding are documented here and in the user manual. |
| Encoding-aware save | Partially implemented | Saves preserve encoding and supported BOM state. Snapshot and string save paths now serialize non-mixed line endings through the stored preferred line-ending policy; focused tests cover UTF-8, UTF-16LE, mixed preservation, split-span CRLF, and Windows-1252 failures. |
| User-facing controls | Implemented | Status bar encoding, line-ending display, and the Encoding dialog are documented in the user manual. |
| Decode-loss visibility | Partial | Decode warnings exist in metadata, but this report remains the source of truth for the stronger warning UX described in Finding 4. |
| Detection confidence | Not implemented | Finding 2 defines the target model. |
| Full encoding picker | Not implemented | Finding 3 defines the target picker behavior. |
| Focused round-trip tests | Partial | Add/expand tests before changing serialization semantics. |

Related docs:

- [User manual](user-manual.md) for the current user-facing behavior.
- [Comprehensive Text File Compatibility](multi-encoding-support.md) for the broader target model.
- [Plain Text Artifact Handling Plan](plain-text-artifact-handling-plan.md) for control-character and terminal-artifact behavior.

## Findings

### 1. Save does not apply stored line-ending policy

`TextDocument` has a preferred line-ending setting and editor insert paths normalize newly inserted line breaks, but `write_snapshot_to_writer` writes each piece-tree span exactly as stored. That means save preserves whatever is currently in the piece tree, not necessarily the `TextFormatMetadata` line-ending policy.

This is risky for files opened with a consistent style that later receive pasted or programmatic text containing different newline forms. The metadata can say `CRLF` while the output still contains mixed newline bytes.

Status: partially implemented. Snapshot and string save paths now route through a newline serialization adapter. Keep this finding open until broader round-trip fixtures and any explicit user-facing newline conversion commands are in place.

The adapter should continue to:

- preserve exact spans for `LineEndingStyle::Mixed`
- preserve exact spans when the user explicitly wants raw mixed output
- otherwise convert logical editor newlines to `format.preferred_line_ending_style()` while streaming to the encoder
- add tests for UTF-8, UTF-16LE, and Windows-1252 with `LF`, `CRLF`, `CR`, and mixed endings

### 2. Heuristic detection has no confidence model

The open path records `EncodingSource::Heuristic`, but the app does not retain confidence, detector rationale, or whether the decoded output looked suspicious beyond `has_decoding_warnings`.

This matters for ambiguous single-byte files. A Windows-1252 file, ISO-family file, and short ASCII-heavy file can all decode without replacement, but choosing the wrong encoding can still corrupt user intent on save.

Improve this by adding an `EncodingDetectionQuality` field or similar:

- `Bom`
- `Explicit`
- `AsciiOnly`
- `HeuristicLikely`
- `HeuristicAmbiguous`
- `DecodeHadErrors`

Use it in the status bar and encoding dialog so the app can suggest "reopen with encoding" only when ambiguity is real.

### 3. Manual encoding choices are limited to a fixed short list

`COMMON_TEXT_ENCODINGS` covers important cases, but the selection is fixed and omits several encodings that `encoding_rs` can decode. That is acceptable for a first UI, but it limits recovery when detection guesses wrong.

Improve this by separating:

- common encodings shown first
- full supported list behind a secondary selector or search field
- recent encodings used in this workspace/session

Also store display labels separately from canonical names. Internally, keep canonical `encoding_rs` names only.

### 4. Decoding warnings are recorded but not strongly actionable

`read_document_with_encoding` sets `has_decoding_warnings` when the decoder reports errors, and `TextFormatMetadata::format_warning_text` can describe substitutions. The current UI path does not make this as prominent as save-time non-compliance warnings.

Improve this by making decode-loss visible as a higher-severity status warning:

- show a red/yellow status indicator when open introduced substitutions
- offer immediate reopen-with-encoding
- avoid clearing that warning just because the buffer later saves successfully

### 5. Binary detection is minimal

The binary check rejects NUL bytes in the prefix and decoded text. That is a useful guard, but it will still admit many binary-like files with dense control bytes, and it can reject legitimate text-like data with embedded NULs.

Improve this by making binary detection explicit and tunable:

- detect high C0-control density in non-BOM inputs
- allow UTF-16 NUL byte patterns only after BOM/UTF-16 detection
- report "likely binary" separately from "unsupported encoding"
- add fixtures for UTF-16 without BOM, binary prefixes, and logs with legitimate control characters

### 6. Encoding correctness lacks focused test coverage

The repository has tests around document editing and other app behavior, but there are no obvious focused tests for `FileService` open/save round trips, BOM handling, legacy encoding failures, or newline serialization.

Add a dedicated test module for file encoding behavior. Minimum cases:

- UTF-8 with and without BOM round-trips byte-for-byte when unchanged
- UTF-16LE and UTF-16BE with BOM round-trip after edit
- Windows-1252 saves fail when text contains unrepresentable characters
- explicit "save with UTF-8" clears unsupported BOM state for legacy encodings
- heuristic-opened ASCII-only files remain marked as ASCII subset
- mixed line endings are detected and preserved unless normalized by an explicit command
- CR-only files are treated as structural line endings, not carriage-return artifacts

## Recommended Execution Order

1. Add encoding and newline round-trip tests around `FileService`.
2. Add a streaming newline serialization adapter and make save use it.
3. Promote decoding warnings into visible, actionable UI status.
4. Add a detection-quality field to `TextFormatMetadata`.
5. Expand the encoding picker with searchable full-list support and recent choices.
6. Tighten binary detection after tests make the intended behavior explicit.

## Summary

Scratchpad's encoding architecture is already pointed in the right direction: decoding is centralized, format metadata exists, save uses format metadata, and the UI exposes encoding actions. The highest-value improvement is to make the format contract testable and exact. Start with round-trip tests, then make save serialization honor the stored line-ending policy, and only then expand detection and UI affordances.
