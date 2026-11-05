# Stage 3: live add chunks and background preparation

Live add-buffer rollover now seals immutable handles into the document's explicit
session store. No add-buffer spill file or process-global store binding is used.
Unbound documents keep sealed text in RAM. Clones inherit their binding and share
committed/pending handles; a different epoch requires explicit copying rather
than silently interpreting the same row IDs in another database. Existing file
originals retain their temporary import path until stage 5.

Restored/cold/reopened documents and path-load requests receive their owning
handle. Cache limits are applied to the store. Foreground viewport pins read via
the dedicated reader; nearby stored ranges are prefetched through a bounded weak
queue. Search windows, scans, hashes, and disk saves read without populating or
evicting the viewport cache. The final unified original/add cache follows when
file originals leave the temporary importer.

Pastes and replacements at 256 KiB, and replacement plans above 256 edits, use
`PreparedText` on the path lane. Healthy bulk work waits at the 32 MiB pending
threshold on that lane, allowing at most one chunk of overshoot. It does not wait
on paint or input. Normalization, edits/history, and metadata analysis complete
before publication. There is one active preparation/outbox per buffer and the
existing bounded path queue. Input during preparation is retained in order for
replay. Close operations preserve queued input and focus its view before closing;
shutdown integration remains stage 6.

The eframe raw-input hook moves large editor events before egui's begin-pass copy.
It preserves event order, leaves non-editor widgets/global-shortcut and pointer
focus routing intact, and keeps ordinary small input on the existing path.
Prepared publication checks the original history token. A stale prepared result
is retained in a recovered tab. Other split-view carets are transformed through
the edit rather than moved to the originating caret. Loaded replacements also
prepare their reversible history off-thread and install only against the captured
original token; unchanged/dirty observations still import no replacement text.

## Evidence

All-feature tests and doctests passed, including 678 library tests and eight
intentional ignores. All-target/all-feature Clippy passed with warnings denied.
Coverage includes atomic Unicode preparation, exact undo/redo, independent split
carets, immutable/divergent snapshots, queued-input close protection, non-editor
raw input routing, and scan policy preserving the viewport cache.

`resource_probe --live-text-stage-three` injects a 200 ms writer job and sends a
64 MiB Unicode paste through production raw-input, logic, editing, layout, and
tessellation. It verifies text preceding the paste and typing during preparation.
This measures headless rendered work, not GPU present or actual device latency.
Three final-source runs are preserved in `evidence/stage-3/frames.jsonl`. Pending
sealed bodies peaked at 33,816,496 bytes, below 32 MiB plus one 256 KiB chunk.

The initial producer exposed egui's extra full-payload input copy; its frame event
is retained in `frames-before-input-hook.jsonl`. The final producer intercepts that
copy while preserving input routing. Exact final run times are in the event file;
no performance budget has been relaxed.

Database schema remains 12 / session export version 10. The old text/history
persistence path is deliberately replaced in stage 4 on this same development
branch. Degraded-operation RAM limits/status/reset/shutdown are stage 6 work.
Native Windows runtime validation remains pending under the user's authorization.
Fresh RQL uses an immutable stage-3 checkout and the local archive
`target/quality-runs/live-text-stage-3-frozen`; strict policy gaps are reported
without changing limits.
