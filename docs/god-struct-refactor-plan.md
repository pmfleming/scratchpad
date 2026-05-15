# ScratchpadApp State Refactor Plan

## Goal

Keep `ScratchpadApp` as a thin application shell and move cohesive state plus behavior behind smaller ownership boundaries. The refactor is not complete when fields are merely moved; it is complete when new code can depend on focused state APIs instead of reaching for `&mut ScratchpadApp`.

## Target Shape

`ScratchpadApp` remains the top-level eframe app and owns the tab manager plus app state. Its job is orchestration: frame lifecycle, command dispatch, and coordination between subsystems.

`ScratchpadAppState` should be a container of named subsystems, not a flat bag of unrelated fields:

- `StatusState` or `StatusCenter`: current status, capped history, status ids, severity/domain helpers, and read/query helpers.
- `DialogState`: persistent modal/dialog state, including encoding, text history, status history, tab rename, pending tab context menu, and startup restore conflicts.
- `ChromeState`: active surface, transition tracking, vertical tab auto-hide state, and deferred status bar visibility.
- `BackgroundIoState`: background I/O channel, request id allocation, and pending background actions.
- `FileWatchState`: watched directories and debounced file-watch rescans.

## Implementation Rules

1. Move behavior with fields. Each extracted state object should expose methods such as `begin_transition`, `is_active`, `open`, `close`, `has_pending_persist`, or `take_due_rescans`.
2. Prefer narrow dependencies in new code. A function that only reads status history should not need the entire app.
3. Keep compatibility wrappers temporarily when they reduce churn, but treat them as migration debt.
4. Do the migration in safe vertical slices: first group fields without behavior changes, then replace direct field access with methods, then remove compatibility wrappers.
5. Split runtime I/O concerns. Background work and file watching share frame timing, but they are different ownership domains.

## Suggested Order

1. Finish the `StatusState` boundary and add query helpers.
2. Extract `DialogState` and move dialog open/close state under it.
3. Extract `ChromeState` with transition and vertical tab-list helpers.
4. Split `RuntimeIoState` into `BackgroundIoState` and `FileWatchState`.
5. Reduce `&mut ScratchpadApp` call sites module by module, starting with UI dialogs and chrome/layout helpers.

## Completion Criteria

- `ScratchpadAppState` no longer has loose dialog, chrome, or background I/O fields.
- Direct mutation of pending actions, dialog flags, transition counters, and vertical tab hide deadlines is limited to their owning state objects.
- New features can use smaller APIs instead of adding another `impl ScratchpadApp` block.
- Tests compile after each slice, and behavior stays unchanged.
