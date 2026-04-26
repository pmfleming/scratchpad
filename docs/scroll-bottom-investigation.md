# Scroll-to-bottom Investigation

Date: 2026-04-26

## Question

Scrollbars and arrow keys do not reliably allow the user to reach the bottom of a file. The working hypothesis is that each tile needs scroll data based on the file length and the tile-specific number of visible rows, because wrapping and split tiles change the viewport width and height.

## Executive Summary

The hypothesis is mostly right, but the most likely failure is more specific: the editor often allocates scrollable height from logical line count, while the actual rendered content can be taller than that. This is especially risky when word wrap is enabled or a split tile becomes narrow. A narrow tile can turn one logical line into many visual rows, but `render_editor_text_edit` still sizes the scroll area as `line_count * row_height`.

All input routes eventually depend on the same computed scroll bounds. Mouse wheel, drag-scroll, scrollbar motion, PageUp/PageDown, and cursor-follow scrolling all clamp to `output.content_size - viewport_size` in `src/app/ui/editor_area/tile.rs`. If `output.content_size.y` is too small, every scrolling method will stop early even though more text exists below.

The code already stores scroll offset per view/tile, so lack of independent offset is probably not the primary issue. What is missing is a reliable per-tile content extent that accounts for the current tile width, wrapping mode, font size, gutter, and visible-window mode.

## Relevant Architecture

- Each split pane leaf maps to an `EditorViewState`, and each view owns `editor_scroll_offset` in `src/app/domain/view.rs:85`.
- `show_editor_scroll_area` reads that offset, passes it to `egui::ScrollArea::both()`, renders the tile content with `show_viewport`, then writes the resolved offset back to the same view in `src/app/ui/editor_area/tile.rs:675`.
- Scroll offset is clamped by `resolve_editor_scroll_offset`, `clamp_scroll_offset`, and `max_scroll_offset` in `src/app/ui/editor_area/tile.rs:732`, `src/app/ui/editor_area/tile.rs:816`, and `src/app/ui/editor_area/tile.rs:828`.
- Splits create smaller tile rectangles through `render_split_pane` and `split_rect` in `src/app/ui/editor_area/mod.rs:246` and `src/app/ui/editor_area/divider.rs:143`.

This means tile-local scroll offset already exists. The open question is whether each tile reports a correct scrollable content size.

## Main Finding: Wrapped Text Can Underestimate Content Height

In `render_editor_text_edit`, the full document galley is built using a wrap width derived from the current tile:

- `render_editor_text_edit`: `src/app/ui/editor_content/native_editor/mod.rs:64`
- `editor_desired_size`: `src/app/ui/editor_content/native_editor/mod.rs:782`

However, the allocated height is based on `buffer.line_count.max(1)` rather than the galley height or visual row count. With word wrap off, logical lines usually match visual rows. With word wrap on, or with narrow tiles, one logical line can occupy multiple visual rows. The galley knows this, but the scroll area content height can still be too short.

Result: the scrollbar maximum is too small, so the bottom of the painted text can exist beyond the scrollable range.

This matches the user-visible symptom very closely: the viewport stops before EOF, and neither dragging nor arrow keys can move farther.

## Large-file Visible-window Mode

For large unwrapped files, the editor switches to visible-window rendering:

- selection logic: `src/app/ui/editor_content/mod.rs:134` and `src/app/ui/editor_content/mod.rs:149`
- read-only window entry: `src/app/ui/editor_content/native_editor/mod.rs:162`
- focused window entry: `src/app/ui/editor_content/native_editor/mod.rs:188`
- viewport-to-line mapping: `src/app/ui/editor_content/native_editor/mod.rs:1007`
- visible line extraction: `src/app/domain/buffer/state.rs:316`

This path is explicitly disabled for word wrap. Its virtual height is represented by top padding + visible line allocation + bottom padding in `render_visible_text_window` at `src/app/ui/editor_content/native_editor/mod.rs:475`.

For unwrapped text, logical line count is a reasonable proxy for visual row count. This mode is less likely to fail because of wrapping, but it can still fail if `buffer.line_count` is stale or wrong. The staged large-file line counter is now CR/LF-aware at `src/app/services/file_service.rs:326` and `src/app/services/file_service.rs:349`, which reduces one known line-count risk.

## Arrow Keys and Page Keys

Arrow keys are not a separate scrolling system. They move the cursor, then request scroll-to-cursor:

- wrapped/full galley cursor movement: `src/app/ui/editor_content/native_editor/cursor.rs:68`
- unwrapped/window cursor movement: `src/app/ui/editor_content/native_editor/cursor.rs:146`
- PageUp/PageDown scroll request: `src/app/ui/editor_content/native_editor/mod.rs:1096`
- cursor visibility scroll request: `src/app/ui/editor_content/native_editor/mod.rs:1144`

Those requested offsets still flow through the same final clamp in `tile.rs`. So if the scroll area content height is underestimated, keys will appear broken too.

## Other Plausible Causes

1. Stale layout after tile resize

   Tile splits, divider resizing, font changes, and word-wrap changes alter visual row count. `latest_layout` is view-owned and revision-checked, but document revision does not change when only the viewport width changes. Cache keys for visible-window layout include wrap width, but the main full-document path still sizes from line count. Width-only layout changes deserve explicit invalidation or extent recalculation.

2. Gutter/content height mismatch

   The line-number gutter sometimes uses `layout.content_height()` when a previous layout exists, but fallback uses `line_count * row_height` in `src/app/ui/editor_content/gutter.rs`. The gutter is probably not the root cause, but it can participate in content-size inconsistencies when text and gutter disagree about height.

3. Focus and active-view transitions

   Large files use different paths depending on whether the view is active/focused. A tile can move between full render, read-only visible-window render, and focused visible-window render. Each path should preserve equivalent scroll extents for the same tile; otherwise the scrollbar can jump or clamp.

4. Scroll input priority

   `resolve_editor_scroll_offset` prefers wheel, then pointer-drag, then editor-requested offsets. That is probably fine, but all of them clamp to `output.content_size`. A bad content size masks as bad input handling.

## Hypothesis Verdict

The hypothesis is directionally correct:

- Each tile already has independent scroll offset.
- Each tile does need a scroll extent derived from its own viewport width and height.
- File length alone is insufficient; visual row count is the key value for scrolling.

The missing piece is not primarily independent storage. It is reliable per-tile measurement of rendered content height under the current view conditions.

## Recommended Fix Direction

1. Treat scroll extent as tile-local derived state.

   For each tile render, compute content height from the actual rendered layout whenever possible. In the full-document path, prefer galley height or visual row count over logical `line_count * row_height`.

2. Separate logical file length from visual scroll extent.

   Keep `BufferLength` for bytes/chars/logical lines. Add or derive a `ViewExtent` concept for visual rows/pixels based on tile width, wrap mode, font, and rendering mode.

3. Make wrapped mode explicit.

   When word wrap is enabled, scroll height must use visual rows. This is the strongest suspected defect.

4. Add tests for narrow wrapped tiles.

   A regression test should create a long single-line buffer, render with a narrow wrap width, and assert that scrollable content height exceeds one logical row and permits reaching the final visual row.

5. Add split-tile tests.

   Duplicate a buffer into two views, assign different tile widths, and assert each view has independent offsets and independently computed scroll bounds.

6. Add instrumentation while debugging.

   Temporarily log or surface: view id, tile rect size, word wrap, logical line count, galley visual row count, allocated content height, scroll area `content_size`, viewport size, and max scroll offset. The bug should become obvious when content height is lower than galley height.

## Highest-risk Code Paths

- Full wrapped rendering: `src/app/ui/editor_content/native_editor/mod.rs:64`
- Desired size calculation: `src/app/ui/editor_content/native_editor/mod.rs:782`
- Final scroll clamp: `src/app/ui/editor_area/tile.rs:732`
- Large-file visible window sizing: `src/app/ui/editor_content/native_editor/mod.rs:475`
- View-specific offset storage: `src/app/domain/view.rs:85`

## Conclusion

The user symptom is best explained by scroll bounds being calculated from an underestimated content height. Split tiles make this more likely because they reduce tile width, which increases wrapped visual rows. The current code already has independent view offsets, but it does not consistently use independent, tile-specific visual content extents. The first fix should focus on replacing logical-line height estimates with visual-layout height in the full render path, then validating that visible-window mode and split resizing preserve correct per-tile scroll bounds.
