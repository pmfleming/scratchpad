# History Callout Review

Review of the text-history callout (the "History" dialog) and its supporting state against best practice, speed, utility, and functionality. Code was not modified — this is an analysis report only.

Date: 2026-05-06

---

## 1. Scope & Method

In-scope code (~1,800 lines across 9 files):

| Area | Path |
| --- | --- |
| Dialog UI | `src/app/ui/dialogs/text_history.rs` |
| Row / group model | `src/app/ui/dialogs/text_history/model.rs` |
| Persisted preferences | `src/app/ui/dialogs/text_history/persistence.rs` |
| Dialog tests | `src/app/ui/dialogs/text_history/tests.rs` |
| Entry-view conversion | `src/app/text_history.rs` |
| Workspace state / actions | `src/app/app_state/workspace/mutation.rs` (`text_history_*` methods) |
| Open / close commands | `src/app/commands.rs` |
| Domain history layer | `src/app/domain/buffer/history.rs`, `history/budget.rs`, `history/coalescing.rs`, `document/history_ops.rs` (referenced, not the focus) |

Findings cite paths and line numbers from the current `master` (`a9b93a00`).

---

## 2. Architecture Summary

The history callout has a clean three-layer shape:

1. **Domain history** (`buffer/history*.rs`, `document/history_ops.rs`) owns the per-buffer operation log, replay, and budgeting.
2. **Adapter layer** (`text_history.rs`) builds `TextHistoryEntryView`s from `PieceHistoryEntry`s — denormalised UI-friendly snapshots with summary, source, undone flag, edit count, and the first-edit text pair used for the icon.
3. **Workspace facade** (`workspace/mutation.rs`) aggregates entries across all buffers, sorts by `(global_seq, buffer_id, id)`, caches the result keyed by per-buffer revision counters, and exposes apply/clear actions.
4. **Dialog UI** (`dialogs/text_history.rs` + `model.rs` + `persistence.rs`) renders two views (Timeline / By File), persists the active tab and "follow undo" toggle, and dispatches a single click action per frame.

The split is sensible. Critique below focuses on the parts that have grown awkward as the feature has expanded.

---

## 3. Strengths

- **Cache invalidation by content key.** `cached_text_history_entries` rebuilds only when the per-buffer `history_revision_counter` changes (`mutation.rs:108-114`). Idle frames don't re-walk the history.
- **Now-line semantics are tested.** `timeline_now_line_anchors` and `per_file_now_line_insert_index` have unit tests covering empty, all-applied, all-undone, and mixed states (`tests.rs:36-173`).
- **Deferred-persistence pattern.** `read_deferred_persisted` / `write_deferred_persisted` (`persistence.rs:5-32`) avoids the same-frame write-then-read trap; toggle changes settle on the next frame.
- **Replay safety.** `apply_text_history_entry_with_focus` checks direction match, the `replayable` flag, an open tab, and surfaces a typed status message for each failure mode (`mutation.rs:236-291`).
- **Per-buffer rewind by single click.** The "click an entry to undo/redo to that point in its buffer" semantics are documented inline (`mutation.rs:207-214`) and intentionally limited to one buffer — clear scope, no surprising cross-buffer rewinds.
- **Two views with shared row model.** Timeline and By-File reuse `TextHistoryRow` / `TextHistoryFileGroup`, so adding fields touches one place.
- **Sorted file groups by latest activity.** `file_groups_from_entries` orders groups by latest seq and falls back to label/buffer-id for stability (`model.rs:65-80`).
- **Aggregate budget enforcement.** `enforce_aggregate_text_history_budget` is called from edit finalisation (`mutation.rs:171`) so unbounded sessions can't blow memory.
- **`prune_text_history_for_buffers` hook exists** at the close-buffer call site (`workspace/lifecycle.rs:170`), even if currently a no-op.

---

## 4. Speed Findings

Findings ordered roughly by expected impact.

### 4.1 `cached_text_history_entries` clones the whole vector on every frame (medium)

`mutation.rs:108-115`:

```
self.text_history_cache.entries.clone()
```

Even when the cache is hit, every frame the dialog is open clones a `Vec<TextHistoryEntryView>` where each entry carries four owned `String`s (`label`, `summary`, `first_deleted_text`, `first_inserted_text`). The UI then immediately walks the clone twice — once to build `timeline_rows` (with another `String` clone per row inside `row_from_entry`) and once to build `file_groups` (yet another clone per row).

**Fix**: store the cache as `Arc<[TextHistoryEntryView]>` and hand callers an `Arc::clone`. The UI can then borrow rows for read-only walks without per-frame deep copies. Or — since the dialog currently only needs `timeline_rows` *or* `file_groups` depending on `active_tab` — build only the active view's projection from a borrowed slice and skip the unused projection entirely.

### 4.2 Both view projections built every frame regardless of which tab is shown (medium)

`text_history.rs:60-62`:

```
let timeline_rows = entries.iter().rev().map(row_from_entry).collect::<Vec<_>>();
let file_groups   = file_groups_from_entries(entries.iter());
```

The inactive tab's projection is computed and discarded. With many entries, `file_groups_from_entries` is the more expensive of the two (see 4.3); skipping it when `active_tab == Timeline` is a free win.

### 4.3 `file_groups_from_entries` is O(n·g), not O(n) (medium for large histories)

`model.rs:43-65`:

```
if let Some(group) = groups.iter_mut().find(|group| group.buffer_id == entry.buffer_id) { ... }
```

Linear scan of `groups` per entry. For an n-entry history across g buffers this is O(n·g). The search-result accumulator (`helpers.rs`) already shows the right pattern: a `HashMap<BufferId, usize>` index alongside the `Vec`. Add the same here.

### 4.4 `text_history.rs::entry_view` always materialises both deleted and inserted text (medium)

`text_history.rs:39-82` calls `first_text_pair`, which for `Replaced` entries concatenates *all* deleted spans into a `String` even though the result is only consulted to:

1. Pick an icon (`model.rs:84-99`) — only checks `is_empty()` of each side.
2. Populate `latest_text_history_inserted_text()` (`mutation.rs:90-96`) — used elsewhere for status messages.

For the icon decision, `is_empty` of the *first* deleted span and the inserted span is enough — no concatenation. For the latest-inserted-text accessor, the work only needs to happen for one entry, not all of them.

**Fix**: compute lazily, or store `(deleted_is_empty, inserted_is_empty, first_inserted_preview)` rather than the full strings. Drops a string allocation per entry on every cache rebuild.

### 4.5 Helper functions repeatedly call `text_history_entries()` (medium)

`mutation.rs:29-96` defines six accessors (`text_history_len`, `*_for_buffer`, `*_editor_len`, `*_search_replace_len`, `*_redo_len`, `latest_text_history_*`). Each calls `text_history_entries()`, which does a fresh allocation + walk + sort — *not* the cached path. A status-bar render that reads two of them in the same frame pays double.

**Fix**: route these through `cached_text_history_entries` (or a borrowing variant), or compute a small `TextHistorySummary { editor_len, search_replace_len, redo_len, latest_id }` once per cache miss and serve the accessors from it.

### 4.6 `apply_text_history_to_entry` rebuilds entries twice for one click (low-to-medium)

`mutation.rs:215-234` calls `text_history_entries()` to look up the target's `undone` flag, then delegates to `apply_text_history_entry_with_focus` which calls `text_history_entries()` *again* to find the same entry. Two full rebuilds per click. Worse — both rebuilds walk every buffer just to find one entry whose `buffer_id` is already known.

**Fix**: take `&BufferState` and call `entries_for_buffer` directly (single-buffer walk), or pass the already-resolved `target` view into `apply_text_history_entry_with_focus`.

### 4.7 `timeline_now_line_anchors` recomputed every frame (low)

`text_history.rs:324-338` allocates a `HashMap<BufferId, u64>` and a `HashSet<BufferId>` each frame. For typical buffer counts (< 30) a small `Vec<(BufferId, u64)>` linear scan would beat the map + hasher overhead. Or memoise on `entries`'s revision-counter tuple.

### 4.8 `text_history_revisions` allocates per frame (low)

`mutation.rs:117-125` builds a fresh `Vec<(BufferId, u64)>` every cache check just to compare. Reasonable for the small N typical in this app, but for a 1000-tab session it's needless. A fold-into-`u64` hash, or maintaining a single `u64` "history-stamp" incremented on any history mutation, would skip the allocation entirely.

### 4.9 Pill state read via `ctx.read_response` lags by a frame (cosmetic)

`text_history.rs:519-534`. `ctx.read_response(frame_id)` returns the previous frame's response, so the hover fill update lags one frame after the cursor enters/exits. Not a perf issue but visible. Allocate the response first, then style based on the *current* response.

---

## 5. Best-Practice / Code-Quality Findings

### 5.1 `text_history_open` is a `pub` field while `close_text_history` is a method

`commands.rs:75-82` exposes `pub text_history_open` (set directly via `app.text_history_open` inside the dialog) but the rest of the API uses methods. Search dialog hides `open` behind `search_open()`. Make these consistent — either both fields, or both methods. Easier to grep-audit invariants.

### 5.2 No Escape-to-close on the history dialog

The search controls explicitly consume `Escape` in `controls.rs:54-56` and `controls.rs:499-501`. The history dialog has no such handler, so users who reach for Escape have to mouse-click the close button. The `show_centered_callout` helper doesn't add one either.

**Fix**: a one-line `ui.input_mut(|i| i.consume_key(NONE, Escape))` near the top of `render_text_history_window`.

### 5.3 `TextHistoryWindowState` is a "borrow-bag" struct

`text_history.rs:47-53` carries five `&mut` fields purely to satisfy the borrow checker through the call chain. Splitting the dialog into "compute decisions" and "apply decisions" — the closure returns a `TextHistoryDecisions { next_tab, next_follow_focus, action, close_requested, clear_requested }`, the caller applies them — would remove the bag and simplify each render function's signature.

### 5.4 Persistence keys are scattered string literals

`persistence.rs:5-32` references four distinct keys (`text_history.active_tab`, `*.active_tab.pending`, `text_history.follow_undo`, `*.follow_undo.pending`). A typo on the read side silently resets to defaults. Lift them to `const`s in one place, or wrap each preference in a small typed handle (`PersistedTab`, `PersistedFollowFocus`) so callers can't construct the wrong key.

### 5.5 `tab_to_persisted` / `tab_from_persisted` is a hand-rolled u8 enum codec

`persistence.rs:34-46`. Adding a third `HistoryTab` variant requires remembering to update both halves. Either move the codec next to the enum (so it's hard to miss in a diff), or use `#[repr(u8)]` + `TryFrom<u8>` and let the compiler enforce exhaustiveness.

### 5.6 Triple `set_width / set_min_width / set_max_width` repeated four times

`text_history.rs:74-76`, `148-152`, `418-420`, `543-546`. A `force_width(ui, w)` helper would DRY these and prevent the three-call mistake (using only `set_width` is a common drift bug in egui code).

### 5.7 `prune_text_history_for_buffers` is a misnamed no-op

`mutation.rs:178-183`:

```
pub(crate) fn prune_text_history_for_buffers(
    &mut self,
    buffer_ids: impl IntoIterator<Item = BufferId>,
) {
    let _ = buffer_ids;
}
```

The cache invalidates correctly because closed buffers drop out of `text_history_revisions`, so functionality is fine. But the function reads as "this prunes" when it does nothing. Either delete it (callers will inline the no-op explicitly) or implement the prune so the cache and any persisted preferences get cleaned up immediately rather than on the next access.

### 5.8 No unit tests for the workspace facade

`tests.rs` covers row models, now-line placement, and follow-focus persistence — all of which are good. But the cache and apply paths in `mutation.rs` are untested:

- Cache hit / miss across revision-counter changes.
- `apply_text_history_to_entry` direction inference for applied vs undone entries.
- `text_history_len_for_buffer` after entries are added then undone.
- `clear_text_history` resets the cache.

These are the parts most likely to regress when domain history grows new edit kinds.

### 5.9 `cached_text_history_entries` requires `&mut self` even on cache hits

`mutation.rs:108-115`. Because the cache lives behind `&mut`, callers that "just want to render once" need a mutable receiver, which propagates up the call tree. A `RefCell` or `OnceCell<Arc<[…]>>` keyed by revision tuple lets this be `&self`.

### 5.10 `truncated_label` reused but with five differing call sites

`text_history.rs:587-601`. Fine as written, but four of the five callers pass the same `text_color`/`size` pair pulled from theme, with the fifth being the muted variant. A small `truncated_title(ui, text, width)` and `truncated_subtitle(ui, text, width)` would drop the per-callsite color/size args.

---

## 6. Utility & Functionality Findings

### 6.1 No keyboard navigation when the dialog is open (significant)

The dialog has no keyboard contract:

- No `Escape` to close (see 5.2).
- No `Up`/`Down`/`PageUp`/`PageDown` to traverse the list.
- No `Enter` / `Space` to apply the focused entry.
- No `Tab` cycling among the controls (timeline/by-file/follow/clear).
- No shortcut (e.g. `Ctrl+Shift+Z`, `Ctrl+Y` history) to open the dialog from anywhere; only the status-bar click in `status_bar.rs:251`.

This is the largest functionality gap — search has `Ctrl+F` / `Ctrl+H` and a full keyboard surface; history has none.

### 6.2 No filter / search within history (medium)

For long sessions the list grows unbounded. A small text filter (matched against `summary` and `label`) would be straightforward and very useful, given the dialog already has a "search by file" axis.

### 6.3 No grouping by time (low-to-medium)

Other history UIs group into Today / Yesterday / Earlier this week / Older. `PieceHistoryEntry` would need a timestamp field for this — currently the `detail` line shows only `<label> · <source>`. If timestamps already exist downstream, surface them; if not, this is a domain-level gap worth noting.

### 6.4 Clear-all has no confirmation (medium)

`text_history.rs:106` simply calls `app.clear_text_history()` on click. Replace-All in search has a "press again to confirm" pattern (`search-replace plan` §replace safety). Mirroring that for clear-all would close the trust gap without a modal.

### 6.5 "Follow undo" tooltip doesn't explain what it does (low)

`text_history.rs:196-208`. The tooltip is `"Follow undo is on"` / `"...off"`. A new user can't tell what *is* being followed. A more descriptive tooltip — "When on, applying a history entry moves the cursor to the affected text" — would help.

### 6.6 Empty states are bland (low)

`"No entries"`, `"No file history"` (`text_history.rs:271-273`, `352-354`). For someone who just cleared history, a hint like `"Edit a file to start tracking changes"` would be friendlier and clearer.

### 6.7 No "Jump to Now" affordance in long lists (low)

Once a user has scrolled into older entries, the only way back to the Now-line is to scroll manually. A small `Now` chip in the controls row (or pressing `Home` if keyboard nav existed) is conventional.

### 6.8 No persisted scroll position per dialog session (low)

Reopening the dialog returns to the top. For long histories users will repeat the same scroll. The deferred-persistence helper already exists; one more key would do it.

### 6.9 Click-to-rewind affects only the clicked entry's buffer (low — possibly intentional)

`mutation.rs:207-214` documents this clearly in code comments, but the *user* sees a global Timeline that visually mixes buffers and might reasonably expect "rewind the whole workspace to here". The tooltip says only `"Click to undo this text change"` (`text_history.rs:496-500`); calling out "in this file" would prevent confusion.

### 6.10 No way to delete or hide a single entry (low)

Only "Clear all". Reasonable for replay correctness — deleting a middle entry would invalidate everything later — but worth a tooltip on the trash button explaining the all-or-nothing semantics.

### 6.11 No multi-entry batch replay (low)

Click selects exactly one entry. Common workflows ("redo my last 5 changes in this file") still need 5 clicks. Per-buffer batch replay of contiguous entries already happens internally; a Shift-click to extend the range to a target would expose it.

### 6.12 Long file paths can still truncate in By-File headers (low)

`render_file_header_pill` reserves `(available_width - 96.0).max(120.0)` for the label (`text_history.rs:440`). For deeply nested project paths this truncates aggressively even with a 620 px dialog. Either reflow the count to a second line, or use the existing `truncate_dialog_title` middle-ellipsis style instead of plain truncation.

### 6.13 No visual distinction between "rewind to" and "redo to" entries beyond opacity dim

`UNDONE_OPACITY = 0.55` (`text_history.rs:32`) is the only signal. For accessibility (low-vision users, high-contrast themes) a small icon or stripe in addition to opacity would help. The existing icon column could carry a `↩`/`↪` glyph for direction.

### 6.14 No "undo this edit only" — clicks always rewind everything between Now and target

Documented but not always desirable. Some editors expose both "rewind to here" and "drop just this edit" (the latter is more dangerous since it changes ancestry). At minimum, surface the semantic in the tooltip.

---

## 7. Prioritised Improvement List

| # | Category | Item | Rough effort |
|---|----------|------|--------------|
| 1 | Functionality | Escape closes the dialog | XS |
| 2 | Functionality | Keyboard nav (Up/Down/PageUp/PageDown/Enter, optional global open shortcut) | M |
| 3 | Speed | `Arc<[TextHistoryEntryView]>` cache; remove per-frame clones | S |
| 4 | Speed | Build only the active view's projection (Timeline xor By-File) | XS |
| 5 | Speed | `HashMap<BufferId, usize>` index in `file_groups_from_entries` | XS |
| 6 | Functionality | Confirmation pattern for Clear All | S |
| 7 | Functionality | Filter input in the dialog | S |
| 8 | Speed | Route the `text_history_len*` accessors through the cache | S |
| 9 | Speed | Drop redundant `text_history_entries()` rebuild in `apply_text_history_to_entry` | XS |
| 10 | Best practice | Encapsulate `text_history_open` like `search_open()` | XS |
| 11 | Best practice | Centralise persistence keys; add `TryFrom<u8>` for `HistoryTab` | XS |
| 12 | Best practice | Tests for cache hit/miss and apply-direction inference | S |
| 13 | UX | Better empty states and "Follow undo" tooltip; "in this file" wording | XS |
| 14 | UX | "Jump to Now" affordance | S |
| 15 | Speed | Deferred or lazy first-text-pair extraction in `entry_view` | S |
| 16 | Functionality | Surface entry timestamps if domain supports it (or note as a domain gap) | M |
| 17 | UX | Direction icon on undone vs applied rows in addition to opacity | XS |
| 18 | Best practice | Implement or remove `prune_text_history_for_buffers` | XS |
| 19 | UX | Persist scroll position across opens | XS |

XS = under an hour; S = under a day for a familiar engineer; M = multi-day.

---

## 8. Quick-Wins Worth Doing First

Three small changes that together make the dialog feel notably more responsive and trustworthy without architectural impact:

1. **Escape closes it** (item 1). Single-line addition; closes a glaring keyboard-contract gap shared with no other dialog.
2. **`Arc<[…]>` cache + active-projection-only build** (items 3 + 4 together). Removes the largest per-frame allocation and eliminates wasted projection work for the inactive tab. No public API changes.
3. **Clear-all confirmation, mirroring Replace-All** (item 6). Prevents an irreversible accident; reuses an existing UX pattern. No new dependencies.

These three are independent of each other and of any architectural changes.
