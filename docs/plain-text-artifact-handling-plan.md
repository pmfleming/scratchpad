# Plan: Plain-Text Handling for Styled and Control-Sequence Output

This plan describes how Scratchpad should behave when it opens files that contain terminal styling, ANSI escape sequences, carriage-return progress updates, backspace overstrikes, and other non-printing control characters.

The goal is not to reinterpret these files as terminal output. Scratchpad is a plain-text editor, so it should preserve the underlying text while presenting it in a predictable, debuggable way.

## Objectives
- Treat terminal/log artifacts as valid text input unless the file is clearly binary.
- Preserve file content exactly on load and save by default.
- Make non-printing sequences visible enough that the document remains understandable.
- Keep line counts, gutters, scrolling, and EOF behavior aligned with the actual buffer model.
- Avoid silent cleanup during normal open operations.

## Problem Statement
Some text files are produced by tools that emit styling or terminal-control output, for example:
- ANSI SGR color sequences such as `ESC[31m`.
- Other ANSI control sequences such as cursor movement or line erasure.
- Carriage-return-only progress updates using `\r`.
- Backspace overstrike output using `\b`.
- Mixed terminal artifacts embedded in otherwise normal text reports.

If Scratchpad loads these files naively, several things can go wrong:
- The document may visually appear to continue past the reported line count.
- Invisible control characters may affect layout assumptions.
- The user cannot tell whether the editor preserved the raw file or modified it.
- Save operations may silently destroy information if the editor normalizes content without telling the user.

## Product Principles

### 1. Preserve by Default
- Opening a file must not strip ANSI or control-sequence content.
- Saving a file must write back the same logical text content the user sees in the raw editor buffer.
- Sanitization should be an explicit action, not an implicit side effect of load.

### 2. Raw Text First
- Scratchpad should treat these files as plain text, not as terminal sessions to replay.
- Escape sequences should not be interpreted into colors, cursor movement, or overwrites in the main editor.
- The default editor surface should show the raw text in a stable way.

### 3. Visible Diagnostics
- If a file contains control-sequence artifacts, the user should be told that the file includes non-printing characters.
- The editor should provide a clear indication of which artifact classes are present.
- The user should be able to inspect the raw content without ambiguity.

### 4. Accurate Buffer Semantics
- Line counts must be derived from the same newline model the editor uses for layout.
- A trailing newline must count as a final empty line when displayed in the gutter.
- EOF detection must depend on the actual buffer contents, not on render height or text shaping behavior.

## Expected Behavior

### Open Behavior
- If the file decodes as text, Scratchpad should open it even when it contains ANSI or control characters.
- If the file appears binary, Scratchpad should continue to reject it.
- On open, Scratchpad should scan the decoded text and compute artifact flags such as:
  - ANSI escape sequences present
  - Carriage returns present
  - Backspaces present
  - Other C0 controls present

### Editor Rendering
- The default editor view should remain a raw-text view.
- Non-printing sequences should not be executed.
- Control characters that are invisible in normal text should have an inspectable representation.

### Status Bar Icons
- The status bar should always show one artifact-state icon for the active tab.
- Use Phosphor `FileText` for plain text with no detected control-sequence artifacts.
- Use Phosphor `WarningCircle` for text that contains ANSI or other control characters.
- The icon should appear near the existing encoding and line-count indicators in the bottom status bar.
- The icon should include hover text describing the current state.

Recommended behavior:
- `FileText`: tooltip `Plain text; no control characters detected`
- `WarningCircle`: tooltip `Control characters present; click to inspect`
- Clicking `WarningCircle` toggles visible-control inspection mode for the active tab.
- When visible-control inspection mode is active, the warning-state icon should remain present and its tooltip should indicate that control characters are currently visible.

Recommended rendering policy:
- Preserve the underlying buffer text exactly.
- Add an optional “show control characters” mode for raw inspection.
- In that mode, render visible glyphs or escaped tokens for characters such as:
  - `ESC` as `␛` or `\\x1B`
  - `CR` as `␍`
  - `LF` as `␊` only in dedicated inspection contexts, not inline by default
  - `TAB` as `→` or a configurable visible-tab marker
  - `BS` as `␈`

### Status and Warnings
- The status bar should show a concise warning when artifact-bearing files are open.
- The status bar should also show the artifact-state icon described above so the user can see the state at a glance.
- Example status text:
  - `ANSI escapes present`
  - `Control characters present: CR, ESC`
  - `Raw text view; control sequences are not interpreted`

Icon interaction:
- Clicking the warning-state icon should switch the active tab into visible-control mode.
- Clicking it again should return the tab to the default raw-text presentation.
- Clicking the plain-text icon should do nothing except show its tooltip, since there is nothing to inspect.

### Save Behavior
- Normal save should preserve the current buffer content as text.
- If the user edits the file while control characters are present, the editor should still save exactly what is in the buffer.
- If the project later adds a sanitize/export flow, it should be separate from normal save.

## Implementation Areas

### 1. File Analysis in `FileService`
Relevant file:
- `src/app/services/file_service.rs`

Add a lightweight text-artifact scan after decoding:
- Detect ANSI CSI and OSC patterns.
- Count `\r`, `\b`, and other control characters except allowed text separators such as `\n` and `\t`.
- Return this metadata alongside decoded content.

Suggested model:
```rust
pub struct TextArtifactSummary {
    pub has_ansi_sequences: bool,
    pub has_carriage_returns: bool,
    pub has_backspaces: bool,
    pub other_control_count: usize,
}
```

Update `FileContent` to carry artifact metadata so UI and buffer code can use it.

### 2. Buffer Metadata in `BufferState`
Relevant file:
- `src/app/domain/buffer.rs`

Extend the buffer model to retain text artifact state:
- Whether ANSI/control characters are present.
- Whether visible-control rendering is enabled for the tab.
- Potentially whether the file should open in a warning state.
- Which status-bar icon state should be shown for the tab.

Suggested additions:
```rust
pub struct BufferState {
    // existing fields...
    pub artifact_summary: TextArtifactSummary,
    pub show_control_chars: bool,
}
```

### 3. Accurate Line Counting
Relevant files:
- `src/app/domain/buffer.rs`
- `src/app/ui/editor_area.rs`

Current behavior uses `content.lines().count().max(1)`, which does not count the final empty line when the file ends with a trailing newline.

That is not the same model the editor UI implies to users.

Replace it with a line-count function based on newline separators in the actual buffer:
- Empty string => 1 line
- Otherwise line count = number of `\n` characters + 1

This keeps status-bar counts, line-number gutters, and EOF semantics aligned with what users see in the document.

Suggested helper:
```rust
fn display_line_count(text: &str) -> usize {
    if text.is_empty() {
        1
    } else {
        bytecount::count(text.as_bytes(), b'\n') + 1
    }
}
```

The implementation does not need `bytecount`; a simple byte iteration is also fine.

### 4. Control-Character Visualization
Relevant file:
- `src/app/ui/editor_area.rs`

Add a display option for visible control characters.

Status-bar interaction requirement:
- The visible-control toggle should be exposed through the bottom status bar icon, not only through a command.
- If the active buffer contains control characters, the warning icon becomes the primary affordance for entering and leaving inspection mode.

Recommended phased behavior:
- Phase 1: Warning-only. Preserve raw content and show artifact presence in the status bar.
- Phase 2: Toggle to visualize control characters in a non-destructive inspection mode, driven by the warning icon in the status bar.
- Phase 3: Optional command to create a sanitized copy for logs/reports.

Important constraint:
- The visible-control presentation must not mutate `buffer.content` just to improve rendering.
- Rendering transformations should be view-only.

### 5. Commands and UX
Relevant files:
- `src/app/app_state.rs`
- `src/app/commands.rs`
- `src/app/ui/editor_area.rs`

Potential user actions:
- Toggle visible control characters for the active tab.
- Copy a sanitized version of the file to a new tab.
- Save sanitized copy as a separate file.

Reasonable default UX:
- Open raw.
- Show the appropriate Phosphor status icon plus a warning when needed.
- Let the user opt into inspection or sanitization.

Chosen icon set:
- Clean text state: Phosphor `FileText`
- Control-character state: Phosphor `WarningCircle`

Why these icons:
- `FileText` communicates that the buffer is ordinary text.
- `WarningCircle` communicates that the buffer is still text, but contains unusual content that merits inspection.

## Detection Scope
The project should explicitly handle at least these classes:

### ANSI Escape Sequences
- CSI sequences such as `ESC[31m`, `ESC[2K`, `ESC[1A`.
- OSC sequences when present in captured terminal output.

### Carriage Return Output
- Progress-style logs such as `Downloading... 10%\rDownloading... 20%\r...`
- These should remain raw text in the main buffer.

### Backspace Overstrike
- Text patterns used by old terminal tools to bold or redraw content.

### Other Control Characters
- Any C0 controls except ordinary text separators that the editor intentionally supports.

## Non-Goals
- Do not emulate a terminal.
- Do not replay cursor movement in the main text editor.
- Do not silently strip control characters during load.
- Do not silently rewrite files just because they contain escape sequences.

## Testing Plan

### Fixtures
Create test fixtures for:
- Plain UTF-8 report with ANSI SGR sequences.
- Progress log with repeated `\r` updates.
- Text containing backspaces.
- File with trailing newline and control characters mixed in.

### Verification Cases
- Opening such files does not trigger binary-file rejection.
- Artifact metadata is populated correctly.
- The status bar surfaces artifact warnings.
- Line counts match the editor-visible line model, including trailing newline cases.
- Visible-control mode does not alter underlying saved content.
- Saving without sanitization preserves the original control characters.

## Rollout Plan

### Phase 1
- Add artifact detection metadata.
- Fix line counting to match displayed EOF semantics.
- Show status-bar warnings for ANSI/control content.

### Phase 2
- Add per-tab visible-control rendering.
- Add a command to toggle the inspection mode.

### Phase 3
- Add explicit sanitize/export actions for log cleanup workflows.

## Success Criteria
- Scratchpad can open text files containing ANSI/control artifacts without corrupting them.
- Users can understand why a file looks unusual.
- Line counts and EOF behavior are consistent and unsurprising.
- The editor remains a plain-text editor rather than drifting into terminal emulation.