# Editor Area, Scrolling & Selection Review

Review of the editor tile/pane layer, the native editor (cursor, selection, painting), and the scrolling subsystem against best practice, speed, utility, and functionality. Code was not modified — this is an analysis report only.

Special focus, per the request: how scrolling and selection actually work end-to-end.

Date: 2026-05-06

---

## 1. Scope & Method

In-scope code (~7,000 lines across 24 files):

| Area | Path |
| --- | --- |
| Pane / tile layout | `src/app/ui/editor_area/{mod,divider,tile,tile/{chrome,scroll_input,autoscroll,context_menu}}.rs` |
| Editor content frame | `src/app/ui/editor_content/{mod,artifact,gutter}.rs` |
| Native editor | `src/app/ui/editor_content/native_editor/{mod,layout,painting,highlighting,types,cursor,editing,word_boundary,interactions/{keyboard,mouse}}.rs` |
| Scrolling | `src/app/ui/scrolling/{mod,area,manager,anchor,display,intent,metrics,source,state,target}.rs` |
| Layout cache | `src/app/domain/view/layout_cache.rs` |

Findings cite paths and line numbers from `master` (`a9b93a00`).

---

## 2. Architecture Summary

The editor surface composes four cleanly separated layers:

1. **Pane tree → tiles.** `editor_area/mod.rs` walks the per-tab `PaneNode` (Split | Leaf). Leaves render as a tile via `tile.rs`; splits render dividers (`divider.rs`). Each tile owns a header (`tile/chrome.rs`), a body that hosts an `egui::ScrollArea`, and a context menu (`tile/context_menu.rs`). Tile actions (Activate / Close / Promote / ResizeSplit / Split) are collected and applied after rendering.

2. **Editor content frame.** `editor_content/mod.rs` adds the gutter (`gutter.rs`) and dispatches to either the native editor or a fallback artifact widget (`editor_content/artifact.rs`). Layout decides which view is active via the active tab's `active_view_id`, set in `editor_area/mod.rs` and consumed in `tile.rs` to drive focus/styling.

3. **Native editor.** `native_editor/mod.rs` owns one frame's worth of work: build galley → process input → paint. The galley is laid out once per visible-slice change and cached (`domain/view/layout_cache.rs`). Cursor logic, key handling, mouse handling, painting, and word-boundary logic each live in their own sub-module.

4. **Scrolling.** A `ScrollManager` per `EditorViewState` owns the application-level truth (anchor + horizontal pixels + viewport metrics + extent + edge-autoscroll velocity). A `ScrollState` keyed by `egui::Id` mirrors pixel-space state for the embedded `ScrollArea`. Scroll commands flow as `ScrollIntent`s queued on the view; the renderer drains them per frame through `ScrollManager::apply_intent` (`scrolling/manager.rs:87-155`).

The split between **what** scrolls (anchor in row-space) and **how** the egui `ScrollArea` is driven (pixel offset) is the cleanest decision in this subsystem.

---

## 3. How Scrolling Works (end-to-end)

The model is intent-driven and anchor-based.

### Anchors

`ScrollAnchor` (`scrolling/anchor.rs:24-37`) has two variants:

- `Piece { anchor: AnchorId, display_row_offset: f32 }` — a piece-tree anchor with a fractional row offset *within* the wrapped block at the top of the viewport.
- `Logical { logical_line, byte_in_line, display_row_offset }` — a fallback used before first render and in tests.

The piece anchor survives edits made above it: `piece_tree.anchor_position(id)` resolves to a current `char_offset` each frame, and the renderer converts that into a display-row pixel offset for the `ScrollArea`. This is the right design — far more stable than persisting raw pixel offsets.

`tile.rs` provides a recovery path (`recover_unresolved_piece_anchor`, ~lines 411-447) that downgrades to a saved pixel offset if a piece anchor cannot be resolved (e.g. content was deleted). On the next frame, `upgrade_scroll_anchor_to_piece` (~line 498) re-establishes the piece anchor.

### `ScrollIntent` and `ScrollManager`

Scroll commands are an enum (`scrolling/intent.rs:10-35`):

`Wheel`, `ScrollbarTo { axis, offset_pixels }`, `Lines(i32)`, `Pages(i32)`, `Top`, `Bottom`, `Reveal { rect, align_y, align_x }`, `RestoreAnchor(anchor)`, `EdgeAutoscroll { axis, velocity }`.

`ScrollManager::apply_intent` (`manager.rs:87-155`) is the **single mutation entry point**, with two closures for converting between anchors and rows. It updates the anchor, sets the `user_scrolled` flag (cleared on `Top`/`Bottom`/`Reveal`/`RestoreAnchor`, set on `Wheel`/`ScrollbarTo`/`Lines`/`Pages`), and clamps to bounds.

### Reveal trace

A typical "reveal cursor" flow:

1. Cursor moves (key or click).
2. `painting.rs::paint_cursor_effects` calls `view.request_cursor_reveal(mode)`.
3. This pushes a `ScrollIntent::Reveal { rect, align_y, align_x }` onto `view.pending_intents`.
4. `tile.rs::drain_pending_scroll_intents` pops them and calls `ScrollManager::apply_intent`.
5. `Reveal` computes the new pixel offset to bring the rect on-screen, then converts back to anchor via `row_to_anchor`.
6. `tile.rs::editor_pixel_offset_resolved` converts anchor → row → pixel for the next `ScrollArea` offset.

This pipeline is correct and robust to edits between request and apply because the anchor is always resolved fresh.

### Edge-autoscroll on drag-select

`tile/autoscroll.rs:14-36` watches the primary pointer during a drag. When the pointer exits the viewport (`autoscroll.rs:38-48` configures: edge zone = max(1.5×row_height, 24 px); outside zone = 8×row_height; velocity range 8–120 rows/s linear), it pushes `ScrollIntent::EdgeAutoscroll { axis, velocity }`. The manager stores velocity in `edge_autoscroll_y/x` (`manager.rs:29-32`) and `tick_edge_autoscroll(dt, ...)` consumes it per-frame. Cursor-reveal intents are suppressed during drag to prevent snap-back. Both axes are supported.

This is well-shaped: linear (predictable) velocity, two-zone falloff, generation-resistant via being stored on the manager rather than as a one-shot intent.

---

## 4. How Selection Works (end-to-end)

`CursorRange` (`native_editor/types.rs:86-127`):

- `primary: CharCursor` — moving end.
- `secondary: CharCursor` — fixed end (anchor for shift-click extension).
- `CharCursor { index: usize, prefer_next_row: bool }` — char-based, with row-preference flag for vertical navigation through wrapped lines.

### Mouse selection

`interactions/mouse.rs:41-104` is the entry point. Each frame:

1. `pointer_selection` maps screen pos → galley `CCursor` → document char index (`mouse.rs:193-216`).
2. `update_click_count` (`mouse.rs:115-127`) tracks repeat clicks within `MULTI_CLICK_MAX_DELAY = 0.4s` and `MULTI_CLICK_MAX_DISTANCE = 4 px`.
3. `apply_click_selection` (`mouse.rs:129-150`) branches on click count:
   - 1 click: `cursor_range_after_click` — sets a single cursor, or extends from `secondary` if Shift is held.
   - 2 clicks: word selection via `word_boundary::{word_start, word_end}` against the **piece tree directly** (so word selection crosses the visible-slice boundary correctly).
   - 3+ clicks: line via `galley.cursor_begin_of_row` / `cursor_end_of_row`.
4. While dragging (`response.dragged()`), `extend_selection_to_cursor` (`mouse.rs:106-113`) keeps `secondary` and moves `primary` to the pointer.
5. Click state (`ClickState`) is persisted in `egui::Memory`, keyed by `response.id.with("click_state")`.

`normalize_click_count` (`mouse.rs:180-191`) drops a multi-click back to single if the pointer is at the row's end-of-row position — a small UX detail to avoid selecting the line when clicking past the last character.

### Keyboard selection

`cursor.rs::apply_cursor_movement` (lines 68-109) is the dispatcher. It tries, in order:

- Horizontal (`horizontal_movement_target`, lines 111-141): Arrow Left/Right, with `is_wordwise_movement(modifiers) = modifiers.alt || modifiers.ctrl` (`cursor.rs:6-8`) jumping by word via `word_boundary::find_word_boundary_left/right` against the piece tree.
- Vertical (`vertical_movement_target`, lines 151-164): Arrow Up/Down, with `modifiers.command` jumping to galley begin/end.
- Row edge (`row_edge_movement_target`, lines 166-179): Home/End, with `modifiers.command` jumping to galley begin/end.
- Page (`page_movement_target`, lines 181-197): PageUp/PageDown stepping `page_jump_rows` rows at a time.

`finalize_cursor_movement` (lines 31-49) holds `secondary` if Shift is down (extending the selection), otherwise collapses to a single cursor. There's a nice subtlety: on a non-Shift left/right with an existing selection, `collapsed_selection_target` (lines 10-29) collapses to the start (Left) or end (Right) of the selection rather than moving by one — standard editor behaviour.

---

## 5. Strengths

- **Anchor-based scrolling** survives edits-above and viewport resizes; far better than persisting raw pixel offsets (`anchor.rs:24-37`, `tile.rs:411-447`).
- **Single mutation entry point** for scroll state. Every scroll command goes through `ScrollManager::apply_intent` with two conversion closures (`manager.rs:87-155`). Easy to reason about, hard to bypass.
- **Layout cache** keyed on a comprehensive struct (`layout.rs:89-107`: revision, char_range, font, size, wrap width, selection, search highlights, dark mode). LRU with size budget (8 entries / 4 MB, `layout_cache.rs:29-36`). Old revisions are dropped via `retain_revision` on each build, so post-edit staleness is eliminated automatically.
- **Reveal pipeline is generation-resistant** — by the time the renderer applies the `Reveal` intent, the anchor is resolved fresh, so an edit between request and apply doesn't leave the viewport at a stale offset.
- **Edge-autoscroll on drag-select** is properly implemented for both axes, with two-zone velocity falloff, suppression of competing reveal intents, and storage on the manager (not as a single-shot intent) so it survives across frames cleanly (`tile/autoscroll.rs`).
- **Word boundaries use the piece tree directly** (`mouse.rs:142-144`, `cursor.rs:124-128`), not the visible galley — word-wise navigation/selection crosses the visible-slice boundary correctly.
- **Click-state lives in `egui::Memory`** keyed by response id (`mouse.rs:77`), so it scopes naturally per-tile and clears on remount.
- **Multi-click distance / time guards** (`mouse.rs:5-6`) match platform conventions.
- **Modifier handling is platform-agnostic** via egui's `command` flag (`cursor.rs:158-176`), which collapses Cmd-on-mac and Ctrl-on-Windows cleanly.
- **Tile is a thin coordinator.** Keyboard, mouse, painting, layout, and scroll are all one indirection deep — no monolithic editor function to plough through.

---

## 6. Speed Findings

Findings ordered roughly by expected impact.

### 6.1 No per-line layout cache (medium)

`layout.rs` re-lays the **entire visible slice** as a single galley on cache miss. Cache key includes `selection` and `search_highlights` (`layout.rs:89-107`), so any selection drag or search-highlight change misses for the whole slice. With a 50–100-row viewport this is typically fast enough, but a sticky drag-select across a large screen rebuilds the galley every frame. Per-line caching is hard with egui's opaque galleys; a partial mitigation is to split the cache key into "structure-affecting" (revision, font, wrap_width, dark mode) and "decoration-affecting" (selection, search highlights) and cache the structural galley once, applying decoration overlays as a separate pass.

### 6.2 Per-frame visible-slice text materialisation (low-to-medium)

`viewport_text_slice` in `layout.rs:28-32` calls into the snapshot's `borrow_or_flatten_range`. On a contiguous piece tree this is zero-copy; on a fragmented one it allocates a `String` of the visible slice every frame. Heavy editing creates fragmentation, so a long drag-select after many edits can allocate ~visible-bytes per frame. The piece tree already has a `rebalance` path; a small "if slice is fragmented, request a coalescing pass" hint would close this.

### 6.3 `apply_search_highlights` walks all views (low)

Called from `runtime.rs::apply_search_highlights`; iterates every view in the active tab to push highlights into `view.set_search_highlights_anchored`, even views whose buffer has no matches. With many views per tab this is wasted work. Mirror the search review's finding 4.6 (clear) — track which views currently hold highlights and update only those.

### 6.4 `DisplaySnapshot` rebuilt per-frame (low)

`scrolling/display.rs:from_galley_with_base_and_overlays` walks the galley's rows to extract `row_tops`, `row_logical_lines`, `row_char_ranges`, `row_records`. Linear in visible rows (~50–100), small Vecs each frame. Cheap, but unconditional. A smaller cached snapshot keyed alongside the galley itself would amortise this with no behavioural change.

### 6.5 Layout cache key cardinality is high (low)

The key includes the full `selection` range and `SearchHighlightState`. Any cursor twitch produces a fresh cache entry, churning the LRU. The 8-entry budget is small; once the user is dragging a selection, every frame is effectively a cache miss. See 6.1 for the recommended split.

### 6.6 `move_by_page_rows` walks one row at a time (low)

`cursor.rs:51-66` calls `galley.cursor_down_one_row` in a loop for `page_jump_rows`. On a viewport with 60 rows, that's 60 galley calls per PageDown. Egui's API doesn't expose a "go N rows" primitive, so this is the workaround — fine, but worth noting.

### 6.7 Edge-autoscroll polls `latest_pos` from `ui.input` repeatedly (cosmetic)

`mouse.rs:299-308` reads `pointer.button_down`, `pointer.latest_pos`, and `response.dragged_by` separately, each grabbing the input lock. Three reads per frame per tile — small but adds up across many panes.

---

## 7. Best-Practice / Code-Quality Findings

### 7.1 Two `user_scrolled` flags

`ScrollManager.user_scrolled` (`manager.rs:23-26`) is the application-level truth. `ScrollState.user_scrolled` (`state.rs:23-25`) is the egui-pixel-layer truth. Both genuinely exist; both are described as "set when the user manually scrolled, cleared on programmatic scrolls". They are kept in sync by the `apply_intent` paths and by `tile.rs::sync_editor_scroll_state`, but the duplication is an open invitation to drift. Consolidate into one — `ScrollManager` is the natural home — and have `ScrollState` derive its flag from there at sync time, or drop it entirely if no caller needs it.

### 7.2 Cursor navigation is constrained to the visible galley (significant)

`apply_cursor_movement` (`cursor.rs:68-109`) clamps the cursor's local index into `[0, slice_chars]` before consulting the galley. Word-wise horizontal jumps escape this bound by walking the piece tree directly, but **vertical (Up/Down) and Page (PgUp/PgDn) movement go through `galley.cursor_*` and so cannot move past the visible slice boundary**. In practice the cursor stops at the top/bottom row of the visible galley; the next key press triggers a reveal that scrolls one row, the slice rebuilds, and the next press moves another row.

This is most visible when holding ArrowDown at the bottom of the viewport: navigation is rate-limited by the per-frame slice rebuild rather than flowing freely as in most editors. Same for PageDown at the bottom of the viewport — instead of jumping 60 rows, it bottoms out at the current slice end and waits for the next frame.

The fix is to compute vertical/page targets against the **full document** (via the piece tree's line index), not the visible galley. This is the largest correctness/UX issue I found in the editor surface.

### 7.3 No IME preedit support

`relevant_input_event` (`interactions/keyboard.rs:147-167`) handles `Event::Text` and `Event::Ime(ImeEvent::Commit(...))` but ignores `ImeEvent::Preedit`, `ImeEvent::Disabled`, and `ImeEvent::Enabled`. Users typing with a CJK / dead-key IME see characters appear only after commit — the in-progress composition window is invisible inside the editor. For a code-focused scratchpad this may be acceptable, but it is worth flagging in the plan.

### 7.4 No multi-cursor / column selection

A single `CursorRange` is the only selection model (`types.rs:86-127`). No box-select, no multi-cursor add-next-occurrence, no column drag with Alt. Possibly intentional given the codebase's scope; worth pinning down as a non-goal in a plan doc if so.

### 7.5 `ScrollState` fields are all `pub`

`scrolling/state.rs:11-26` exposes every field publicly — `offset`, `pending_target`, `content_size`, `viewport_size`, `scrollbar_drag`, `user_scrolled`. The `ScrollManager` is correctly encapsulated; `ScrollState` is not. Outside callers can mutate `offset` without going through any clamp/sanitize path. Tighten with accessors, or document explicitly that `ScrollState` is intended as a passive POD.

### 7.6 No unit tests in the editor or scrolling modules

The search and history reviews found the same gap. Particularly important to test here:

- Cursor movement at slice boundaries (the 7.2 issue).
- Word-boundary detection across mixed scripts (ASCII, CJK, combining marks).
- `ScrollManager::apply_intent` for each variant — especially `Reveal` margin math.
- Piece-anchor recovery (`recover_unresolved_piece_anchor`) when content above is deleted.
- Edge-autoscroll velocity calculation across the two-zone falloff.
- Click-count normalisation (`normalize_click_count` end-of-row case).

### 7.7 `ViewId` and `EditorViewState` shadow each other in conversation

In code comments and signatures, "view" sometimes means the layout-leaf (`ViewId`) and sometimes means the editor's per-pane state (`EditorViewState`). The code is correct; the language is ambiguous. A type-rename (`PaneViewState` or `EditorPaneState`) or a documentation pass on the difference would help.

### 7.8 `tile.rs` is approaching too-big

638 lines, mixing scroll-anchor recovery, layout-resize tracking, intent draining, snapshot sync, header rendering, and tile-action collection. Most of the helpers are short, but the file as a whole does a lot. Splitting scroll-bridge work (`recover_unresolved_piece_anchor`, `editor_pixel_offset_resolved`, `sync_editor_scroll_state`, `drain_pending_scroll_intents`, `layout_resize_scroll_offset`, `upgrade_scroll_anchor_to_piece`) into a `tile/scroll_bridge.rs` would clarify the file's job.

### 7.9 `is_insertable_text` rejects `\n` and `\r`

`keyboard.rs:170-172` filters those out of the `Event::Text` path. Enter is handled via the `Key` event path, which is correct, but the test reads as defensive against a confused egui event ordering. Worth a comment explaining the intent so a future reader doesn't "fix" it.

### 7.10 `ScrollManager.replace_anchor` is a single-purpose escape hatch

Used for "upgrading from a v1 logical anchor to a piece-tree-backed one" (`manager.rs:65-68`). Public and bypasses `apply_intent`. The comment says exactly when to use it; consider gating with a typed parameter (e.g. `AnchorUpgradeReason`) so callers can't accidentally use it for unrelated purposes.

---

## 8. Utility & Functionality Findings

### 8.1 No smart-home (Home toggles indent ↔ column 0)

`row_edge_movement_target` (`cursor.rs:166-179`) sends Home to `cursor_begin_of_row` unconditionally. Many editors toggle: first Home jumps to the first non-whitespace character; second Home (still on that row) jumps to column 0. Cheap to add and very welcome for code editing.

### 8.2 PageUp / PageDown bounded by visible galley

See 7.2 — same root cause; user-visible as "PageDown only goes to bottom of viewport, not 60 rows past current".

### 8.3 No mouse middle-click paste (low)

X11/Linux convention; macOS and Windows convention is None. Worth flagging only if Linux is a target.

### 8.4 No "scroll past EOF" toggle (low)

`ScrollState::max_offset` (`state.rs:57-65`) supports an `eof_overscroll: bool` parameter, suggesting infrastructure is in place — but I didn't see a setting that exposes it. Surface it in settings if not already, or remove the parameter if it's permanent.

### 8.5 Drag-select autoscroll velocity is linear, not eased (cosmetic)

`autoscroll.rs:38-48` uses a linear ramp from edge to outside zone. Fine — predictable. An eased curve (`x²` or `smoothstep`) would feel a bit smoother but isn't a clear win.

### 8.6 No virtualised gutter / sticky line / minimap

Confirmed absent. For a code-focused editor these are conventional but not always essential. Document the position in the plan rather than leaving silent.

### 8.7 Cursor blink

I didn't find blink-rate handling in `painting.rs::paint_cursor_effects`; the cursor appears to be drawn unconditionally when focused. If blink is intended (accessibility / focus signal), it's missing; if not intended, a comment would clarify.

### 8.8 `Reveal` always re-centers when far off-screen

`ScrollIntent::Reveal { align_y, align_x }` accepts alignment. The search runtime sometimes sends `KeepHorizontalVisible`, sometimes `Center` (`search_state/runtime.rs:282-287`). For typing-induced reveals, conventional behaviour is to keep the cursor as close to its current visual position as possible (KeepVisible) and only re-centre on jumps. Worth confirming the search/typing alignments match user expectation.

### 8.9 No keyboard shortcut to swap pane focus

The pane tree supports splits but I didn't find shortcuts to move focus between panes (Ctrl+Tab style or vim-like Ctrl+W h/j/k/l). Mouse-only navigation between tiles is a usability gap once a user splits.

### 8.10 No selection-aware copy with rich format

`Event::Copy` is forwarded but I didn't trace whether it copies the raw text or runs through any preserve-formatting path. For a plain-text editor raw is correct; flagged only as a "verify" item.

---

## 9. Prioritised Improvement List

| # | Category | Item | Rough effort |
|---|----------|------|--------------|
| 1 | Correctness/UX | Cursor vertical/page navigation against the full document, not the visible galley (fixes "navigation rate-limited at viewport edge") | M |
| 2 | Best practice | Unit tests for cursor movement at slice boundaries, `apply_intent` variants, edge-autoscroll, anchor recovery, word boundaries | M |
| 3 | Speed | Split layout cache key into structural vs decoration; cache structural galley once per slice | M |
| 4 | Best practice | Consolidate `user_scrolled` to a single source of truth | S |
| 5 | UX | Smart-home (toggle indent ↔ column 0) | XS |
| 6 | Functionality | IME preedit support (Preedit/Enabled/Disabled events) | M |
| 7 | Best practice | Encapsulate `ScrollState` fields behind accessors; document the `replace_anchor` escape hatch | S |
| 8 | Best practice | Split `tile.rs` into `tile/scroll_bridge.rs` and the pure tile shell | S |
| 9 | Speed | Track which views hold search highlights; update only those | S |
| 10 | UX | Keyboard pane-focus swap (Ctrl+Tab or directional) | S |
| 11 | Speed | Fragmentation hint to coalesce piece-tree pieces when visible slice keeps allocating | S |
| 12 | UX | Audit `Reveal` align defaults — typing → KeepVisible, jumps → Center | XS |
| 13 | Functionality | Multi-cursor or column selection (or document as explicit non-goal) | L (or XS for the doc) |
| 14 | UX | Cursor blink (or document as intentionally absent) | XS |
| 15 | UX | Surface or remove the `eof_overscroll` toggle | XS |

XS = under an hour; S = under a day; M = multi-day; L = week+.

---

## 10. Quick-Wins Worth Doing First

Three independent items with high payoff and small surface:

1. **Fix vertical/page cursor navigation across the visible-slice boundary** (item 1). Today, holding ArrowDown or pressing PageDown at the bottom of the viewport advances by exactly one frame's worth of slice-rebuild rather than flowing freely. The fix lives entirely in `cursor.rs` and uses the piece tree's line index instead of the galley's row API. Single biggest UX improvement available.

2. **Smart-home** (item 5). One small addition to `row_edge_movement_target`: if the cursor is past the first non-whitespace character, Home goes there; otherwise to column 0. Five-minute change with disproportionate user-visible benefit on indented code.

3. **Unit tests for the scroll-anchor recovery and `apply_intent` cases** (item 2). These two behaviours are the foundation that everything else assumes works; they're untested today, and they're exactly the sort of code that breaks silently when the piece tree or egui API evolves. A few dozen lines of tests fix that.

These three are independent and preserve all existing public APIs.
