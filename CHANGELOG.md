# Changelog

All notable user-visible changes to Scratchpad will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- Retained closed workspace tabs under configurable age and byte budgets, including unsaved text and undo/redo history, with reopening after restart.
- Added bounded per-buffer save points sharing stored text chunks; recover them into new tabs from the Saved Versions menu.

### Changed

- Shared cached recovery-query results across UI frames instead of copying archive/version lists, and removed the obsolete startup-conflict dialog workflow.
- Reduced session-worker history caches to per-row fingerprints and reused unchanged chunk hashes through bounded byte-range edit tracking.
- Advanced session storage to schema 8 with transactional upgrades from schemas 6 and 7. Stored byte counts and covering indexes avoid scanning undo/text payloads during retention accounting and disk-usage refreshes. Older schemas are preserved but not migrated.
- Made warm session saves insert only new buffer IDs and collect only affected recovery blobs, with conservative rescans after restart, rollback, or detected damage. Positional tab-row rewrites and full capture/preparation costs remain.
- Added Linux lint and runtime-test coverage, including the native file watcher.
- Updated to upstream `eframe`/`egui` 0.37.0 and removed the local Wayland presentation fork, retaining WGPU by default and the Linux Glow fallback.
- Enabled native system-font fallback for missing scripts and color emoji while preserving bundled editor and icon fonts.
- Added accessible names and roles for custom controls, settings, search fields and dialogs.
- Added settled PRIMARY-selection publishing and middle-click paste targeting in the custom editor on X11/Wayland; known trackpad/momentum input no longer receives extra wheel acceleration.

### Fixed

- Kept reload and preview revision guards stable across view-only anchor updates while rejecting independently replaced documents.
- Kept history-preview replay tied to the original document identity, and preserved the selected view when restoring text with follow-focus disabled.
- Rejected stale reload and encoding-reopen results after intervening edits, document replacement, path changes or newer disk baselines, preserving current text.
- Cleared stale disk/settings-file metadata consistently when recovering untitled archive copies, and exposed accessible names for recently closed file buttons.
- Quarantined damaged closed-tab content encountered during undo compaction without blocking healthy session saves.
- Cached and acknowledged computed undo recovery points so idle saves no longer repeatedly reconstruct history and write unchanged recovery tokens.
- Excluded pinned and quarantined data from retention-budget totals so it cannot evict all unprotected archives and recovery points.
- Restored changed clean files from disk consistently in mixed split tabs while keeping the original archive available after subsequent saves.
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
