# Show/Hide Control Characters implementation plan

Reviewed paths: `display_text_slice` / `visible_control_char` / `DisplayTextMap` in `src/app/ui/editor_content/native_editor/layout.rs`, the status-bar toggle in `src/app/ui/status_bar.rs`, the Unicode context-menu toggle in `src/app/ui/editor_area/tile/context_menu/unicode_menu.rs`, per-view propagation in `src/app/domain/tab/layout.rs`, and session restore in `src/app/services/session_store/restore.rs`.

The current render-time substitution plus bidirectional cursor map is the right architectural shape. The next pass should keep that property: control-character visibility must be a display-only view of the buffer, never a document mutation and never a save-time serialization concern.

## Goals

- Show more control characters with single visible glyphs instead of multi-character labels where possible.
- Preserve the exact document contents when toggling, editing, copying, searching, and saving.
- Keep cursor mapping predictable across display substitutions, especially around line endings.
- Remove duplicate toggle state so session, split, and render behavior cannot drift.
- Avoid unnecessary layout-cache invalidation and avoid doing control-character substitution work for clean buffers.

## Glyph strategy

Use two tiers of visible substitutions:

1. Standard Unicode Control Pictures for controls that already have assigned symbols.
2. Scratchpad private-use glyphs for format controls that do not have standard control-picture codepoints.

Noto Sans Symbols 2 supports the Unicode Control Pictures block. Bundle it directly, and also derive `ScratchpadControlSymbols-Regular.ttf` from it so the custom font covers both the public Control Pictures and Scratchpad's private-use glyphs. The derived font should remap the Control Pictures to editor-scaled glyphs so the indicators read at normal text size instead of as tiny fallback symbols.

| Source char | Display glyph | Display codepoint | Notes |
| --- | --- | --- | --- |
| `U+0000`..`U+001F` | `U+2400`..`U+241F` | Control Pictures | Use for C0 controls, with special line-ending handling below. |
| `U+007F` | `␡` | `U+2421` | DEL. |
| `\t` | `␉` | `U+2409` | Use Control Pictures consistently. |
| `\n` | `␊\n` | `U+240A` plus real newline | The marker glyph maps to the source `\n`; the real display newline maps to the cursor immediately after it. |
| bare `\r` | `␍\n` | `U+240D` plus real newline | Bare CR is a document-side line break, so it must create a visual row break. |
| CRLF `\r\n` | `␍␊\n` | independent `U+240D` and `U+240A\n` substitutions | Keep `\r` and `\n` independent. `\r` maps as a single cell; `\n` uses `LineEndingMarker`. |

For Unicode format controls such as zero-width and bidi controls, create a renamed derived font, for example `ScratchpadControlSymbols-Regular.ttf`, based on Noto Sans Symbols 2's visual style. Do not ship a modified font under the Noto family name. Keep the OFL license/copyright notice with the bundled font.

Assign custom glyphs in the Private Use Area starting at `U+F000`. This avoids `egui-phosphor`, which occupies lower PUA codepoints beginning at `U+E000`.

| Source char | Current label | PUA display char |
| --- | --- | --- |
| `U+200B` | `<ZWSP>` | `U+F000` |
| `U+200C` | `<ZWNJ>` | `U+F001` |
| `U+200D` | `<ZWJ>` | `U+F002` |
| `U+200E` | `<LRM>` | `U+F003` |
| `U+200F` | `<RLM>` | `U+F004` |
| `U+202A` | `<LRE>` | `U+F005` |
| `U+202B` | `<RLE>` | `U+F006` |
| `U+202C` | `<PDF>` | `U+F007` |
| `U+202D` | `<LRO>` | `U+F008` |
| `U+202E` | `<RLO>` | `U+F009` |
| `U+2060` | `<WJ>` | `U+F00A` |
| `U+2061` | `<FA>` | `U+F00B` |
| `U+2062` | `<IT>` | `U+F00C` |
| `U+2063` | `<IS>` | `U+F00D` |
| `U+2064` | `<IP>` | `U+F00E` |
| `U+2066` | `<LRI>` | `U+F00F` |
| `U+2067` | `<RLI>` | `U+F010` |
| `U+2068` | `<FSI>` | `U+F011` |
| `U+2069` | `<PDI>` | `U+F012` |
| `U+FEFF` | `<BOM>` | `U+F013` |
| `U+061C` | `<ALM>` | `U+F014` |
| `U+206A` | `<ISS>` | `U+F015` |
| `U+206B` | `<ASS>` | `U+F016` |
| `U+206C` | `<IAFS>` | `U+F017` |
| `U+206D` | `<AAFS>` | `U+F018` |
| `U+206E` | `<NADS>` | `U+F019` |
| `U+206F` | `<NODS>` | `U+F01A` |

The custom glyphs should visually encode the short label in a compact single-cell-ish mark. They do not need to be semantically meaningful Unicode characters because the renderer maps them back through `DisplayTextMap`.

## Implementation steps

1. Bundle and register the symbols font.

Add `fonts/NotoSansSymbols2-Regular.ttf` and `fonts/ScratchpadControlSymbols-Regular.ttf`. Register the Scratchpad font, then Noto Sans Symbols 2, in `src/app/fonts.rs` after the selected editor font and before CJK fallbacks. This keeps control glyphs available in every editor preset without changing normal text shaping. The generated Scratchpad control-symbol font is checked in alongside the other bundled fonts.

2. Replace string labels with display glyph specs.

Change `visible_control_char` into a helper that returns a structured substitution, for example:

```rust
struct VisibleControlSubstitution {
    text: &'static str,
    cursor_policy: CursorSubstitutionPolicy,
}
```

Use standard control pictures for C0/DEL and PUA glyphs for Unicode format controls. Keep a fallback textual form such as `\xNN` only for control characters outside the explicitly supported map.

3. Fix line-ending display at the same time.

Bare CR cannot render as only `␍` while metadata treats it as a line break. Pin the local contract as `\r` -> `␍\n` for bare CR. For CRLF, keep `\r` and `\n` as independent substitutions: `\r` -> `␍`, then `\n` -> `␊\n`.

4. Make cursor mapping policy explicit.

The current `len.div_ceil(2)` split is too implicit for line-ending substitutions. Replace it with named policies:

- `SingleCell`: all interior display positions map to either the source doc char or the next doc cursor based on a documented midpoint.
- `LineEndingMarker`: the marker glyph maps to the line-ending char, and the real display newline maps to the cursor after the line-ending char.
This should make the “click on newline marker” behavior testable instead of incidental.

5. Collapse toggle state to the buffer.

Remove `EditorViewState::show_control_chars` unless a real per-view mode is desired. Rendering, status, context menu, session storage, restore, and split propagation should all use `BufferState::show_control_chars` as the source of truth.

Keep the existing backwards-compatible restore migration: read legacy `SessionView::show_control_chars` and OR it into the buffer flag during restore when the buffer still has controls. Delete the live persisting side and split-time `view.show_control_chars` plumbing.

6. Stop clearing layout caches on toggle.

`show_control_chars` is already part of `LayoutCacheKey`. Remove explicit `view.layout_cache.clear()` calls from both:

- `src/app/ui/status_bar.rs`
- `src/app/ui/editor_area/tile/context_menu/unicode_menu.rs`

The cache key should select the correct layout. Revision retention still handles document changes.

7. Gate or auto-clear irrelevant mode.

Auto-clear `buffer.show_control_chars` when there are no remaining visible substitutions after edits. The status/context toggle may still be disabled for a currently clean buffer; the important state contract is that visible-control mode cannot remain stuck on after the last substitution disappears.

8. Document the search/selection zero-width range contract.

Add a comment near `DisplayTextMap::doc_range_to_display` that `None` means “no visible range for this caller,” not necessarily “out of bounds.” This protects future cursor-oriented callers from using the search/selection helper incorrectly.

## Tests

Add or update focused tests in `src/app/ui/editor_content/native_editor/layout.rs`:

- C0 controls map to Control Pictures codepoints.
- DEL maps to `␡`.
- Unicode format controls map to the configured PUA codepoints.
- LF substitution keeps a real newline and has explicit cursor mapping.
- Bare CR substitution creates a visual row break and maps back to the single source char.
- CRLF behavior is pinned, including cursor mapping around both source chars.
- `doc_range_to_display` behavior for zero-width ranges is documented by a test or comment.
- Copy/cut selection paths return original document text, not display glyphs.
- Undo after editing text containing substituted controls restores original document text, not display glyphs.

Add state tests around:

- session restore migrates view-level show-control state into the buffer flag only when the buffer still has controls;
- splitting a view does not introduce a second source of truth;
- toggling twice can reuse cache entries because explicit cache clears are gone;
- editing away all controls disables or clears visible-control mode.

## Verification checklist

- Open a file with LF, CRLF, CR-only, and mixed line endings.
- Toggle control characters on and off while checking row count, cursor clicks, arrow navigation, Home/End, search highlight placement, and selection painting.
- Open a file with `ZWSP`, `LRM`, `RLO`, and `PDF`; verify the custom glyphs appear and surrounding text no longer reflows because the bidi controls are not rendered as active bidi formatting controls.
- Copy and save the file and verify bytes/text are unchanged unless the user actually edited text or changed encoding/line-ending policy.
- Confirm fallback behavior when the custom font is missing during development fails visibly, not silently.
