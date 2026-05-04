# egui Widget ID Pass-Stability Plan

## Symptom

The diagnostic dialog is flooded with `egui::context` `WARN` entries of the form:

```
Widget rect [[1738.7 1033.1] - [1756.7 1057.8]] changed id between passes:
prev ids: ["B663"], new ids: ["D59E"]
```

Hundreds of these per frame in long scrolled lists. Rect width is consistently
~18 px; rect y values span a tall range (e.g. −1527 to +1057) — characteristic
of a many-row scrolled list of icon-sized widgets.

## What this warning actually means

Source: `egui-0.34.1/src/context.rs::warn_if_rect_changes_id`
(`#[cfg(debug_assertions)]`, fires per frame).

For each layer egui builds two `BTreeMap<rect, [WidgetRect]>` snapshots — one
from the previous pass, one from the new pass — and warns when the rect appears
in both passes but with **different ids** AND:

1. None of the previous ids survive at the same rect in the new pass.
2. At least one previous id is gone from the layer entirely (so the widget
   wasn't merely shifted elsewhere).
3. At least one previous and new id share a `parent_id` (so the same logical
   container generated different child ids — not a global cascade).

Translation: **inside the same parent container, between pass 1 and pass 2 of
the same frame, child widgets at the same screen rect ended up with different
ids.** This is almost always one of three things:

- **A. Auto-id drift.** Calls like `ui.label`, `ui.add_sized`,
  `ui.allocate_exact_size`, untitled `Button` allocate auto-ids whose value
  derives from a per-`Ui` counter. If pass 2 inserts/removes/reorders one
  earlier widget in the same parent `Ui`, every later sibling's auto-id shifts.
  The widget at rect R is now a different sibling than it was in pass 1.
- **B. State that flips between passes.** A piece of `ctx.data` is *written*
  during pass 1 and *read back* during pass 2 (typical for click-toggled
  expand/collapse, tab switches, focus). The new value changes which children
  the parent emits, which shifts auto-ids.
- **C. Response-driven structural change.** `ctx.read_response(id)` in pass 2
  sees a response that did not exist in pass 1, and the code conditionally
  emits a different widget tree as a result.

Two-pass evaluation is normal in egui: any `ScrollArea`, `with_layer_id`,
`request_discard`, sized-content `Frame`, or first-frame layout discovery can
trigger it. The bug is *not* that egui runs twice; the bug is that our UI
produces a different child id at the same rect when it does.

## Existing infrastructure (do not duplicate)

- `src/app/ui/widget_ids.rs` — central id helpers (`feature_scope`, `scope`,
  `local`, `ctx_key`, `area_id`, `interact`, `track`). Forbids raw
  `make_persistent_id`/`push_id`/`Area::new`/`LayerId::new`/etc. in app code
  via the `app_code_uses_widget_id_wrappers_for_raw_egui_ids` test.
- `src/app/diagnostics.rs` — installs a `log` subscriber and a panic hook;
  forwards `egui::*` `WARN` records to `error.log` as JSON
  (`AppDiagnosticKind::EguiWarning`). Already captures the warnings we see.
- `widget_ids::track` already records `(id, rect, kind)` for our wrappers and
  feeds `DiagnosticsState::track_widget` for in-pass duplicate detection.

What is **missing** is attribution: the egui warning gives short hex ids
(e.g. `B663`), not call sites. We can decode them only if we have a per-frame
`{id → call site}` map, which we do not currently emit.

## Goals

1. Eliminate "rect changed id between passes" warnings that originate in our
   UI code.
2. Make egui's internal multi-pass behavior something we observe, not avoid;
   the goal is stable ids across passes, not single-pass UIs.
3. Add tooling so a future regression points at a file/line, not a hex hash.

Non-goals:

- Reducing pass count or disabling sized passes.
- Suppressing the warning (it is debug-only and useful).

## Plan

### Phase 1 — Attribution

Without source-line attribution we will be guessing for hours. Wire the
diagnostic just enough to translate egui's hex ids to our call sites.

1. **Capture call site at id creation.** Extend `widget_ids::track` (and
   `interact`) to optionally take `&'static std::panic::Location<'static>`
   via `#[track_caller]`. Store one entry per id per pass in
   `DiagnosticsState`: `id_full_hex → (rect, kind, location, parent_id_hex)`.
   Use `Id::short_debug_format()` so the hex matches what egui logs.

2. **Snapshot passes.** Wrap our root `update` callback so we
   call `diagnostics::begin_frame` once per frame (already exists) and
   `diagnostics::begin_pass` once per egui pass. The simplest hook is
   `egui::Context::on_begin_pass` (provided by egui 0.34) — register it from
   `configure_debug_options`. Maintain `prev_pass` and `current_pass`
   snapshots in `DiagnosticsState`.

3. **Annotate egui warnings.** In `DiagnosticsState::log_record`, when the
   message starts with `Widget rect ` and contains `changed id between passes`,
   parse out the prev/new short ids and attach the matching call sites from
   the snapshots into `AppDiagnostic::details`. The viewer already renders
   `details`. Now a single warning row tells us "rect R: prev was
   `text_history.rs:608 history_pill::frame_id`, new is
   `text_history.rs:514 file_header_pill::caret`" or similar.

4. **Acceptance for Phase 1.** Open the text history dialog with a buffer
   that has many rows; reproduce a warning; confirm `error.log` entries now
   carry `prev_site` / `new_site` strings pointing into our source.

### Phase 2 — Repro harness

A unit-style test that drives the warning deterministically, so we can fix
without manual reproduction.

1. **Headless context.** Build an `egui::Context` with `warn_on_id_clash =
   true` and a `log` capture sink that buffers `egui::context::WARN` records.
   Run two consecutive passes (`ctx.run` with the same input twice) on a
   constructed `text_history` dialog populated with synthetic rows.

2. **Assertion.** The captured warnings list must be empty after both passes.
   Initially this test is expected to fail; it gates Phase 3.

3. **Variations.** Add cases for:
   - timeline tab with mixed applied/undone rows that produce now-line anchors;
   - by-file tab with multiple groups, some expanded, some collapsed;
   - search results dialog with many groups, both expanded and collapsed;
   - a buffer-list panel if any other long scrolled list exists in app.

   Tests live in their respective dialog modules, not in `widget_ids.rs`.

### Phase 3 — Fix the call sites

These are concrete candidates surfaced by static read of code today
(`src/app/ui/dialogs/text_history.rs`, `src/app/ui/search_replace/results.rs`).
Phase 1 attribution will confirm or rule each out — do not "fix" speculatively.

**Candidate C1 — auto-id drift around the now-line.**
`render_timeline_rows` and `render_file_history_rows` conditionally emit
`render_now_line(ui)` between rows. `render_now_line` calls
`ui.allocate_exact_size(... Sense::hover())`, which consumes an auto-id slot
in the parent `Ui`. The neighboring rows wrap themselves in `widget_ids::scope`
so their *scope ids* are stable, but any other auto-id widgets in the same
parent `Ui` would shift. **Fix**: give the now-line an explicit id by wrapping
its allocate call in `widget_ids::interact(... "text_history.now_line")`, and
audit that it is the only auto-id allocator at that parent level.

**Candidate C2 — toggled expand state read mid-frame.**
`render_file_group` and `show_result_group` do:

```rust
let expanded = ui.data_mut(|d| d.get_persisted::<bool>(expansion_id)).unwrap_or(true);
// ... render header pill ...
if caret.clicked() { ui.data_mut(|d| d.insert_persisted(id, !expanded)); }
if !expanded { return; }
```

If pass 1 reads `expanded=false` and the click handler runs during pass 1 and
flips storage, pass 2 reads `expanded=true` and emits the indented rows. The
parent `Ui` now has a different child set at the same starting y → drift.
**Fix**: read the value once at the *start of the frame* (before any pass),
store it in a frame-local owned `Vec<bool>` keyed by buffer/group id, and
apply the toggle on the *next* frame via the same `write_pending` mechanism
already used for `text_history.active_tab`. That mechanism (lines 728-742 of
`text_history.rs`) is the established pattern; extend it to per-group expand
state and search-group expand state.

**Candidate C3 — `read_response`-driven hover style.**
`history_pill` reads its own previous response to decide the fill color.
This changes only paint output, not id structure, so it should be benign for
this warning. Verify with Phase 1 attribution; do not preemptively rewrite.

**Candidate C4 — unsalted `add_sized` / `Button` chains.**
In `show_group_pill` (`results.rs:130-197`) and `render_file_header_pill`
(`text_history.rs:485-553`), `ui.add_sized(.., Button::new(...))` is called
twice in sequence. Both rely on egui auto-ids. If anything earlier in that
horizontal `Ui` is conditional, both buttons drift. **Fix**: each
`Button::new(...)` becomes `Button::new(...).id(widget_ids::local(ui, key))`
(check egui 0.34 — actually `Button` uses `Sense` only and is not directly
id'd; instead wrap in `widget_ids::scope(ui, key, |ui| ui.add_sized(...))`).

**Candidate C5 — `truncated_label` with `Sense::click()`.**
`text_history.rs:529 truncated_label(... Sense::click())` adds a clickable
label with auto-id. If a sibling label above it is conditional, this one
drifts. **Fix**: same scope-wrapping pattern.

**Candidate C6 — search results group label.**
`show_group_pill` builds `group_response` from a Button using
`group_response = Some(response);` and falls back to `ui.label("")`. The
fallback is unreachable in practice but emits an extra auto-id widget if the
horizontal closure ever returned None. **Fix**: remove the fallback or make
it explicit.

For each candidate confirmed by Phase 1 attribution, the fix is one of:
- wrap with `widget_ids::scope` so child ids derive from a stable salt;
- replace `ui.allocate_exact_size` with `widget_ids::interact`;
- defer state writes to next frame via the existing `write_pending` helper.

### Phase 4 — Prevent regressions

Append to `widget_ids.rs`'s `FORBIDDEN_APP_PATTERNS` and to
`docs/widget_ids.txt` (the live policy doc):

- `ui.allocate_exact_size(` outside `widget_ids.rs` — must use
  `widget_ids::interact` instead, which threads through `track`.
- `ui.allocate_response(` (same reason).
- `ui.label(RichText::new(...).sense(Sense::click(...)))` patterns where the
  label is interactive without an explicit id wrapper. (Hard to express as a
  single substring; consider matching `Sense::click()` followed by
  `.add_sized(` or `.label(` within the same line.)
- `data.insert_persisted(` outside `widget_ids.rs` and `text_history.rs`'s
  `write_pending` — force callers to go through a same-frame-stable helper.

Also add a runtime check in debug builds: at end-of-frame, panic if
`DiagnosticsState` saw a "changed id between passes" warning during the run
of `cargo test --features ui-tests` (gate behind a feature so it doesn't
break interactive runs).

### Phase 5 — Backstop

Once `error.log` is clean for the dialogs in scope, add a CI script
(`scripts/ci.ps1` already exists and runs the test suite) that runs the
Phase 2 headless tests and parses their captured warnings. CI fails if the
list is non-empty.

## Out of scope

- Editor-area widget ids: no warnings observed from there in the screenshot.
  Revisit if Phase 1 attribution surfaces them.
- Tab strip and tab-overflow ids: same — already feature-scoped, only revisit
  with evidence.
- Egui upstream changes: the warning is upstream behaviour we want to keep.

## Order of operations

1. Phase 1 (attribution) — **must come first**; otherwise we are debugging
   blind.
2. Phase 2 (repro test for `text_history`) — confirms a deterministic case.
3. Phase 3 — fix the candidates that Phase 1 has named, in priority order
   driven by warning frequency in `error.log`.
4. Phases 4–5 — only after the live `error.log` for the two dialogs is clean.

## Risks

- **Hex-id parsing is fragile.** Egui's short_debug_format may change shape
  in future versions. Pin egui version explicitly in `Cargo.toml` (already
  pinned at 0.34.1) and write the parser to log unrecognised lines verbatim
  so we never silently lose warnings.
- **Two-pass timing differences.** `on_begin_pass` may fire before our
  feature scopes register their ids; verify the snapshot in Phase 1 is taken
  at end-of-pass, not begin.
- **Deferring state writes** with `write_pending` introduces a one-frame
  delay for click responses. The text-history tab switch already accepts this
  delay; verify the same is acceptable for expand/collapse interactions
  before applying the same fix in Phase 3 / C2.
