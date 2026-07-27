# Changelog

All notable user-visible changes to Scratchpad will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Changed

- Pinned and documented the supported Rust toolchain.
- Strengthened contributor, security, dependency-audit, Cargo feature-matrix, and per-rule reliability checks.
- Replaced recoverable application invariants with non-panicking save, search, restore, and UTF-8 handling.
- Updated vulnerable transitive dependencies used by Wayland XML generation and benchmark support.

### Fixed

- Corrected rustdoc link markup in the editor scrolling documentation.
- Documented the safety invariant for the resource-probe global allocator implementation.
