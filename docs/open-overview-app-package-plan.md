# Open Overview App Package Plan

## Goal

Extend the overview stack so `scripts/open-overview.ps1` can open a viewer for the JSON package the app writes at runtime, instead of only serving the repo-local analysis artifacts under `target/analysis`.

## Current Code Anchors

- `src/app/services/session_store/mod.rs` persists the session package under `std::env::temp_dir().join("scratchpad")` and writes `session.json` plus `*.tmp` buffer snapshots.
- `src/app/services/session_store/model.rs` defines the JSON manifest shape: session version, active tab index, tabs, buffers, views, pane tree, encoding data, disk metadata, and text history.
- `src/app/services/session_store/ops.rs` establishes that persisted buffer snapshots use the `.tmp` extension.
- `src/app/diagnostics.rs` writes `error.log` into that same session root, and each line is JSON.
- `scripts/open-overview.ps1` currently refreshes repo-local analysis artifacts and launches `scripts/dashboard_server.py`, then opens `viewer/index.html`.
- `scripts/dashboard_server.py` currently exposes run APIs and serves static files, but it does not expose session-root artifacts outside the repository.
- `viewer/index.html` and `viewer/data-viewer.js` are hard-wired to `target/analysis/*.json` and have no app-package view.

## What The Viewer Should Cover

The app package viewer should focus on the persisted session root, not just one file.

Primary artifacts:

- `session.json` as the manifest and entry point.
- `*.tmp` buffer snapshot files referenced by `temp_id`.
- `error.log` as the adjacent structured diagnostics stream.

Minimum viewer outcomes:

- Show whether the session package exists.
- Show manifest version and top-level counts for tabs, buffers, views, and dirty buffers.
- Show buffer-level metadata from `session.json`, including path, temp id, encoding, BOM state, disk metadata, and whether the buffer is a settings file.
- Show the pane and view topology at a summary level.
- Show recent diagnostics from `error.log`, grouped or filterable by kind.

## Required Architecture Change

The current viewer loads files directly from paths under `target/analysis`. That model will not work for the app package because the session root lives under the OS temp directory, outside the repo.

That means the new feature should be server-backed, not static-file-only.

## Plan

### 1. Add A Server-Side App Package Loader

Target files:

- `scripts/dashboard_server.py`
- `src/app/services/session_store/mod.rs`
- `src/app/diagnostics.rs`

Plan:

- Mirror the app's session-root convention in the server by resolving `Path(tempfile.gettempdir()) / "scratchpad"` on the Python side.
- Read `session.json` if present.
- Read and parse `error.log` as newline-delimited JSON, skipping malformed lines defensively.
- Enumerate `*.tmp` snapshot files in the session root and join them with the `temp_id` references from the manifest.
- Return one normalized payload, for example from a new `/api/app-package` endpoint.

Suggested payload sections:

- `session_root`
- `manifest`
- `manifest_summary`
- `buffers`
- `buffer_files`
- `diagnostics`
- `warnings`

### 2. Keep Missing Or Partial Artifacts Non-Fatal

Target file:

- `scripts/dashboard_server.py`

Plan:

- If `session.json` is missing, return a valid payload with `exists: false` and an explanatory message.
- If `error.log` is missing, return an empty diagnostics list.
- If a `temp_id` listed in the manifest has no matching `.tmp` file, keep the manifest row and mark the snapshot as missing.
- If `error.log` contains malformed lines, keep the valid ones and report a parse warning instead of failing the whole endpoint.

This is important because the app package is live state, not a curated analysis artifact set.

### 3. Add An App Package View To The Browser UI

Target files:

- `viewer/index.html`
- `viewer/data-viewer.js`
- `viewer/styles.css`

Plan:

- Add a new top-level tab such as `App Package`.
- Load the new server payload through `/api/app-package` rather than through `../target/analysis/...`.
- Render summary cards for manifest version, tab count, buffer count, dirty buffer count, diagnostics count, and missing snapshot count.
- Render a manifest table for buffers and views.
- Render a diagnostics table or feed sourced from `error.log` with filters for kind and text.
- Render a lightweight session topology summary derived from `SessionPaneNode`, without trying to fully recreate the editor UI.

Recommended first slice:

- Summary cards
- Buffer table
- Diagnostics table

Leave richer topology visualization as a follow-up once the data plumbing is stable.

### 4. Update `open-overview.ps1` To Advertise The New Data Source

Target file:

- `scripts/open-overview.ps1`

Plan:

- Keep the current refresh flow for `target/analysis` unchanged.
- Add a switch or query parameter that opens the viewer directly to the new app-package tab.
- Print the resolved session-root path before opening the browser so it is obvious which package the viewer will inspect.
- Do not add analysis refresh tasks for the app package because the app itself owns that data.

Reasoning:

- The app package is runtime state, not a derived report that `open-overview.ps1` should regenerate.
- The script's job is to expose the viewer and point it at the correct source.

### 5. Decide How Much Snapshot Content To Expose

Target files:

- `scripts/dashboard_server.py`
- `viewer/data-viewer.js`

Plan:

- Start by exposing metadata for `.tmp` files: filename, size, modified time, and whether the file exists.
- Only add preview text if needed, and cap preview size aggressively.
- Avoid loading all snapshot text into the initial payload if the session becomes large.

This keeps the first version cheap and avoids turning the overview into a second editor.

## Proposed Endpoint Contract

Suggested initial endpoint:

- `GET /api/app-package`

Suggested optional follow-up endpoint if previews are needed later:

- `GET /api/app-package/buffer/<temp_id>`

The first version should avoid the per-buffer endpoint unless the summary view proves insufficient.

## Validation

- Run the app so it writes `session.json` and `error.log` into the temp session root.
- Launch `scripts/open-overview.ps1` and verify the new tab loads without requiring any `target/analysis` refresh.
- Verify missing-session, missing-log, and partially-corrupt-log cases render explanatory empty states instead of HTTP 500s.
- Verify the manifest counts shown in the viewer match the contents of `session.json`.
- Verify the diagnostics table shows the same event kinds that `src/app/diagnostics.rs` emits.

## Expected Outcome

After this work, the overview stack will cover both sides of the project:

- repo-generated analysis artifacts under `target/analysis`
- app-generated runtime package artifacts under the Scratchpad session root

That will make `open-overview.ps1` useful not just for code-health reports, but also for inspecting the app's persisted runtime state and its structured diagnostics output.
