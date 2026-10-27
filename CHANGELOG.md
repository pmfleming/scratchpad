# Changelog

All notable user-visible changes to Scratchpad will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Changed

- Made warm session saves insert only new buffer IDs and collect only affected recovery blobs, with conservative rescans after restart, rollback, or detected damage. Positional tab-row rewrites and full capture/preparation costs remain.
- Added Linux lint and runtime-test coverage, including the native file watcher.
- Updated to upstream `eframe`/`egui` 0.37.0 and removed the local Wayland presentation fork, retaining WGPU by default and the Linux Glow fallback.
- Enabled native system-font fallback for missing scripts and color emoji while preserving bundled editor and icon fonts.
- Added accessible names and roles for custom controls, settings, search fields and dialogs.
- Added settled PRIMARY-selection publishing and middle-click paste targeting in the custom editor on X11/Wayland; known trackpad/momentum input no longer receives extra wheel acceleration.

### Fixed

- Protected saved sessions from placeholder/partial-workspace saves during startup restore and from fallback saves after restore failure, including shutdown and drop.
- Allowed shutdown when session writes are permanently blocked, while keeping retryable save failures open without per-frame last-tab exit retries.
- Kept piece-tree line indexes revision-local so snapshots cannot corrupt line lookup after history compaction.
- Serialized session persistence and committed manifests before stale-snapshot cleanup to prevent overlapping or interrupted saves from producing inconsistent restore state.
- Rejected overlapping saves for the same buffer instead of silently writing an untracked second Save As target.
- Made Linux directory watches recover after a watched directory is recreated and report native watcher failures through diagnostics.
- Made the CPU-only frame benchmark consume egui texture deltas so all-target debug tests complete cleanly.

## 0.4.2 - 2026-08-13

### Added

- Added a per-user single-instance broker that forwards files and workspace targets to the running Scratchpad window.
- Added smooth keyboard caret transitions that snap during edits, pointer input, scrolling, focus changes, large jumps, and IME composition.
- Added an MIT license and a detailed configuration guide with a tested example settings file.
- Added file-backed piece-tree storage and staged hydration for large files and workspaces.

### Changed

- Updated `eframe`/`egui` and the local Phosphor integration to 0.36.1.
- Reframed Scratchpad as a local-first Windows and Linux text workspace and refreshed the README, user manual, Linux packaging, and Home Manager documentation.
- Pinned and documented the supported Rust toolchain.
- Strengthened contributor, security, dependency-audit, Cargo feature-matrix, and per-rule reliability checks.
- Replaced recoverable application invariants with non-panicking save, search, restore, and UTF-8 handling.
- Updated vulnerable transitive dependencies used by Wayland XML generation and benchmark support.

### Fixed

- Improved bounded memory behavior and responsiveness when opening and navigating large files.
- Corrected rustdoc link markup in the editor scrolling documentation.
- Documented the safety invariant for the resource-probe global allocator implementation.
