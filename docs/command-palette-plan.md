# Command Palette Plan

This plan covers the in-app searchable command surface that was previously bundled into the Windows/open-with planning document.

## Goals

- Add a searchable command palette for user-invokable editor actions.
- Reuse the existing `AppCommand` execution path instead of duplicating action behavior.
- Improve discoverability for existing keyboard-driven and button-driven actions.

## Current Review

### What Already Exists

- Scratchpad already has executable app actions in `src/app/commands.rs`.
- Keyboard shortcuts in `src/app/shortcuts.rs` already dispatch many of those actions.
- App state in `ScratchpadApp` is the right place to host command-palette UI state.

### Gaps Identified

1. There is no searchable command registry.
2. Shortcut handling is hard-wired and not metadata-backed.
3. There is no command-palette UI overlay.

## Proposed Architecture

## 1. Introduce A Command Registry Layer

Add a small command catalog above `AppCommand`.

Suggested shape:

```rust
enum CommandId {
    NewTab,
    OpenFile,
    OpenFileHere,
    SaveFile,
    SaveFileAs,
    CloseTab,
    CloseTile,
    SplitUp,
    SplitDown,
    SplitLeft,
    SplitRight,
    ToggleLineNumbers,
    IncreaseZoom,
    DecreaseZoom,
}

struct CommandSpec {
    id: CommandId,
    title: &'static str,
    category: &'static str,
    keywords: &'static [&'static str],
    shortcut_hint: Option<&'static str>,
}
```

Key design choice:

- Keep `AppCommand` as the execution payload layer.
- Add `CommandId` plus `CommandSpec` as the discoverability layer.
- Add an app-side resolver that maps `CommandId` to either a concrete `AppCommand` or direct app logic when a command is contextual.

## 2. Add Command Availability Evaluation

Some commands depend on current tab/view state.

Examples:

- `CloseTile` should be disabled when the active workspace has one tile.
- Split commands should require an active tab.
- `SaveFile` should stay available but may delegate to `Save As` when the file has no path.

Suggested shape:

```rust
struct CommandPresentation {
    spec: &'static CommandSpec,
    enabled: bool,
}
```

## 3. Add Command Palette State To ScratchpadApp

Suggested shape:

```rust
struct CommandPaletteState {
    open: bool,
    query: String,
    selected_index: usize,
}
```

Recommended behavior:

- Open with `Ctrl+Shift+P`.
- Close with `Esc`.
- Up/Down moves through results.
- `Enter` executes the selected command.
- Query filters by title, category, keyword, and shortcut hint.

## 4. Render The Palette As An Overlay

Render the command palette in the UI layer as a lightweight overlay, likely via `egui::Area` or a top-centered floating panel.

Suggested module:

- `src/app/ui/command_palette.rs`

Suggested responsibilities:

- text input and focus management
- filtered command rendering
- keyboard navigation
- execution callback on selection

## 5. Refactor Shortcuts To Reuse The Registry Where It Helps

Do not rewrite all shortcut handling at once. Start incrementally.

- Keep existing shortcut consumption in `src/app/shortcuts.rs`.
- Move the actual action dispatch behind a shared helper so shortcuts and the palette both call the same command execution path.
- Add the palette shortcut first.
- Gradually migrate existing hard-coded handlers to use `CommandId` where the mapping is straightforward.

## Implementation Phases

## Phase 1: Command Registry Foundations

- Add `CommandId` and `CommandSpec`.
- Add a registry module.
- Add command enablement evaluation based on current app state.
- Add a shared execution helper from `CommandId` to app behavior.

## Phase 2: Command Palette UI

- Add `CommandPaletteState` to app state.
- Add `Ctrl+Shift+P` shortcut.
- Render overlay UI.
- Execute selected commands from the shared registry path.

## Testing Plan

- command list stability tests
- enablement tests based on app state
- filtering tests by title and keyword
- selection movement tests
- execute-selected-command tests

## Risks And Watchouts

1. `src/app/commands.rs` is already a central execution surface.
   Avoid turning it into a metadata registry, UI state store, and dispatcher all at once.

2. Palette focus behavior needs to coexist with editor focus restoration.
   Opening the palette must take focus cleanly, and closing it must return focus to the editor predictably.