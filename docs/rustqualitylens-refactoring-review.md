# RustQualityLens review and refactoring

Baseline: `9cd8d9b`. This is a focused, behavior-preserving first pass, not a claim that all project risks are resolved.

## Evidence and scope

Used the locally built `../rust-quality-lens/target/debug/rqlens` (0.1.0), with the existing `src` source scope. Ran `hotspots`, `clones`, `escape-hatches`, `reliability`, `locality`, and `leverage` before and after the changes.

Local artifacts and validation logs are under `target/quality-review/`:

- `baseline/`: measurements before editing.
- `current/`: measurements of the final Rust sources.
- `rqlens.toml`: isolated configuration; the project's normal analysis directory and policy were not changed.
- `clippy.log`, `tests-all.log`: final compiler/test results.

To repeat the measurements in this checkout:

```sh
RQL=../rust-quality-lens/target/debug/rqlens
for metric in hotspots clones escape-hatches reliability locality leverage; do
    "$RQL" measure "$metric" --config target/quality-review/rqlens.toml
done
```

**Limits:** reports are partial because macros are unexpanded and the bounded rust-analyzer run cannot resolve every dependency reference. Baseline resolved 2,517/5,303 references; final resolved 2,186/5,324. Architecture scores are advisory and not a complete, strictly comparable dependency graph. This version does not emit Halstead effort; no numeric effort reduction is claimed. Reduced branching, repeated code and state plumbing are the maintainability improvements instead.

The scope includes `src` tests and profiling binaries, but not `tests/`, `benches/`, or local dependency crates such as patched `crates/eframe`. The latter contains shipping code and needs a separate review; this is not a whole-dependency safety audit.

## Findings addressed

### 1. Piece-tree lookup duplicated an invariant at three tree levels

`src/app/domain/buffer/piece_tree/support/lookup.rs` had six helpers implementing the same two fast paths for nodes, leaves and pieces. Its main lookup function was the highest scored function in the baseline (119.73).

Moved the shared logic onto `LineLookupCursor::advance_over`, using existing `PieceTreeMetrics`. This removes five helpers and reduces nesting without changing scan boundaries, sampling, UTF-8 handling or introducing allocations. The strict boundary comparison remains important: a span ending on the target line still needs scanning to find the line start.

Extended the randomized edit test to check every line against a string model. Added a multi-node, long-line, Unicode and line-sampling regression test.

### 2. Display actions duplicated buffer and view ownership rules

`src/app/shortcuts/utility.rs` and `src/app/ui/editor_area/tile/context_menu/unicode_menu.rs` independently toggled buffer flags, invalidated view caches and marked sessions dirty.

Both now call shared operations in `src/app/app_state/workspace/editing.rs`. The menu takes one buffer lookup rather than three and separates its insert submenu from display controls. Cache invalidation and session dirtiness have one implementation and a nearby multi-buffer/multi-view test. Cut, delete, insert and undo/redo also share their mutation/search finalization sequence.

This deliberately increases complexity in the owning workspace module while removing more complexity from its callers. For the three modules together, cognitive complexity falls **70 → 45**, cyclomatic **86 → 77**. It is not just movement into unmeasured helpers.

### 3. Editor state carried impossible alternatives

The editor's `request_editor_focus` output was always `false`; it was threaded through two result types and a three-way tile-focus decision. Removed that field, the unreachable request path and its obsolete decision test. Existing requested focus is still consumed after rendering.

`store_latest_snapshot` and `consume_cursor_reveal` each received a constant `false` argument. Removed those impossible branches, narrowed snapshot storage to a plain `&Galley` and a required revision, and removed a redundant revision read. Added an edited-frame test for current snapshot publication and reveal consumption, and extended selection tests to cover unfocused views.

Affected modules: `src/app/ui/editor_area/tile.rs`, `tile/decisions.rs`, and `src/app/ui/editor_content/{mod.rs,native_editor/{mod.rs,painting.rs,tests.rs}}`.

### 4. Header controls computed and cloned unused inputs

`TileControl::show` ignored its supplied ID, using the existing rectangle-derived ID instead. Removed the unused parameter, caller ID construction, ID-prefix fields and pane-path clone. Actual widget IDs are unchanged. Tooltip attachment now consumes the response rather than cloning it. The white-on-black close control and red hover behavior remain unchanged.

Also borrowed the active file path for copy/reveal instead of cloning a `PathBuf`, borrowed the active selection for painting, and compared the next selection directly instead of cloning the old range. Range clones are cheap; this is ownership simplification, not a measured performance claim.

Replaced two test glob imports with explicit imports. Fixed the pre-existing strict-Clippy single-variant wildcard warning in `src/app/ui/tile_header/control.rs` rather than suppressing it.

## Measured results

Totals below include all measured `src` code and tests; complexity sums use module rows only to avoid counting functions twice.

| Metric | Before | After |
| --- | ---: | ---: |
| Cognitive complexity sum | 4,665 | 4,616 |
| Cyclomatic complexity sum | 9,322 | 9,296 |
| RQLens nonblank source lines | 67,334 | 67,247 |
| Physical Rust lines under `src` | 75,236 | 75,154 |
| Clone records | 379 | 368 |
| AST clone records | 51 | 48 |
| Duplicated lines | 5,242 | 5,133 |
| Duplication percentage | 7.79% | 7.63% |
| Counted escape-hatch occurrences | 13 | 11 |

The escape-hatch reduction is **two test glob imports**, not elimination of unsafe runtime behavior. Runtime safety boundaries were not weakened to obtain a lower count. Reliability records rise 328 → 334 solely due to six test-fixture `unwrap`s in the added workspace test; no production panic paths were added.

Selected module results:

| Module | Cognitive sum | Cyclomatic sum | Hotspot score |
| --- | ---: | ---: | ---: |
| Piece-tree `support/lookup.rs` | 57 → 34 | 43 → 27 | 108.08 → 72.86 |
| Unicode context menu | 35 → 10 | 23 → 13 | 76.33 → 41.24 |

### Locality/leverage tradeoffs

The concrete improvement is one owner for buffer-display state and its invalidation rules, local regression tests, and a single edit-finalization invariant. The workspace editing module's observed leverage score moves **0 → 10**, pressure **65 → 62.5**, with locality risk unchanged at 18. Conversely, the utility shortcut module gains an explicit workspace-editing dependency, raising observed locality risk **18 → 21** and pressure **65 → 68**. This is a conscious tradeoff for eliminating duplicate mutation logic, not an across-the-board numeric architecture win. Partial semantic resolution prevents stronger claims.

## Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --all-features`: **536 passed**, including 483 library tests; no failures or ignored tests.
- `git diff --check`: passed.

No GUI interaction, Windows build, coverage run or performance benchmark was performed. The Linux headless tests cover the changed editor, buffer and split-control behavior.

## Remaining priorities and intentional exclusions

- **High-coupling runtime workflows:** `app_state/background_io.rs`, `services/file_controller/open.rs`, `open_here.rs` and startup state remain the main locality/leverage review candidates. Separate request/result ownership and retain streaming/cancellation behavior before refactoring these larger workflows.
- **Remaining behavioral hotspots:** editor input processing, contiguous selection painting, file decoding, and history application need focused behavioral tests before another simplification pass.
- **Shortcut dispatch tables:** large exhaustive matches drive high cyclomatic counts, particularly `ShortcutAction::config_key` (44). Retained because exhaustiveness is useful; replacing them with indexed tables solely to lower a metric could weaken correctness.
- **Retained escape hatches:** the safe `PieceTreeText` deref wrapper, documented editor-frame length suppression, and platform fallback dead-code allowance have concrete purposes. Replacing them mechanically would add churn or obscure platform behavior.
- **Non-shipping work:** `src/bin/capacity_probe.rs`, `src/bin/resource_probe*`, and profiling workflows remain in the measured totals but were not refactor targets. In particular, the resource probe allocator's unsafe hooks are instrumentation, not the shipped editor allocator. No fixture/probe consolidation was undertaken just to improve clone counts.
- **Effort and architecture evidence:** obtain a complete semantic graph and a separately defined effort metric before asserting project-wide improvement in those numeric measures.
