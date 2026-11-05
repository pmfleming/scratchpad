# Changelog

All notable user-visible changes to Scratchpad will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- File edit trees with an empty starting point, explicit Now, preserved branches, and recently edited file discovery, including closed files.

### Changed

- Session documents now retain their text and edit trees independently of open tabs and closed split layouts. Closing a file's final split view commits its retained state before removing the view. The new storage format requires the existing explicit fresh-session reset for older sessions.
- Made History's Forget actions file-specific with a named confirmation, and fixed picker dismissal, caret preservation when selecting files, and selection of files reopened from split tabs.
- Refined History labels, dates, operation icons, folder paths, selection indicators and contrast; kept visible rows stationary during navigation and sized confirmations without resizing the window.
- Redesigned History with newest-first branch lanes, a searchable file picker, local edit times, readable whitespace labels, and a scroll-to-Now control. All files uses one scrolling list, includes only files with recorded history, and stays open when selecting a file.
- Redesigned the scrubber as a square, stable-width card with a wider live track, branch markers, step/start/end controls, and Open in History. Escape and resumed editing still dismiss it; history navigation and undo/redo behaviour are unchanged.
- Undo, redo, History clicks, and the selected-branch scrubber share the same persisted position. Hover leaves text unchanged.
- Removed saved-version snapshots and automatic history expiry. Forget history is explicit and per file.
- Incompatible session versions now show a warning with a **Start new session** action. The old database moves to temporary backup storage before a fresh session is created and saving resumes; files and settings are preserved.
- Status messages and error details wrap and remain visible without a Detail dropdown.

- Removed the History list's inner focus border so the list stays visually integrated with the main panel.

- Shade every History row between the starting document state and the current live position, with distinct endpoint markers.

- Highlight specific matching settings once a search has at least two meaningful characters and matches fewer than half the settings; support setting labels and aliases, including hidden tab-delay controls.

- Aligned Settings with the shared UI font, background, text, border, and accent colours in both themes.

- Added a font-size slider beside the preset dropdown, adjustable from 8 to 72 pt.

- Replaced settings' Expand all and Collapse all text buttons with right-aligned icons and tooltips.

- Kept `Ctrl+F` for Find, moved Replace to `Ctrl+Shift+F`, and assigned `Ctrl+H` to History.

- Removed the scrubber's close button; Escape and resumed editing dismiss it.

- Made the scrubber appear on undo/redo and remain available for keyboard or pointer navigation until editing resumes or Escape dismisses it; removed its status-bar button and dedicated shortcut.

- Deferred search refresh until a scrubber drag ends or is cancelled, and avoided reactivating the same editor on every step while keeping session dirtiness current.
- Replayed history entries directly in their owning file instead of rebuilding the workspace history twice per scrubber step.

- Bounded history previews and shared cached entries across panel, scrubber, and keyboard rendering instead of cloning full edit text.

- Made History resizable with square corners and the workspace background; Linux and Hyprland dismiss it with Escape or an outside click instead of a close button.
- Kept the History panel width stable with large change counts and switched the editor immediately when choosing a file in History.
- Made the history scrubber update the file and highlight the current change while dragging, retaining guards against edits or file switches during a drag.
- Reduced session-worker history caches to per-row fingerprints and reused unchanged chunk hashes through bounded byte-range edit tracking.
- Made warm session saves insert only new buffer IDs and collect only affected recovery blobs, with conservative rescans after restart, rollback, or detected damage. Positional tab-row rewrites and full capture/preparation costs remain.
- Added Linux lint and runtime-test coverage, including the native file watcher.
- Updated to upstream `eframe`/`egui` 0.37.0 and removed the local Wayland presentation fork, retaining WGPU by default and the Linux Glow fallback.
- Enabled native system-font fallback for missing scripts and color emoji while preserving bundled editor and icon fonts.
- Added accessible names and roles for custom controls, settings, search fields and dialogs.
- Added settled PRIMARY-selection publishing and middle-click paste targeting in the custom editor on X11/Wayland; known trackpad/momentum input no longer receives extra wheel acceleration.

### Fixed

- Preserve each view's caret and selection when reopening a closed file or restoring a session.

- Preserve replayability after recovery for replacements that delete multiple piece-tree spans. Their fingerprints now follow persisted payload boundaries.
- Reject stale history interactions after edits or document replacement, and prevent stationary hover from overriding later navigation.
- Use one ordered activity stream for scrubber visibility, including undo boundaries and IME input.

- Refresh cached History rows when a document is replaced or restored, even if its history revision counter is unchanged, so clicks use the current undo/redo position.

- Focus the History list when it opens so arrow keys navigate immediately, without first hovering a row or switching files.

- Kept history replay in the target file’s active split view, matching the History file picker.

- Refreshed cached History filenames immediately after rename or Save As.

- Let search, tab renaming, settings, and popups handle Escape before dismissing an unfocused History panel.
- Kept reload and preview revision guards stable across view-only anchor updates while rejecting independently replaced documents.
- Rejected stale reload and encoding-reopen results after intervening edits, document replacement, path changes or newer disk baselines, preserving current text.
- Cleared stale disk/settings-file metadata consistently when recovering untitled archive copies, and exposed accessible names for recently closed file buttons.
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
