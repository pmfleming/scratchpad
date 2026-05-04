# Error Handling Expansion Plan

## Goal

Extend the new structured diagnostics path beyond egui-specific warnings so the app records basic, machine-readable failures for file I/O, session persistence, and other external boundaries that currently only surface as transient status text.

## Current Code Anchors

- `src/app/diagnostics.rs` initializes a per-session `error.log` under the session root, writes newline-delimited JSON, and currently records `session_started`, `egui_id_conflict`, `egui_warning`, and `panic` events.
- `src/app/app_state/startup_state.rs` points diagnostics at `session_store.root().join("error.log")`, so the error log already lives beside the app's persisted session artifacts.
- `src/app/services/session_store/mod.rs` and `src/app/services/store_io.rs` perform the highest-value persistent writes: `session.json`, `*.tmp` buffer snapshots, stale-file cleanup, and atomic replacement.
- `src/app/services/file_service.rs` is the main file I/O boundary for reading, decoding, writing, and renaming user files.
- `src/app/services/background_io.rs` and `src/app/app_state/background_io.rs` convert many background lane failures into plain strings and status messages, but they do not emit structured diagnostics.
- `src/app/services/file_controller/open.rs`, `src/app/services/file_controller/open_here.rs`, and `src/app/services/file_controller/save.rs` summarize open and save failures in the UI, but today they drop path-level detail once the operation completes.

## Current Gaps

- `AppDiagnosticKind::Io` exists in `src/app/diagnostics.rs`, but production code does not currently emit `io` events.
- Session restore and persist failures are visible to the user through `set_error_status(...)`, but the structured log does not capture which file, operation, or phase failed.
- File open failures are flattened to `Result<BufferState, String>` in `src/app/services/background_io.rs`, which loses structured context before the UI sees the error.
- File save, reload, reopen-with-encoding, and rename paths report human-readable errors, but the error log does not capture them for later inspection.
- Background I/O channel saturation and fallback paths (`BackgroundIoDispatcher::send`, `BackgroundIoFallback`) degrade to generic messages like "Background file loader unavailable." without recording why the lane failed.
- A missing bundled resource such as the user manual or a font load failure reaches the status bar, but not the structured diagnostics stream.

## Scope Priorities

### 1. Session Store And Atomic Persistence

Target files:

- `src/app/services/session_store/mod.rs`
- `src/app/services/store_io.rs`
- `src/app/app_state/background_io.rs`

Plan:

- Add public helper functions in `src/app/diagnostics.rs` for recording I/O failures with consistent fields: operation, path, source module, and message.
- Emit diagnostics around the session manifest write path, temp buffer snapshot writes, stale temp cleanup, manifest reads, manifest JSON parse failures, and version mismatches.
- Keep the existing status-bar behavior in `src/app/app_state/background_io.rs`, but pair it with structured logging so the UI remains unchanged while the log gains durable detail.

Why first:

- These writes are central to startup recovery and shutdown safety.
- The diagnostics file is already colocated with `session.json`, so the data will be easy to inspect together.

### 2. User File Open, Save, Reload, Rename, And Encoding Paths

Target files:

- `src/app/services/file_service.rs`
- `src/app/services/file_controller/open.rs`
- `src/app/services/file_controller/open_here.rs`
- `src/app/services/file_controller/save.rs`
- `src/app/services/background_io.rs`

Plan:

- Record structured diagnostics when `FileService::read_file`, `read_file_with_encoding`, `write_snapshot_with_format`, `write_file_with_format`, or `rename_path` fail.
- Attach operation context such as `open`, `open_here`, `reload`, `reopen_with_encoding`, `save`, `save_as`, or `rename`, along with the target path and the selected encoding when applicable.
- Move the first structured logging point as close to the real failure boundary as possible, before errors are flattened into plain strings.
- Preserve the current batch-summary UX, but include enough per-path detail in the log to explain which file failed and why.

Why second:

- These are the highest-frequency external operations after session persistence.
- `src/app/services/file_service.rs` is already the narrow choke point for disk and encoding boundaries.

### 3. Background I/O Infrastructure And Lane Failures

Target files:

- `src/app/services/background_io.rs`
- `src/app/app_state/background_io.rs`

Plan:

- Emit diagnostics when a background lane send fails because the sync channel is full or disconnected.
- Record whether the request was for path, session, or analysis work, and include the request kind and queue fallback path used.
- Replace the remaining generic fallback-only behavior with a pair of outputs: a user-facing status message and a structured diagnostic event.
- Review the `handle.join().expect("path load worker panicked")` path and convert it into a recoverable error path if practical; if not, ensure the panic hook output contains enough context to distinguish it from unrelated panics.

Why third:

- These failures are rarer, but when they happen they currently collapse into vague messages that are difficult to correlate with the originating request.

### 4. Bundled Resources And Other External Boundaries

Target files:

- `src/app/app_state/frame.rs`
- `src/app/app_state/workspace/lifecycle.rs`

Plan:

- Log structured warnings or I/O events when bundled editor fonts fail to apply or when the user manual path is missing.
- Keep severity low for these cases unless they block the current action.

Why fourth:

- These failures are already surfaced to the user, so they are useful but not the first place to expand the new diagnostics system.

## Diagnostics Shape Changes

The current `AppDiagnostic` payload already has `kind`, `message`, `source`, and `frame`. To support non-egui failures cleanly, extend it in a backward-compatible way instead of creating a separate file format.

Recommended additions:

- Optional `operation` field for verbs such as `session_persist`, `session_restore`, `open_file`, `save_file`, `reload_file`, `rename_path`, or `background_send`.
- Optional `path` field for disk targets.
- Optional `details` map or a small set of targeted optional fields for data such as encoding name, queue lane, error kind, or buffer id.

This keeps `error.log` as a single stream while making non-egui diagnostics queryable.

## Implementation Order

1. Add diagnostics helpers in `src/app/diagnostics.rs` for I/O and infrastructure failures.
2. Wire session store and atomic persistence paths first.
3. Wire file service and file controller read/write paths next.
4. Wire background queue and fallback failures.
5. Add low-severity resource-boundary logging for fonts and manual loading.

## Validation

- Add unit tests in `src/app/diagnostics.rs` for the new payload fields and append behavior.
- Add focused session-store tests that force manifest read and write failures and assert that the diagnostics log receives an `io` event.
- Add focused file-service or controller tests that trigger open and save failures against temporary paths and assert both status behavior and structured logging.
- Manually verify that `error.log` still stays append-only and newline-delimited JSON.
- Manually verify that startup, open, save, and restore continue to show the same user-facing status messages after logging is added.

## Expected Outcome

After this work, `error.log` should stop being mainly an egui-specific trace and become the app's primary lightweight incident log for real file and infrastructure failures, without changing the existing interactive UI behavior.
