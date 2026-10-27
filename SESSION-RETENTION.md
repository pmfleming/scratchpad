# Retained session data

## Closed tabs (item 1)

Explicit workspace-tab closes capture the final text, layout and undo/redo history
before removing the tab. The next session transaction commits the closed record
and live workspace together. Failed saves leave the pending capture retryable;
background acknowledgements retire only the captures they actually carried.
Closing the last tab therefore preserves it even though the application exits.

The newest **32 tabs** are retained in `closed_tabs`. Their snapshots/history use
the same buffer rows and content-addressed blobs as the live session. Retention
expiry removes their recovery roots and ordinary garbage collection reclaims
unreferenced content. A close is not free: unique text and undo payloads consume
space. The limit is currently fixed, not a configurable byte budget.

Use **Reopen Last Closed Tab** in a tab's context menu, including after restarting.
The archive is consumed only by a successful save of the reopened workspace.
Recovered disk-backed text is marked dirty deliberately, so external file changes
cannot silently replace the archived text or invalidate its undo history. No
reopen operation writes original files. Cold file shells with no stored snapshot
still depend on their source file; loading previews cannot archive unavailable
bytes. Closing individual split views is not workspace-tab retention.

This stage uses database schema **4**. Older schemas are rejected intact, with no
migration or automatic reset. To opt into a fresh schema, stop Scratchpad and
move the old state directory aside yourself. Original text files are unrelated.
The logical open-session export does not include closed archives; retain the
whole database/WAL using the documented safe backup procedure for full recovery.

Tests cover close-last/restart/undo, bounded retention and blob sharing, idempotent
retries, and transaction failure before archive commit.
