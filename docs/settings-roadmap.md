# Settings Roadmap

## Goal

Expose user-configurable settings in the app, provide a dedicated settings UI, and persist those settings in a TOML file that is independent from workspace/session restore data.

## Why This Change

The app currently stores user-facing preferences such as `font_size` and `word_wrap` inside the session persistence flow. That creates a coupling between:

- user preferences
- session restore behavior
- workspace/tab restoration

This makes preferences harder to reason about and means settings behavior is tied to whether session restore is enabled. A dedicated settings system should:

- load even on clean startup
- persist independently of open tabs and buffers
- be safe to evolve without changing session restore semantics
- support a clear UI for future settings growth

## Current State

Current runtime settings live on `ScratchpadApp` in [`app_state.rs`](/C:/Code/scratchpad/src/app/app_state.rs):

- `font_size`
- `word_wrap`

Current persistence happens via the session store in [`session_store/mod.rs`](/C:/Code/scratchpad/src/app/services/session_store/mod.rs), where those values are serialized alongside:

- open tabs
- pane layout
- active tab
- active view
- buffer/session metadata

That persistence model is appropriate for session restore, but not ideal for stable user settings.

## Target Architecture

### Settings Domain

Introduce a dedicated `AppSettings` model that represents persistent user preferences.

Suggested initial fields:

- `font_size: f32`
- `word_wrap: bool`

Likely future fields:

- `restore_session_on_startup: bool`
- `show_line_numbers_by_default: bool`
- `default_open_target`
- theme or appearance settings
- editor behavior toggles

### Storage Model

Store settings in a TOML file, separate from session data.

Suggested file:

- `settings.toml`

Suggested ownership:

- a new `SettingsStore` service responsible for load/save/default/fallback behavior

Suggested responsibilities:

- load settings from TOML
- return defaults when the file is missing
- handle malformed TOML gracefully
- save updated settings atomically
- expose the resolved settings file path for diagnostics/UI

### Session vs Settings Split

Keep session data focused on:

- tabs
- workspace composition
- pane tree
- active tab/view
- open buffers
- session-only restore state

Move stable preferences into the settings file:

- font size
- word wrap
- logging enabled

This separation will make `/clean` startup behavior easier to reason about and reduce accidental coupling between preferences and workspace restore.

## UI Direction

### Recommended First UI

Implement a settings modal or panel first, even if the long-term goal is a full settings page.

Why:

- lower implementation cost
- easier to integrate with current layout
- faster validation of settings model and persistence
- easier to evolve into a full page later

### Long-Term UI

Add a dedicated settings page/view once the settings model is stable.

Possible direction:

- add an app-level mode such as `Editor` vs `Settings`
- route navigation through commands
- keep page layout grouped by sections

Suggested first sections:

- Editor
- Logging
- Session
- Advanced

## File/Module Plan

### New Files

- `src/app/services/settings_store.rs`
- `src/app/ui/settings.rs`

### Existing Files Likely To Change

- `src/app/app_state.rs`
- `src/app/commands.rs`
- `src/app/shortcuts.rs`
- `src/app/ui/mod.rs`
- `src/app/services/mod.rs`
- `src/app/services/session_store/mod.rs`
- `src/app/services/session_manager.rs`
- `Cargo.toml`

## Persistence Plan

### Data Model

Create a serializable `AppSettings` struct with `Default`, `Serialize`, and `Deserialize`.

Design goals:

- defaults are centralized
- unknown fields are ignored safely if desired
- future additions remain backward-compatible

### TOML Serialization

Add TOML support with `toml`.

Settings store behavior:

1. Resolve settings file path.
2. If the file does not exist, return defaults.
3. If the file exists, deserialize TOML into `AppSettings`.
4. If deserialization fails, return defaults and surface a warning status/log entry.
5. Save settings atomically on change.

### Save Strategy

Recommended first version:

- save immediately when a setting changes

Reasons:

- simple mental model
- fewer edge cases than staged Apply/Cancel
- aligns with current persistence style in the app

Potential later enhancement:

- dirty-state tracking and explicit Apply/Reset controls

## Startup Flow Plan

Desired startup order:

1. Construct app with defaults.
2. Load TOML settings.
3. Apply settings to runtime app state.
4. Restore session if startup options allow it.
5. Apply startup file-open behavior.

This order ensures:

- preferences are available on every launch
- `/clean` still preserves settings
- session restore stays focused on workspace state

## Migration Plan

We should not break existing users who already have session manifests containing current preference values.

Recommended migration approach:

1. Introduce TOML settings loading.
2. If `settings.toml` exists, it becomes the source of truth.
3. If `settings.toml` does not exist, optionally migrate legacy `settings.yaml` or fall back to legacy values from the session manifest.
4. After the first successful settings save, TOML becomes canonical.
5. Stop writing migrated settings into new session manifests once the settings system is fully adopted.

This gives a gentle transition with minimal surprise.

## UX Plan

### Entry Points

Suggested ways to open settings:

- header button
- status bar button
- keyboard shortcut such as `Ctrl+,`

### Initial Controls

Recommended first controls:

- font size slider or drag value
- word wrap toggle
- runtime logging toggle

Optional near-term additions:

- restore previous session on startup
- reset settings to defaults
- reveal/open settings file location

### Error Handling

If settings load/save fails:

- keep the app usable with in-memory defaults
- show a clear status message

## Risks And Watchouts

### 1. Over-coupling UI state and persisted state

Avoid scattering direct writes to `font_size` and `word_wrap` throughout the codebase. Prefer setter methods or a central settings update path.

### 2. Session restore precedence confusion

Be explicit about whether session restore is allowed to override settings-derived runtime values. In the target design, stable preferences should come from TOML, not the session manifest.

### 3. Partial migration bugs

If both TOML and session persistence write the same fields during transition, behavior may become confusing. Define one source of truth per release step.

### 4. Settings page scope creep

Ship the persistence and first three settings before expanding into a large preferences system.

## Recommended Rollout

### Phase 1: Foundation

- Add `toml`
- Add `AppSettings`
- Add `SettingsStore`
- Load defaults or TOML on startup

### Phase 2: Runtime Integration

- Add central setter methods for settings mutations
- Route existing font size and wrap mutations through settings-aware setters
- Save settings immediately on change

### Phase 3: UI

- Add settings modal or page
- Add entry points and shortcut
- Bind controls to the settings model

### Phase 4: Migration

- Add fallback reading from legacy session values if needed
- Stop treating stable settings as session-owned values

### Phase 5: Expansion

- Add more settings categories
- Add reset/export/import improvements if useful

## Implementation Checklist

### Foundation

- Add `toml` to `Cargo.toml`.
- Create `src/app/services/settings_store.rs`.
- Define `AppSettings` with `Default`, `Serialize`, and `Deserialize`.
- Define a stable settings file path and filename.
- Implement `load()` for TOML settings with default fallback.
- Implement `save()` with atomic write behavior.
- Add unit tests for missing-file, malformed-file, and valid-file cases.

### App Integration

- Add settings storage/state to `ScratchpadApp`.
- Load settings during app construction before session restore.
- Apply loaded settings to runtime fields.
- Add centralized setters such as:
- `set_font_size(...)`
- `set_word_wrap(...)`
- Ensure all existing mutation sites use those setters instead of writing fields directly.
- Persist settings immediately after changes.
- Surface load/save failures through the existing status/log system.

### Session Boundary Cleanup

- Remove ownership of stable preferences from the session store design.
- Decide whether to keep legacy fields readable during migration.
- Add fallback migration logic from session manifest to TOML if no settings file exists.
- Stop writing migrated preferences into new session manifests when ready.
- Add regression tests covering clean startup vs restore-session startup.

### UI

- Create `src/app/ui/settings.rs`.
- Add a settings entry point in the UI.
- Add a `Ctrl+,` shortcut for opening settings.
- Add controls for:
- font size
- word wrap
- Show save/load errors in the UI status system.
- Show the resolved settings file path somewhere in the settings UI.
- Add a reset-to-defaults action if time allows.

### Verification

- Add tests for settings persistence and migration.
- Verify settings persist across app restart.
- Verify settings still load on `/clean`.
- Verify session restore no longer overrides canonical settings unexpectedly.
- Verify existing session restore tests still pass.
- Verify malformed TOML falls back cleanly without crashing the app.

## Suggested First Milestone

The smallest useful shippable milestone is:

- TOML-backed `AppSettings`
- startup load
- immediate save on change
- one simple settings UI surface
- migration support for current `font_size` and `word_wrap`

That gets the architecture right early while keeping the implementation manageable.
