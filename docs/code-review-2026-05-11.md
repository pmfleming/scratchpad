# Scratchpad code review — 2026-05-11

Scope: `src/` (~50K LoC, 222 files) and `crates/windows_file_watch/`.
Goal: Identify gaps in functionality, performance, interface, code quality, and
correctness for a "supercharged Notepad" use case.

The headline: this is a strong codebase for a single-author project of this
size — clean module separation, a thoughtful concurrency story, and unusually
high care around text format edge cases. The biggest opportunities are
(1) closing a few real correctness/durability bugs in the save path, (2) a
short list of feature gaps that users of a "Notepad replacement" expect, and
(3) reducing the size of `ScratchpadApp` and a handful of UI files.

Severity legend used below: **HIGH** (correctness or data-loss risk),
**MED** (user-visible bug, performance ceiling, or significant friction),
**LOW** (polish, code health, or low-impact gap).

---

## 1. Architecture and code health

### Strengths

- **Clear layering.** `domain/` (text model, panes, tabs, views) →
  `services/` (file IO, search, sessions, settings, background workers) →
  `app_state/` (mutable runtime state, frame loop) → `ui/` (rendering,
  dialogs, scrolling). Cross-layer references go top-down. This is rare for a
  hand-rolled UI app and pays off in readability.
- **Piece-tree document storage** ([piece_tree.rs](src/app/domain/buffer/piece_tree.rs))
  with leaf/internal/root tiers, prefix metric arrays, and ASCII fast-paths.
  Append-only `add` buffer plus original buffer is the right shape for a text
  editor with cheap snapshots.
- **Anchor system** ([view.rs](src/app/domain/view.rs)) for cursors, selection,
  search highlights, and scroll position so they survive edits above the
  viewport. The explicit `take_runtime_anchors_for_release` discipline is a
  good defense against the registry growing unbounded.
- **Three-lane bounded background IO**
  ([background_io/dispatcher.rs](src/app/services/background_io/dispatcher.rs))
  — separate `Path`, `Session`, `Analysis` lanes with independent queue depth
  caps, instrumented with capacity metrics, and pre-incremented to avoid
  underflow. Notably mature.
- **`forbid(unsafe_code)`** at [main.rs:1](src/main.rs:1); all `unsafe` is
  isolated in the [windows_file_watch crate](crates/windows_file_watch/src/lib.rs).
- **Atomic config/session writes** through
  [`store_io::write_atomic_with`](src/app/services/store_io.rs:10) — temp
  file + rename — but see HIGH issue #2.1 below.
- **Test discipline.** ~180 `#[test]` functions, including dedicated test
  files for the document, history, search API, scrolling manager, and text
  history dialog. The status-message tests in
  [app_state.rs:332](src/app/app_state.rs:332) actively guard against the
  pluralization regressions called out in #1.4.
- **Capacity metrics + memory budget**
  ([capacity_metrics.rs](src/app/capacity_metrics.rs),
  [memory_budget.rs](src/app/memory_budget.rs)) — built-in observability that
  the layout warmer respects.
- **Diagnostics IO error reporting** is consistent: every fs call routes
  through `diagnostics::record_io_error[_with_details]`. Easy to reason about
  what gets logged.

### MED — `ScratchpadApp` is a god struct

[`ScratchpadApp`](src/app/app_state.rs:82) holds 30+ fields covering session,
search, encoding dialog, status messages, file watcher, background IO,
selection state, layout transitions, vertical tab list state, etc. Most of
the file is `impl ScratchpadApp` with a wide assortment of methods. A natural
refactor target:

- `StatusCenter` (current_status, history, next_id, severity helpers).
- `EncodingDialogState`, `TextHistoryDialogState`,
  `StatusHistoryDialogState`, `TabRenameDialogState`.
- `ChromeTransitionTracker` (chrome_transition_frames_remaining,
  vertical_tab_list_open + hide_deadline, active_surface,
  pending_status_bar_visible).
- `BackgroundIoState` (tx, rx, next_id, pending_actions).

These groups already exist conceptually in the field naming; pulling them
out reduces the surface that every `pub(crate) fn` on the app sees.

### LOW — module fragmentation

A few modules exist only to re-export one item, e.g.
[`services/file_watch.rs`](src/app/services/file_watch.rs) is a single
`pub(crate) use` line, [`file_controller.rs`](src/app/services/file_controller.rs)
is just a unit struct + 5 module declarations. Inlining or co-locating these
with their callers would cut navigation cost.

### LOW — `expect("buffer location validated")` repetition

The pattern `find_buffer_location(...) → buffer_by_id_mut(...).expect(...)`
appears ~6 times in [save.rs](src/app/services/file_controller/save.rs). A
helper that takes a closure (`with_buffer_mut(app, buffer_id, |buffer| ...)`)
would consolidate the proof obligations.

---

## 2. Correctness and durability

### HIGH — `write_atomic_with` deletes target before rename

[store_io.rs:49](src/app/services/store_io.rs:49) does:

```
remove_file_if_exists(path)?;
fs::rename(&temp_path, path)
```

On Windows, `std::fs::rename` already calls `MoveFileExW` with
`MOVEFILE_REPLACE_EXISTING | MOVEFILE_COPY_ALLOWED`, so the explicit delete
is unnecessary **and harmful**: if `fs::rename` fails after the delete (path
locked by AV scan, permission flake, antivirus quarantine), the user's file
is gone. This affects every settings save and every session manifest write
— the most user-visible "I just want my notes back" path. Drop the
`remove_file_if_exists` call and rely on `fs::rename`'s atomic replace.

### HIGH — no `sync_all` before atomic rename

Same function: the temp file is written, `flush()`ed, then dropped, then
renamed. Without `file.sync_all()` (which fsyncs both data and metadata),
a power loss between rename and OS flush can leave a zero-byte file at the
destination path. Less critical than #2.1 because most desktop scenarios
don't lose power, but for a "trustworthy Notepad" positioning it matters.
Add `file.sync_all()?;` before `drop(file);`.

### MED — synchronous save on the UI thread

[`save_buffer_to_path`](src/app/services/file_controller/save.rs:322) calls
`FileService::write_snapshot_with_format` directly from the frame handler.
For a 100 MB file with non-UTF-8 encoding the encoder runs on the UI thread
and blocks frames. There is already a `Path` lane in the background IO
dispatcher; reads use it but writes don't. Plumb saves through it; keep the
status-bar dirty indicator until the write completes.

### MED — synchronous session restore on startup

[`session_store::load`](src/app/services/session_store/mod.rs:84) reads the
manifest and every per-buffer `*.txt` snapshot before the first frame paints
(see `restore::restore_buffer_content`). Sessions with many large dirty
buffers cause a visibly slow cold start. The window doesn't show until two
frames have been painted (`show_window_after_first_frame` in
[frame.rs:103](src/app/app_state/frame.rs:103)), so the user sees nothing
during this period. Either show a splash or move buffer-content reads to the
`Session` lane and stream tabs in.

### MED — `inspect_file_prefix` only checks first 4 KB for nulls

[file_service.rs:299](src/app/services/file_service.rs:299) declares a file
binary if any null byte appears in the first 4 KB (`is_probably_binary`).
Many real binary formats (zip, png, large pdfs) start with text-looking
headers and put NULs further in. Mid-stream the loader `read_document_with_encoding`
re-checks `if text.contains('\0')` per chunk, which is good — but only on
the **decoded** UTF-8, so e.g. a UTF-16-mis-detected binary may decode to
garbage rather than rejecting. Consider also a "looks like UTF-N but
contains C0 control runs" heuristic, or at least make the per-chunk null
check stricter (e.g. high ratio of unprintable bytes in the first MB).


### MED — pluralization in status messages

[restore.rs:34](src/app/services/session_store/restore.rs:34) emits
`"Session restore found {} disk conflicts and {} missing files."` and
`"Reloaded {n} clean files from disk during session restore."` literally. So
single-item messages read "1 disk conflicts" / "1 clean files". The
[app_state.rs tests](src/app/app_state.rs:344) already enforce that primary
status text avoids `"file(s)"` and `"tab(s)"` patterns, but this literal-`s`
formatting is a different bug class. Add a small `pluralize(n, "file")`
helper and use it through `status_history`, `restore`, save-all reporting.

### LOW — `paths_match` lowercases canonicalized paths

[lib.rs:23](src/app/mod.rs:23) does
`fs::canonicalize(...).to_lowercase()`. Correct on Windows / NTFS, but if
the project ever ships on Linux it will incorrectly merge `/notes/A.txt`
and `/notes/a.txt`. Either gate the lowercase on `cfg!(windows)` or accept
the limitation and put a doc-comment on `paths_match` saying so.

### LOW — `fs::canonicalize` inside `watched_parent_dir`

[file_watch.rs:99](src/app/app_state/file_watch.rs:99) canonicalizes every
buffer's parent on every directory-set sync. For workflows with many open
files in different network locations this can introduce per-frame stalls.
Cache the canonicalization, or only re-canonicalize when the path set
actually changes.

### LOW — `Drop for ScratchpadApp` swallows errors

[app_state.rs:163](src/app/app_state.rs:162) calls
`let _ = self.persist_session_now()`. If the final session save fails
(disk full at exit), the user has no signal. At minimum log via
`diagnostics::record_io_error`.

---

## 3. Performance

### MED — every encoded chunk forces a piece-tree recalc on file open

[file_service.rs:432](src/app/services/file_service.rs:432) calls
`document.insert_direct(end, text)` once per 32 KB decoded chunk. Each
insert into `PieceTreeLite` triggers leaf recalc and prefix-metric updates
(`recalculate` cascades up through internal node and root). For a 100 MB
file with ~3,200 chunks this is ~3,200 cascading recalcs. Two options:

1. Use `from_string` once for files small enough to materialize an
   intermediate `String`.
2. Batch inserts into a "build mode" that defers `recalculate` until done
   (the existing `build_root_from_pieces` already does this for the
   one-shot construction path).

The capacity probes under [`bin/`](src/bin/) likely already show this —
worth confirming.

### MED — `DisplayTextMap` builds full `Vec<usize>` maps per frame on the visible slice

[layout.rs:27](src/app/ui/editor_content/native_editor/layout.rs:27) keeps
two `Vec<usize>` maps (doc↔display) for the rendered slice. The layout
cache hits hide most of the cost, but on first paint after a font change,
window resize, or any `clear_editor_layout_caches` event (e.g. switching
editor font) every visible view rebuilds these. For wrap-on, viewports
that show ~10K characters of mixed control text, this is a few KB of
allocation per view per frame. Worth profiling whether a `SmallVec` or a
sparse representation pays off.

### MED — search lifetime ties to `SearchState::default()` thread

[`SearchState::default`](src/app/app_state/search_state.rs:67) spawns a
worker thread the moment the state is constructed. `ScratchpadApp::default`
is also called by tests (`Default::default()` in
`app_state.rs:128`). Each test that constructs an app leaks a search
worker. Negligible in production, but it noticeably slows large test
suites and complicates teardown. Lazy-spawn on first search, or expose an
explicit `shutdown` method.

### MED — `recheck_encoding_compliance` walks the whole piece tree synchronously

[buffer/state.rs:419](src/app/domain/buffer/state.rs:419) iterates every
span of the document. The "should I refresh" gate
(`encoding_compliance_stale`) avoids most work, but the first refresh after
a paste or a settings change still happens on the UI thread. The
`Analysis` lane handles `RefreshEncodingCompliance` for the async path —
make the synchronous path either route through it or only kick on idle.

### MED — `find_buffer_location` is O(tabs × buffers) and is called from many save paths

[save.rs:560](src/app/services/file_controller/save.rs:560) iterates every
tab and its buffers to resolve a `BufferId → (tab_index, path)`. Called
from auto-reload completion, encoding reopen, manual reload, conflict
resolution. In normal use this is fine (~tens of tabs, single buffers per
tab), but for a power user with 100+ tabs it adds up. A `HashMap<BufferId,
TabIndex>` maintained alongside `tab_manager` resolves this in O(1).

### LOW — pending background-action dedup is by linear scan

`has_pending_reload_for_buffer` and
`has_pending_reopen_with_encoding_for_buffer` walk
`app.pending_background_actions.values()` ([save.rs:409](src/app/services/file_controller/save.rs:409)).
Hash by `(BufferId, ActionKind)` if this becomes hot.

### LOW — `extract_text` allocates the full document for `borrow_range(0..len)`

[piece_tree.rs:392](src/app/domain/buffer/piece_tree.rs:392) has a fast
path "no edits → return `original.clone()`", but the moment any edit
happens, full-document `extract_range` allocates a fresh `String` of the
whole buffer. `previews_for_matches` ([line 323](src/app/domain/buffer/piece_tree.rs:323))
intentionally takes that path for the contiguous-text case, which is
correct given the current API, but document the trade-off so a future
change doesn't accidentally make full-document extraction routine.

### LOW — file watcher polls 50 ms when more than 64 directories are watched

[windows_file_watch/lib.rs:198](crates/windows_file_watch/src/lib.rs:198)
chunks `WaitForMultipleObjects` (Windows hard limit 64). For users with
dozens of files in many different folders this adds a 50 ms poll cycle
to file-change latency, but more importantly the loop is O(chunks × 50ms)
which can starve later chunks. For the target audience of "notes / logs"
this is unlikely to bite; flag as a future limit.

---

## 4. Functionality gaps for a "supercharged Notepad"


### MED — no drag-and-drop file open

Grepping for `dropped_files` / `on_drop_file` returns nothing. egui exposes
`ctx.input(|i| i.raw.dropped_files)`; wiring this through
`FileController::open_paths_async` would be straightforward and is a
strong "you can replace Notepad" signal.

### MED — no goto-line / goto-offset

No `goto_line` or `jump_to` symbol exists. Easy to add as an `AppCommand`
+ shortcut (`Ctrl+G`).


### MED — no multi-cursor or column/block selection

Search results: zero matches for `multi_cursor`, column selection, or
block selection. `CursorRange` is a single primary/secondary pair. For the
"copied terminal output" / "exported logs" use cases, multi-cursor and
column selection are heavily used. Implementing them is a non-trivial
change to `CursorRange` and the editing pipeline, but worth flagging as
the single biggest functional gap.


### LOW — no accessibility / screen-reader integration

`accessibility|a11y|screen_reader|aria` matches only `fonts.rs` (font
loading). egui has `accesskit` integration; enabling it would make
Scratchpad usable with NVDA / Narrator. Important for a notepad
replacement that defaults Windows users would pick up.

### LOW — no key-binding customization

`shortcuts.rs` is hard-coded. Settings has font/colors/tab placement but
no keybind editor. For power users this is a big "I'll keep using my
current editor" signal.

### LOW — file watcher is Windows-only

[`windows_file_watch`](crates/windows_file_watch/src/lib.rs:65) gates the
real watcher behind `#[cfg(windows)]`; non-Windows builds spin a no-op
thread. Acknowledged in the doc-comment; flagging here so it doesn't
silently bit-rot if anyone tries to run on macOS/Linux.

---

## 5. UX / interface

- **Window flicker avoidance.** Hidden until 2 frames painted then
  shown ([frame.rs:103](src/app/app_state/frame.rs:103)) — nice.
- **Custom decorations + caption buttons** — gives a polished feel, but
  watch out for HiDPI / multi-monitor regressions; egui's viewport
  primitives can drift between releases.
- **Status bar items are clickable** (encoding, history, control chars,
  settings), and the bar uses width budgeting
  ([status_bar.rs](src/app/ui/status_bar.rs)). Good. Consider also a
  cursor "Ln 12, Col 7" click-to-goto-line affordance.
- **Replacement preview** is rendered inline via
  [`paint_replacement_previews`](src/app/ui/editor_content/native_editor/painting.rs:295)
  — a strong UX touch.
- **Cursor reveal modes** (KeepVisible / KeepHorizontalVisible / Center)
  composed via `request_cursor_reveal` are well-thought-out. The "center
  beats keep-visible" precedence in
  [view.rs:222](src/app/domain/view.rs:222) is the right default.
- **No cursor blink configuration**, no cursor shape options. Probably
  fine for an MVP.
- **Status messages** — strong library of helpers (`report_save_failed`,
  `report_session_save_failed`, etc.) keeps message phrasing centralized.
  See pluralization issue in #2.5.
- **Encoding / line-ending visibility in the status bar** is a nice
  Notepad-plus differentiator. The accompanying "non-compliant character"
  badge is the kind of thing this product should keep leaning into.
- **No empty-state / welcome surface** — fresh launches drop you into an
  untitled buffer. A simple "Recent files / Open / New" panel when the
  workspace is empty would help discoverability of features that are
  buried in shortcuts.

---

## 6. Security

- `forbid(unsafe_code)` at the binary entry; all `unsafe` is in
  `windows_file_watch`. The unsafe blocks look correct
  (handle is checked against `INVALID_HANDLE_VALUE`, RAII drop releases
  the handle).
- Atomic write window (#2.1) is the closest thing to a security-relevant
  bug: the brief "no file at destination" window could be exploited to
  drop a substitute file, but only by a process that already has the
  user's filesystem rights, so the impact is limited.
- File-format detection rejects null-byte-containing prefixes early; no
  protocol/network handling; no scripting; no plugin execution. Threat
  surface is low.
- Settings/session are written to the OS temp directory. On multi-user
  Windows machines this is per-user, but worth confirming the path
  doesn't allow cross-user disclosure of in-progress notes. (Quick
  check: `std::env::temp_dir()` on Windows resolves to
  `%LOCALAPPDATA%\Temp`, which is per-user; OK.)

---

## 7. Suggested priority list

**Now / next iteration:**

1. Fix `write_atomic_with` to drop the explicit pre-rename delete and add
   `sync_all` (#2.1, #2.2).
2. Pluralize status messages from `restore.rs` and equivalent
   call-sites (#2.5).
3. Drag-and-drop file open (#4.1) — small change, large UX win.
4. Goto-line shortcut (#4.2).
5. Move file save to the existing background `Path` lane (#2.3).

**Soon (a few sprints out):**

6. Recent files MRU list — either implement or remove the toggle (#4.3).
8. Streamed session restore so the window paints while buffers load
   (#2.4).
9. Print / export to HTML (#4.5).
10. Refactor `ScratchpadApp` into focused subsystems (#1.2).

**Later:**

11. Multi-cursor / column selection (#4.6) — biggest functional gap, big
    change.
12. Accessibility via `accesskit` (#4.10).
13. Keybinding customization (#4.11).

---

## 8. What was not reviewed

This pass intentionally skipped `docs/`, `viewer/`, `scripts/`, `assets/`,
the `bin/` profiling probes (read for context only), and settings
serialization round-tripping. Tests were enumerated but not exercised; the
review trusts the existing assertions where they exist.
