# egui Error Log Plan

## Goal

Add application-level diagnostics that capture egui-related UI issues, especially ID conflicts that can cause red flashes, into an `error.log` file. The implementation should preserve useful diagnostic data without making logging failures user-visible or fatal.

## Key Assumption

eframe/egui may not expose stable numeric error codes for ID conflicts. In the current app, the reliable model should be diagnostic categories rather than traditional numeric codes. The app can define its own structured codes such as `EguiIdConflict`, `EguiWarning`, and `Panic`, while also preserving raw egui/eframe log messages when they are available.

## Plan

1. Confirm what egui emits

   Verify how the repository's current egui/eframe version reports ID conflicts and related UI warnings. Check whether conflicts are exposed through structured APIs, logger messages, debug-only checks, or a combination of those. Treat egui's native output as diagnostic source data rather than assuming stable error codes exist.

2. Add a central diagnostics module

   Add a small diagnostics layer, likely under `src/app/diagnostics/`, responsible for:

   - Creating/opening `error.log`.
   - Appending structured diagnostic records.
   - Keeping write failures non-fatal.
   - Deduplicating repeated frame warnings.
   - Supporting future diagnostic categories beyond egui.

   Candidate data shape:

   ```rust
   enum AppDiagnosticKind {
       EguiIdConflict,
       EguiWarning,
       Panic,
       Io,
       Other,
   }

   struct AppDiagnostic {
       timestamp: String,
       kind: AppDiagnosticKind,
       message: String,
       source: Option<String>,
       widget_id: Option<String>,
       rect: Option<String>,
       frame: Option<u64>,
   }
   ```

3. Choose the `error.log` location

   Prefer an existing app data/config directory if the app already has one. If no clear app-owned directory exists yet, use the development working directory as an interim location. The chosen path should be deterministic and should not depend on whichever terminal launched the app unless that is explicitly intended.

   Proposed filename: `error.log`.

4. Initialize diagnostics early

   Initialize diagnostics before the first egui frame is rendered. Startup should:

   - Create parent directories if needed.
   - Open `error.log` in append mode.
   - Write a session header with timestamp, app version, OS, build profile, and egui/eframe version if practical.
   - Install a logger bridge if the app does not already have one.

   Example header:

   ```text
   === Scratchpad session started 2026-05-04T12:34:56Z ===
   version=0.3.0
   profile=debug
   os=windows
   ```

5. Capture app-owned ID conflict checks

   The app already centralizes custom widget ID checks through `widget_ids::track(...)` and `ctx.check_for_id_clash(...)`. Extend that path first because it is the most direct app-owned capture point.

   For each tracked widget, record:

   - Widget kind.
   - egui `Id` debug representation.
   - Current `Rect`.
   - Frame number if available.
   - The diagnostic category `EguiIdConflict` when a duplicate ID is detected by the app registry.

   Important: `Context::check_for_id_clash` may log internally rather than return a result, so the app should not rely on it as the only source of structured diagnostics.

6. Add a debug-only ID conflict registry

   Add a per-frame registry that records app-owned widget IDs before or alongside `ctx.check_for_id_clash(...)`.

   Each `widget_ids::track(ctx, id, rect, kind)` call should:

   - Check whether the same `id` has already been seen during the current frame.
   - If it has, append an `EguiIdConflict` entry containing the previous kind/rect and the new kind/rect.
   - Deduplicate repeated reports across frames so the same conflict does not flood the log.
   - Continue calling egui's native clash checker in debug builds.

7. Reset frame diagnostics at the frame boundary

   Add a hook near the start of the main app update loop to:

   - Increment a diagnostics frame counter.
   - Clear the current-frame ID registry.
   - Retain a bounded set of recently reported conflict fingerprints for deduplication.

8. Capture egui and eframe logger warnings

   Install or extend the app logger to mirror relevant egui/eframe warnings into `error.log`.

   Initial filters should include:

   - `egui` targets.
   - `eframe` targets.
   - Messages containing `id clash`, `id conflict`, `duplicate Id`, or similar wording discovered during verification.

   These records should use `EguiWarning` and preserve log level, target, message, timestamp, and frame number if available.

9. Add panic capture

   Install a panic hook that writes to `error.log` before delegating to the previous panic hook.

   Capture:

   - Panic message.
   - Source file and line if available.
   - Thread name.
   - Backtrace when enabled or cheaply available.

10. Keep logging safe and quiet

   Diagnostic logging must not create new app failures.

   Rules:

   - All writes are best-effort.
   - Logging errors are swallowed or stored in memory for later inspection.
   - No diagnostic path may panic.
   - Use append-only writes.
   - Consider later rotation or size caps, such as `error.log` plus `error.1.log`.

11. Add tests

   Add focused tests for the diagnostics layer:

   - Creates `error.log` if it is missing.
   - Appends rather than overwrites.
   - Formats diagnostic records consistently.
   - Deduplicates repeated ID conflicts.
   - Does not panic when the log path is unavailable.

   Add a unit test for the debug ID registry if it can be isolated from egui rendering.

12. Manual verification

   Add a temporary debug-only way to intentionally create an ID conflict, then run the app and confirm:

   - egui still shows its normal debug warning/red flash behavior.
   - `error.log` is created.
   - The log contains the conflicting ID, widget kind, previous/new rects, and frame number.
   - Repeated frames do not flood the file.

## Implementation Order

1. Add `AppDiagnostic` types and the `error.log` writer.
2. Initialize diagnostics during startup.
3. Add the debug-only ID registry.
4. Wire `widget_ids::track(...)` into the registry.
5. Reset diagnostics frame state during the app update loop.
6. Add an egui/eframe logger bridge.
7. Add a panic hook.
8. Add tests.
9. Add a short user-facing note for where `error.log` lives and what it captures.

## Risks And Open Questions

- egui ID conflict detection may remain debug-oriented, so the app-owned registry should be the reliable structured source for app widgets.
- Native egui widgets that do not pass through `widget_ids::track(...)` may only be visible through logger capture.
- The final log location should match the app's existing settings/state path conventions once those are confirmed.
- The log should avoid capturing user document content unless a future diagnostic explicitly requires it and the privacy implications are reviewed.
