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

Schema 10 starts a fresh session when opening a recognized older Scratchpad
schema. Previous session/history data is discarded; files and settings remain.
An ownership lock protects replacement. Unknown future versions, foreign databases,
corruption, and other I/O failures never authorize an automatic reset.
