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

Schema 10 starts a fresh session when opening a recognized older Scratchpad
schema. Previous session/history data is discarded; files and settings remain.
An ownership lock protects replacement. Unknown future versions, foreign databases,
corruption, and other I/O failures never authorize an automatic reset.
