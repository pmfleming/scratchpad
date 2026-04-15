# Plan: Comprehensive Text File Compatibility

This plan expands the existing encoding work into a full text-file compatibility model for Scratchpad.

The goal is not only to open more encodings, but to correctly recognize how text varies across platforms and tooling:

- UTF variants such as UTF-8 and UTF-16
- legacy single-byte encodings often described by users as "ANSI"
- ASCII and UTF-8 subset files
- newline conventions from Windows, Unix, and older Mac tooling
- files that mix valid text structure with terminal artifacts or control characters

This document should be read alongside [plain-text-artifact-handling-plan.md](plain-text-artifact-handling-plan.md). That document governs how true control-sequence artifacts are preserved and inspected. This document governs how Scratchpad should classify, display, preserve, and save text-file format variations overall.

## Goal

Make Scratchpad reliable for real-world text files from different operating systems, editors, terminals, and older tools without silently corrupting their format.

Target behavior:

- open common UTF, ASCII-subset, and legacy encoded text files predictably
- recognize newline conventions separately from control-character artifacts
- preserve original file format on save unless the user explicitly converts it
- make format state visible in the UI
- avoid confusing users with warnings for legitimate platform text conventions

## Current State

The app already has the foundations of multi-encoding support:

- `src/app/services/file_service.rs` uses BOM detection and `chardetng` heuristics
- `BufferState` stores the detected encoding and BOM state
- saves already write back using the stored encoding and BOM metadata
- binary files are rejected using NUL-byte heuristics
- `TextArtifactSummary` detects ANSI, backspace, carriage returns, and other controls

What is still missing or incomplete:

1. Scratchpad does not have a first-class newline model.
2. Lone `\r` is currently treated as a control artifact even when it may represent a legitimate line-ending style.
3. The app has no explicit representation of mixed line endings.
4. The app does not distinguish format metadata from artifact metadata.
5. User-facing terminology for `ASCII`, `ANSI`, and guessed encodings is still too loose.
6. There is no manual reopen/override flow when detection is ambiguous.
7. OS-aware defaults for new files and save behavior are not yet formalized.

## Format Dimensions To Track

Scratchpad should treat text-file compatibility as five related but distinct dimensions:

### 1. Character Encoding

Examples:

- UTF-8
- UTF-8 with BOM
- UTF-16LE
- UTF-16BE
- Windows-1252
- Shift_JIS
- GBK
- ISO-8859-1

### 2. BOM State

Examples:

- no BOM
- UTF-8 BOM
- UTF-16LE BOM
- UTF-16BE BOM

### 3. Newline Convention

Examples:

- `LF`
- `CRLF`
- `CR`
- mixed newline styles
- no newline separators present

### 4. Control / Artifact State

Examples:

- ANSI escape sequences
- backspaces
- non-newline C0 control characters
- carriage returns used for progress-overwrite behavior rather than line structure

### 5. Detection Confidence / Origin Hints

Examples:

- BOM-derived encoding
- heuristic guess
- ASCII-only content
- platform hint from file history or explicit user override

## Terminology Rules

The plan should be explicit here because users and APIs often use imprecise labels.

### ASCII

- ASCII is not a separate Unicode storage model inside the app.
- If all bytes are ASCII-compatible, Scratchpad should treat the text as an ASCII subset of a compatible encoding, typically UTF-8 or the detected single-byte encoding.
- UI may display `ASCII-only` as a helpful note, but the persisted encoding should still use a real canonical encoding name.

### ANSI

- `ANSI` is not a real encoding identifier.
- In practice it usually means a Windows code page such as `Windows-1252` or another locale-specific ACP.
- Scratchpad should never store `ANSI` as the canonical encoding name.
- UI may optionally show `Windows-1252 (ANSI)` or similar friendly wording, but internal state should keep the true encoding identifier.

### UTF Variants

- UTF-8, UTF-16LE, and UTF-16BE should remain first-class supported encodings.
- UTF-32 support should be evaluated separately because the current stack does not treat it as a normal path.
- If UTF-32 BOM support is added later, it should be explicit and tested rather than implied by heuristic fallback.

## Core Product Rules

### 1. Preserve Format By Default

- Opening a text file must not silently convert encoding, BOM, or line endings.
- Saving must preserve the file's recorded format metadata unless the user explicitly chooses conversion.
- Format conversion should be a deliberate command, not an implicit side effect.

### 2. Separate Newlines From Artifacts

- Legitimate line-ending styles must not be reported as control-character problems.
- `CRLF`, `LF`, and `CR` must be classified as structural newline data first.
- Only remaining standalone `\r` characters that are not acting as line separators should be treated as artifacts.

### 3. Canonical Internal Policy Must Be Explicit

Scratchpad should choose one of these models and implement it consistently:

- preserve raw newline bytes in-memory
- or normalize in-memory text to a canonical newline form and re-encode on save using stored newline metadata

Recommended direction:

- use a canonical internal text model based on `\n` for editor-entered line breaks
- preserve original file newline style as metadata
- serialize back to the saved line-ending style on write
- keep true non-structural control characters exact in the buffer

This removes OS-specific input ambiguity while still preserving file format on save.

### 4. Surface Ambiguity Clearly

- If detection is heuristic rather than BOM-backed, Scratchpad should record that fact.
- The UI should make guessed encodings visible without scaring the user when the text renders correctly.
- If confidence is low or decoding produced replacement characters, the app should offer a reopen-with-encoding action.

## Proposed Data Model

Introduce a distinct text-format metadata model instead of spreading fields across unrelated structures.

Suggested additions:

```rust
pub enum LineEndingStyle {
    Lf,
    Crlf,
    Cr,
    Mixed,
    None,
}

pub enum EncodingSource {
    Bom,
    Heuristic,
    ExplicitUserChoice,
    DefaultForNewFile,
}

pub struct TextFormatMetadata {
    pub encoding_name: String,
    pub encoding_label: String,
    pub has_bom: bool,
    pub line_endings: LineEndingStyle,
    pub line_ending_counts: LineEndingCounts,
    pub encoding_source: EncodingSource,
    pub is_ascii_subset: bool,
    pub has_decoding_warnings: bool,
}

pub struct LineEndingCounts {
    pub lf: usize,
    pub crlf: usize,
    pub cr: usize,
}
```

Where this should live:

- `src/app/domain/buffer.rs`: buffer-owned format metadata
- `src/app/services/file_service.rs`: detection results on load and encoding/newline serialization on save
- session persistence: save and restore this metadata with the buffer

## Detection Pipeline

### 1. Read Raw Prefix Bytes

- keep the current prefix read for BOM and heuristic detection
- retain enough bytes to inspect newline patterns and possible UTF signatures

### 2. Detect BOM First

- BOM remains the highest-confidence source for UTF-8 and UTF-16
- if BOM exists, use it as the authoritative encoding unless decoding fails

### 3. Detect Likely Binary Content

- keep the current binary rejection heuristic
- consider tightening it for high-control-byte density in non-BOM files

### 4. Decode To Unicode Text

- keep the current decoding pipeline for supported encodings
- record whether the result came from BOM or heuristic detection
- record whether replacement characters or other decoding anomalies appeared

### 5. Analyze Newline Structure

Add a dedicated newline scan before artifact classification:

- count `CRLF` pairs
- count lone `LF`
- count lone `CR`
- classify the file as:
  - `CRLF` if all separators are `CRLF`
  - `LF` if all separators are `LF`
  - `CR` if all separators are `CR`
  - `Mixed` if more than one style is present
  - `None` if there are no separators

Important rule:

- if `CR` is the consistent line separator for the file, it should be treated as line-ending structure, not as a carriage-return artifact

### 6. Analyze Control Artifacts After Newline Classification

- ANSI escape sequences remain artifact state
- backspaces remain artifact state
- other control bytes remain artifact state
- lone `\r` should only count as an artifact if it is not consumed by the newline model

This is the key change needed to stop treating legitimate old-Mac `CR` line endings or certain OS newline cases as artifact warnings.

### 7. Derive Friendly UI Labels

Examples:

- `UTF-8`
- `UTF-8 BOM`
- `UTF-16LE`
- `Windows-1252 (ANSI)`
- `UTF-8, ASCII-only`
- `Line endings: CRLF`
- `Line endings: Mixed`

## Open Behavior

When a file is opened:

1. Detect encoding and decode.
2. Detect line endings separately.
3. Detect control artifacts after newline analysis.
4. Store full format metadata on the buffer.
5. Present concise status information in the UI.

Expected behavior:

- Windows files with normal `CRLF` must not be flagged as containing `CR` artifacts.
- Unix files with `LF` must remain plain text.
- classic `CR` line-ending files should be recognized as a newline variation rather than as progress-output artifacts.
- true progress-style `\r` overwrites in logs should still be flagged as control artifacts.

## Save Behavior

Save should operate on explicit policy rather than accidental current-buffer content.

### Existing Files

- preserve encoding
- preserve BOM state
- preserve newline convention if it was unambiguous
- preserve mixed line endings exactly unless the user explicitly normalizes them

### New Files

Introduce a settings-backed default newline policy:

- `Auto (platform)`
- `LF`
- `CRLF`

Recommended default:

- `Auto (platform)` for new unsaved files
- preserve-on-open for existing files

### Manual Conversion Commands

Add explicit commands later for:

- convert encoding
- add/remove BOM
- normalize line endings to `LF`
- normalize line endings to `CRLF`

## UI Plan

### Status Bar

Show format details as separate signals instead of collapsing everything into one warning.

Recommended status-bar fields:

- encoding label
- BOM indicator if present
- line-ending label
- guessed/explicit indicator when useful
- artifact icon only for true control artifacts

Examples:

- `UTF-8 | LF`
- `Windows-1252 (ANSI) | CRLF`
- `UTF-8 | Mixed line endings`
- `UTF-16LE BOM | CR`

### Warnings

Warnings should be reserved for cases such as:

- heuristic encoding guess with low confidence
- mixed line endings
- unsupported encoding fallback
- true control-character artifacts
- decoding loss or replacement-character insertion

### Reopen / Override Flow

Later UI work should allow:

- reopen with different encoding
- reinterpret file as another code page
- convert file format explicitly

## FileService Work

Relevant file:

- `src/app/services/file_service.rs`

Planned changes:

1. Return a richer `FileContent` shape that includes newline metadata and encoding source.
2. Add newline analysis alongside artifact analysis.
3. Stop treating structural `CR` line endings as artifacts.
4. Add serialization helpers that can write text back using a requested line-ending style.
5. Keep binary detection separate from encoding detection.

Suggested API direction:

```rust
pub struct FileContent {
    pub content: String,
    pub format: TextFormatMetadata,
    pub artifact_summary: TextArtifactSummary,
}

pub fn write_file_with_format(
    path: &Path,
    content: &str,
    format: &TextFormatMetadata,
) -> io::Result<()>;
```

## Buffer And Session Work

Relevant files:

- `src/app/domain/buffer.rs`
- session persistence modules

Planned changes:

- move encoding and BOM into shared format metadata
- store newline style on the buffer
- restore format metadata with the session
- ensure editor-inserted newlines follow the buffer policy for that file

## OS Recognition Rules

Scratchpad should recognize common platform conventions without pretending platform equals truth.

### Windows

- common line endings: `CRLF`
- common legacy label users call `ANSI`: ACP such as `Windows-1252`
- new files in auto mode should default to `CRLF`

### Unix / Linux

- common line endings: `LF`
- UTF-8 is the normal default
- new files in auto mode should default to `LF`

### macOS

- modern convention: `LF`
- older classic-Mac content may still use `CR`
- `CR` files should be recognized structurally, not automatically treated as broken text

### Cross-OS Files

- mixed line endings should not be auto-corrected on open
- warnings should say `Mixed line endings` rather than implying corruption

## Testing Matrix

### Encoding Coverage

Add explicit tests for:

- UTF-8 without BOM
- UTF-8 with BOM
- UTF-16LE with BOM
- UTF-16BE with BOM
- Windows-1252
- Shift_JIS
- ASCII-only file detected as UTF-8 subset

### Newline Coverage

Add explicit tests for:

- pure `LF`
- pure `CRLF`
- pure `CR`
- mixed `LF` and `CRLF`
- text containing progress-style `\r` that is not a line ending

### Combined Cases

Examples:

- Windows-1252 with `CRLF`
- UTF-8 with BOM and `LF`
- UTF-16LE with `CRLF`
- ASCII-only text with mixed line endings
- artifact-bearing log file with `LF` plus progress `\r`

### Save / Round-Trip Coverage

Verify that:

- opening then saving preserves encoding
- opening then saving preserves BOM state
- opening then saving preserves newline style
- mixed newline files round-trip unchanged
- explicit conversion commands change only the requested format dimension

## Execution Order

1. Introduce `LineEndingStyle` and newline analysis in `FileService`.
2. Split newline classification from control-artifact classification.
3. Consolidate encoding, BOM, and newline state into `TextFormatMetadata`.
4. Update save logic to use explicit format metadata, not scattered fields.
5. Update status-bar UI to show encoding and newline state separately from artifact warnings.
6. Add focused tests for encoding, BOM, newline, and artifact interactions.
7. Add manual reopen/override and conversion commands later.

## Definition Of Done

This work is complete when:

- Scratchpad opens common UTF and legacy encoded text files predictably
- `CRLF`, `LF`, and `CR` are recognized as newline conventions, not all as artifacts
- artifact warnings are shown only for true control-sequence content
- save preserves encoding, BOM, and newline style by default
- new files use a clear OS-aware or user-configured default
- the UI exposes encoding and line-ending state clearly enough that users can understand what format they are editing
