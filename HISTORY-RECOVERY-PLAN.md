# History & recovery implementation

Stages (each ends with a commit):

1. Settings policy and asynchronously refreshed storage accounting.
2. Preserve historical snapshots before undo compaction, with transactional persistence.
3. Archive-aware reopening and safe handling of external changes.
4. Automatic budget/age maintenance and protected archives.
5. Unified history/recently-closed UI.
6. Compatibility, regression tests and documentation.

Completed in six staged commits. Stage 6 also closed integration gaps: durable
pre-close commits, archive-aware Open Here, automatic startup conflict separation,
clean-buffer historical snapshots, archive-specific snapshot thinning, and totals
covering separate settings/state directories.

Validation:
- `cargo test --all-features`: library, binary, integration and documentation tests.
- `cargo clippy --all-targets --all-features -- -D warnings`.
- `cargo fmt --all -- --check` and `git diff --check`.
- Targeted cases cover migration, rollback, pre-close failure, undoable recovery,
  external edits, missing files, pins, age/global budgets, archive compaction,
  snapshot thinning, restart, distinct settings roots, and accessible preview UI.

Scope notes: recovery points appear as a named section/tab within the existing
History window, rather than pretending sampled points are individual edits. Actual
per-feature disk allocation cannot be separated exactly inside the shared database;
Settings explicitly distinguishes filesystem totals from payload attribution.
No Windows runtime, manual desktop interaction, real power-loss, or large-workload
performance qualification is claimed by these automated checks.

Storage accounting must distinguish filesystem allocation from stored payload sizes.
Undo, live snapshots and historical snapshots share one database; text chunks can
belong to several categories. Never sum overlapping references or label payload
sizes as exclusive filesystem allocation. Report database/WAL/other files and the
whole data directory separately, including reusable database space where supported.

Safety: no automatic writes to original files; failed recovery must not consume an
archive; snapshot creation and removal of persisted undo belong to one transaction.
Retention is finite and explained in Settings; operational failures remain visible.
