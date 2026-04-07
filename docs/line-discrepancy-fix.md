# Line Discrepancy Fix

## Problem

The status bar currently shows a logical line count derived from the buffer text. That is useful, but it can disagree with what the user perceives on screen for files that:

- use mixed newline styles
- contain long wrapped lines
- render extra visual artifacts

This is most obvious on large reports such as `docs/complexity-report.txt`, where the displayed line count can suggest the file ends earlier than it really does.

## Current State

The following pieces are already in place:

- logical line counting handles `\n`, `\r`, and `\r\n`
- buffers store a `latest_galley` field
- the status bar can show both logical line count and rendered row count when a galley is available

That means the remaining work is not "count lines correctly from scratch." The remaining work is making the visual count and gutter behavior reliable.

## Goal

Make the editor communicate two different things clearly:

- `Lines`: logical lines in the document
- `Rows`: rendered visual rows in the current editor layout

If line numbers are shown in the left gutter, they should align with the rendered rows without depending on brittle egui internals.

## Constraints

- Do not assume egui row internals are stable across versions.
- Do not rely on APIs that existed in older egui versions but are not present now.
- Do not mutate the active buffer from inside the `TextEdit::layouter` callback unless the borrowing model makes that completely safe.
- Avoid any approach that requires a second full text layout pass on every frame.

## Recommended Approach

### 1. Keep logical and visual counts separate

Logical line count should continue to come from the buffer text. That is stable and already implemented.

Visual row count should come from the currently rendered galley when one is available. If no galley is available yet, the UI should fall back to showing only the logical line count.

### 2. Capture layout results in a controlled way

The editor should keep using the galley produced for the actual text edit layout, but the implementation should not assume that mutating buffer state directly from inside the layouter closure is trivial.

Preferred options:

- capture the returned `Arc<Galley>` through local state in the editor rendering flow, then write it back to the buffer after the `TextEdit` call completes
- or keep the most recent galley in UI-local state keyed by buffer identity if that is easier to manage safely

The important part is to reuse the layout that already happened instead of performing a separate layout just to count rows.

### 3. Treat the gutter as a row-aligned display, not a text parser

The gutter should align to rendered rows, but the implementation should avoid depending on unstable low-level row metadata where possible.

Safer direction:

- render one gutter entry per visible galley row
- show a line number only on the first rendered row of each logical line
- show blank gutter entries for wrapped continuation rows

If egui does not expose enough row detail to do this robustly, prefer a simpler gutter over a fragile one.

### 4. Be explicit in the status bar

The status area should clearly distinguish:

- `Lines: <logical_count>`
- `Rows: <visual_count>` when available

This prevents users from reading the logical line count as a statement about what is currently visible on screen.

### 5. Validate against the real failure case

The fix should be tested against:

- `docs/complexity-report.txt`
- files with `\n`
- files with `\r\n`
- files with mixed newline styles
- long wrapped lines

The key check is that the document does not appear to stop at the reported logical line count when the UI is still rendering more wrapped rows.

## Non-Goals

- perfect row introspection through egui-private or unstable APIs
- pixel-perfect gutter behavior if that requires a brittle implementation
- any extra layout pass purely for metrics

## Summary

The right fix is not to replace the logical line count. It is to present logical lines and visual rows as separate concepts, reuse the existing layout result when possible, and avoid fragile dependence on egui row internals.
