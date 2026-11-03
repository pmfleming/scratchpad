# History storage

Scratchpad retains every edit branch and closed-file history until the user
explicitly forgets it. History has no age limit or disk budget. Settings reports
actual database, journal, settings, and history payload usage; storage grows as
changes accumulate.

The local Turso database is `session.db` in Scratchpad's state directory. On Linux
this is normally `$XDG_STATE_HOME/scratchpad`, or `~/.local/state/scratchpad`.
Session rows describe tabs, views, and document identities. `history` stores the
edits; `document_history` stores Now, selected tip, and edit recency. Current text
caches use deduplicated blobs. Everything is local; there is no remote Turso URL.

Memory limits evict loaded immutable text chunks, not edits. Cold chunks use
private temporary backing while the application runs, and are reconstructed from
the database on restart. The limits exclude tree metadata and the active editable
chunk. Temporary backing is released on process exit; Forget history removes the
durable tree without promising immediate reclamation of that temporary space.

Schema 10 does not migrate incompatible older or newer session formats. A failed
restore opens Status with the error and a **Start new session** action. The warning
explains that the old database will move to a temporary backup, which may be
deleted during temporary-file cleanup. The backup includes database sidecars and
is left in temporary storage after app exit. Its location is shown in Status.

Replacement retains the ownership lock, closes database handles, and copies the
complete file set before removing the originals. Saving resumes after a fresh
session containing the current workspace is committed at the original location.
An unsuccessful reset leaves saving paused and can be retried. Saved files and
settings remain unchanged; no reset happens without selecting the action.
