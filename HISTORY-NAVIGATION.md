# Live history navigation contract

History and the scrubber navigate the same live document and retained undo/redo
timeline. Position k means the state after k retained edits. Navigation is atomic;
an unavailable target or replay conflict leaves text and history unchanged.

- Armed hover, row clicks, navigation keys, and scrubber dragging select a live
  position. Opening a surface or a stationary pointer does not select a new state.
- Selecting the current position does nothing. There is no confirmation edit.
  Escape and outside clicks dismiss the surface without rolling back navigation.
- Replay preserves both sides of the timeline. Editing at an older position starts
  the ordinary new branch. Navigation updates recovery and dirty state but does
  not save the source file.
- Snapshots use live visit/return navigation, preserving the original position and
  redo tail across restart. Loading cannot overwrite a newer edit or selection.
- Explicitly opening History focuses its navigation; automatic scrubber opening
  leaves focus in the editor. Undo at a boundary still opens the scrubber. Editing
  dismisses it, including IME intent before the text is committed.
- Rail navigation updates an already visible scrubber; it does not open an overlay
  over the rail. Typing or undo/redo while either tool has focus returns input to
  the editor without dropping the event.
- Every cache and outstanding action validates document identity and text/history
  revisions. A gesture accepts its own replay; unrelated changes invalidate it.

Snapshot selection and its preserved return buffer belong to the buffer. UI reads
do not change visits. Clearing history during a visit first checkpoints the cleared
return branch, so returning after restart does not resurrect its undo entries.

Regression coverage includes seeded edit/replay/recovery sequences, replay barriers,
split views, focus, stationary hover, stale snapshot loads, large checkpoints, and
snapshot return navigation across restart.
