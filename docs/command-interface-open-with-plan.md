# Command Interface And Windows Open With Plan

This plan is based on the current Scratchpad architecture in `src/main.rs`, `src/app/app_state.rs`, `src/app/commands.rs`, `src/app/shortcuts.rs`, and `src/app/services/file_controller.rs`.

This document is now scoped to:

- Windows Explorer `Open with`
- direct command-line invocation
- startup file-routing behavior
- command-line switches for controlling how incoming files are opened

The command palette work has been moved into its own plan.

## Current Review

### What Already Exists

- Scratchpad already has a solid internal action layer via `AppCommand` in `src/app/commands.rs`.
- File opening logic is centralized in `FileController`, including duplicate-path detection, batch open, existing-tab activation, and `Open Here` behavior.
- The app has top-level state in `ScratchpadApp`, which is the right place to host startup-open requests and startup-open policy.
- Session restore already runs during app initialization, so startup requests can be layered on top after restore completes.

### Gaps Identified

1. There is no startup argument parsing yet.
   `src/main.rs` does not read `std::env::args_os()`, so Explorer and direct shell invocation cannot pass files or switches into the app.

2. There is no defined command-line grammar.
   The app has no parsing rules for switches, no notion of open mode, no validation, and no user-facing error strategy for invalid combinations.

3. File-open orchestration is reusable, but not yet exposed as a startup-facing interface.
   `FileController` can already open multiple paths and route them correctly, but it currently expects UI-driven entrypoints.

4. Explorer integration is only partially an app problem.
   Even once startup argument handling exists, Windows still needs a registration strategy so `Open with` launches Scratchpad with the expected arguments.

## Goals

- Support command-line invocation of Scratchpad with files and switches.
- Support Windows `Open with` on one or more files.
- Add explicit startup-open switches such as `/clean` and `/addto`.
- Support quoted, comma-delimited file lists when callers choose to pass files that way.
- Reuse the existing file open and workspace routing logic instead of creating a second file-open path.
- Keep duplicate-path activation and session restore behavior predictable.

## Non-Goals For The First Pass

- Command palette UI or searchable in-app action palette.
- Single-instance IPC so an already-running process receives new file-open requests.
- Full installer and release packaging pipeline.
- A large Unix-style CLI with dozens of editor automation verbs.

## Proposed CLI Model

## 1. Support Both Standard File Arguments And Explicit File Lists

Scratchpad should accept files in two ways:

1. Standard positional arguments
   Example:

```text
scratchpad.exe "C:\notes\a.txt" "C:\notes\b.txt"
```

2. Explicit comma-delimited file list inside one quoted argument
   Example:

```text
scratchpad.exe /files:"C:\notes\a.txt","C:\notes\b.txt"
```

Why both should be supported:

- Explorer and normal shell launches naturally provide repeated file arguments.
- Some custom launchers, scripts, or registry commands may prefer one explicit `/files:` payload.
- Supporting both reduces friction and avoids making shell registration more brittle than necessary.

Recommended parsing rule:

- Positional file arguments remain the primary and simplest input path.
- `/files:` is an optional explicit override for callers that need a single-argument list.
- If both are present, combine them in the order received.

## 2. Define Startup Open Modes

Add a small startup-open policy layer.

Suggested shape:

```rust
enum StartupOpenMode {
    Default,
    Clean,
    AddTo,
}
```

Behavior:

- `Default`: restore session first, then open incoming files using the normal top-level open behavior.
- `Clean`: ignore restored tabs for this launch and start from one clean new tab before processing incoming files.
- `AddTo`: add incoming files into one specific existing or restored tab instead of opening them as separate top-level tabs.

## 3. Add The Requested Switches

### `/clean`

Requested behavior:

- "opens with a single new tab"

Recommended interpretation:

- Do not restore the previous session into the working UI for this launch.
- Start with exactly one fresh untitled workspace tab.
- Open incoming files after that clean start.

Important edge case:

- If `/clean` is supplied with no files, launch a blank window with one untitled tab.

### `/addto`

Requested behavior:

- "add all the files to a specific tab"

This needs a target selector, so `/addto` should not be a bare boolean switch.

Recommended syntax:

```text
/addto:active
/addto:index:2
/addto:name:"notes"
```

Recommended first-pass support:

- `/addto:active`
- `/addto:index:N`

Defer `/addto:name:` unless there is a strong use case, because tab titles are not guaranteed unique.

Behavior:

- Restore the session first unless `/clean` is also present.
- Resolve the target workspace tab.
- Add all incoming files into that tab using the same layout-building path as `Open Here`.

Important rule:

- `/addto` should be invalid together with `/clean` unless you explicitly define what tab is being targeted after the clean launch.

Recommended compatibility rule:

- Allow `/clean /addto:active` and interpret it as "create one clean tab, then add all incoming files into that new active tab."
- Reject `/clean /addto:index:N` because there is no restored tab set to index into.

### `/files:`

Recommended new switch:

```text
/files:"C:\a.txt","C:\b.txt"
```

Behavior:

- Parse one quoted, comma-delimited list of full file paths.
- Trim surrounding whitespace around each item.
- Preserve commas inside paths only if escaped support is intentionally added. For the first pass, document that commas in file names are not supported inside `/files:`.

Why this switch is relevant:

- It satisfies the explicit requirement for comma-delimited quoted file lists.
- It gives registry scripts and automation a stable one-argument file-list mode.

## 4. Add Other Relevant Switches

The following switches are useful and low-risk in the same design:

### `/here`

Behavior:

- Open incoming files into the current active workspace tab using the same behavior as `Open Here`.

Why it belongs:

- It maps directly to an existing app capability.
- It is simpler than `/addto:index:N` for common scripting usage.

### `/line:N`

Behavior:

- After opening one file, move the caret to line `N`.

Why it belongs:

- It is a common editor launch behavior.
- It is useful for future integration from scripts or tooling.

Why it may be deferred:

- It requires explicit cursor placement support in the view model rather than only opening files.

### `/help`

Behavior:

- Print usage information to stdout or show a message box in release builds if no console is attached.

Why it belongs:

- Once switches exist, usage output becomes necessary.

### `/version`

Behavior:

- Print application version and exit.

Why it belongs:

- It is cheap to add and useful for scripts.

### `/log-cli`

Behavior:

- Write parsed startup arguments and startup-open decisions to the existing runtime log.

Why it belongs:

- Startup parsing bugs are usually easiest to diagnose with explicit logging.

## 5. Recommended Parsing Rules

### Accepted Forms

- `scratchpad.exe`
- `scratchpad.exe "C:\a.txt"`
- `scratchpad.exe "C:\a.txt" "C:\b.txt"`
- `scratchpad.exe /clean`
- `scratchpad.exe /clean "C:\a.txt"`
- `scratchpad.exe /addto:active "C:\a.txt" "C:\b.txt"`
- `scratchpad.exe /files:"C:\a.txt","C:\b.txt"`
- `scratchpad.exe /here /files:"C:\a.txt","C:\b.txt"`

### Validation Rules

- Unknown switches should produce a startup warning and abort startup-open handling for safety.
- `/addto` requires at least one incoming file.
- `/here` and `/addto:*` should be mutually exclusive.
- `/line:N` should only be valid when exactly one target file is resolved.
- `/clean` with no files is valid.
- Empty file list entries inside `/files:` should be ignored or rejected consistently. Prefer rejection with a clear error.

## 6. Add A Startup Options Model

Suggested shape:

```rust
struct StartupOptions {
    mode: StartupOpenMode,
    add_to_target: Option<StartupTabTarget>,
    open_here: bool,
    files: Vec<PathBuf>,
    line: Option<usize>,
    log_cli: bool,
}

enum StartupTabTarget {
    Active,
    Index(usize),
}
```

Where it should live:

- In a small dedicated startup module, not inside `main.rs` directly.

Suggested module:

- `src/app/startup.rs`

Responsibilities:

- parse arguments
- validate combinations
- expose startup decisions to app construction

## 7. Apply Startup Requests After Session Decision

Recommended startup flow:

1. Parse CLI arguments into `StartupOptions`.
2. Decide whether session restore is enabled for this launch.
3. Construct the app.
4. If restoring, restore the session first.
5. Apply startup-open requests using the selected mode.
6. Surface any failures through status and logging.

Important policy:

- `Default`, `/here`, and `/addto:*` should restore the session first.
- `/clean` should bypass the restored tab set and start from a single fresh tab.

## 8. Reuse Existing FileController Paths

Do not build a second file-open engine.

Instead:

- Expose a startup-safe non-dialog entrypoint from `FileController`.
- Reuse the current batch open path for normal startup file opens.
- Reuse the current `Open Here`-style path for `/here` and `/addto:*`.

Recommended additions:

- `open_external_paths(app, paths)` for normal top-level opens
- `open_external_paths_here(app, paths)` for adding files into an existing workspace
- `open_external_paths_into_tab(app, target, paths)` if `/addto:index:N` is kept distinct from `/here`

## 9. Windows Open With Registration Strategy

Explorer integration needs a shell-registration path that launches Scratchpad with the correct arguments.

Recommended first-pass registration:

- Register Scratchpad as an `Open with` target using repeated file arguments.

Preferred shell command shape:

```text
"C:\Path\To\scratchpad.exe" "%1"
```

For multi-select support, evaluate shell behavior carefully and prefer repeated arguments if the registration path supports them.

Only use `/files:` in the registry command if repeated arguments are too awkward for the selected registration mechanism.

## Implementation Phases

## Phase 1: CLI Parsing And Startup Model

- Add a startup parsing module.
- Parse positional file arguments.
- Parse `/clean`, `/addto:*`, `/files:`, `/here`, `/help`, and `/version`.
- Validate conflicting combinations.

Definition of done:

- Startup options can be parsed deterministically from a raw argument list.

## Phase 2: Startup File Routing

- Add startup options to app construction.
- Decide restore-vs-clean behavior.
- Add a public non-dialog file-open path in `FileController`.
- Route `/here` and `/addto:*` through workspace-aware open logic.

Definition of done:

- Direct shell launch with files opens them correctly.
- `/clean` works.
- `/addto:active` works.

## Phase 3: Extended Targeting And Quality Of Life Switches

- Add `/addto:index:N`.
- Add `/help` and `/version`.
- Decide whether `/line:N` is ready or should be deferred.
- Add explicit logging for CLI parsing and startup execution.

Definition of done:

- Startup switch behavior is documented and testable.

## Phase 4: Windows Registration

- Add documentation for local shell registration.
- Add a PowerShell or `.reg` helper for local development.
- Validate quoted paths, spaces, and multi-file launches.

Definition of done:

- A user can choose Scratchpad from Windows `Open with` and the selected file opens with the expected startup mode.

## Testing Plan

## Unit Tests

- parse no args
- parse positional files
- parse `/files:` comma list
- parse `/clean`
- parse `/addto:active`
- parse `/addto:index:N`
- reject invalid switch combinations
- reject malformed `/files:` payloads

## Integration Tests

- startup open with one file
- startup open with multiple files
- `/clean` starts from one fresh tab
- `/addto:active` adds all files into one workspace
- duplicate existing paths activate instead of duplicating
- invalid files are skipped with status/log output

## Manual Windows Verification

- direct executable launch from PowerShell
- paths containing spaces
- `Open with` from Explorer
- multi-file Explorer selection
- registry command quoting validation

## Risks And Watchouts

1. `src/app/services/file_controller.rs` is already a hotspot.
   The startup-open entrypoint should be extracted carefully rather than piling more orchestration into one file.

2. `/files:` parsing can become fragile quickly.
   Keep the first version intentionally narrow and document unsupported edge cases such as commas in file names.

3. `/addto:index:N` depends on restored or current tab ordering.
   That makes it less stable than `/addto:active`, so it should be introduced after the active-target version is solid.

4. `Open with` may not pass arguments exactly the same way as manual shell launch.
   The registry command should be tested manually before the switch grammar is treated as final.

## Recommended First Implementation Slice

If the work should be broken into the smallest useful vertical slice, do this first:

1. Add positional file-argument parsing.
2. Add `/clean`.
3. Add a non-dialog external-open entrypoint.
4. Verify direct executable launch.
5. Add `/here` and `/addto:active`.
6. Add `/files:` only after the basic startup-open path is working.

That sequence delivers the simplest reliable startup behavior first, then layers on more specific command-line control.
