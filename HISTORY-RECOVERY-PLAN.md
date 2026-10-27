# History & recovery implementation

Stages (each ends with a commit):

1. Settings policy and asynchronously refreshed storage accounting.
2. Preserve historical snapshots before undo compaction, with transactional persistence.
3. Archive-aware reopening and safe handling of external changes.
4. Automatic budget/age maintenance and protected archives.
5. Unified history/recently-closed UI.
6. Compatibility, regression tests and documentation.

Storage accounting must distinguish filesystem allocation from stored payload sizes.
Undo, live snapshots and historical snapshots share one database; text chunks can
belong to several categories. Never sum overlapping references or label payload
sizes as exclusive filesystem allocation. Report database/WAL/other files and the
whole data directory separately, including reusable database space where supported.

Safety: no automatic writes to original files; failed recovery must not consume an
archive; snapshot creation and removal of persisted undo belong to one transaction.
Retention is finite and explained in Settings; operational failures remain visible.
