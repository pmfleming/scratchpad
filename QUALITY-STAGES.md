# Follow-up quality stages

Baseline: `d8fc4c4` (the preceding recovery refactor). Each stage is committed
separately. No sibling repository changes are included.

## 1. Asynchronous reload/reopen safety

- One pending-load model and completion path for explicit reload, automatic
  refresh and encoding reopen. Removed four validated-location `expect` calls,
  redundant metadata clones and the unused reload-error wrapper.
- Requests capture document identity plus generation, not a numeric revision
  alone: replacing a document can reset that number. Capturing the weak revision
  key does not clone text or runtime anchors.
- Completion validates buffer ownership, requested/result paths, result cardinality,
  disk baseline and document identity. Explicit reload can discard pre-existing
  edits, but not edits made while the request was pending. Auto-refresh conflicts
  and failed-load status remain mode-specific.
- A 3-mode × 10-scenario completion matrix and real queued-load journeys cover
  edits (including edits marked clean), replacement, rename, closure, newer disk
  baseline, errors, mismatched paths, empty/extra results and duplicate requests.
- Passed: focused save tests, full locked all-feature tests, formatting and strict
  all-target/all-feature Clippy. Logs: `target/quality-stage1-tests.log`.
