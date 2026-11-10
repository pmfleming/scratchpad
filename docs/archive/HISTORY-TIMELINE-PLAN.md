# History timeline implementation plan

Status: all six implementation stages are complete. The validation record below
distinguishes passing Linux checks from unavailable Windows validation and the
non-passing strict reliability gate. Stage 1 is complete: fresh schema 11/session
version 9, document-owned metadata, reference-only workspace layouts, independent
retention, and transactional final split-view closes. Validation: 614 library tests
passed (6 manual/profile tests ignored), all-feature library Clippy passed, and
formatting passed. Stage 2 is complete: retained path lookup, identity-preserving
reopen/startup reconciliation, a distinct external edit source, separate saved and
observed disk versions, background retained-path/cold-shell loading, and removal
of recovery identity forking and both recovery policy settings. Validation: 617
library tests passed (6 ignored), all-feature library Clippy and formatting passed.
Stage 3 is complete: metadata-only keyset queries, a separately indexed/virtualized
Timeline, stable edit row identities, independent keyboard navigation, and background
opening of a selected closed document. Turso's UNION probe showed a sorter, so the
combined activity feed will use limited source queries and merge their results.
Validation: 625 library tests passed (6 ignored), the final UI-focused pass passed
25 tests (1 ignored), and all-feature library Clippy/formatting passed.
Stage 4 is complete: transactional lifecycle activity, capture-specific
acknowledgements, informational rows, pending-event projection and deduplication,
blank untitled cleanup, cancellation of loading previews, and split/cold-shell
installation guards. Opens use the normal session durability boundary. Validation:
636 library tests passed (6 ignored), all-feature library Clippy and formatting
passed. Stage 5 is complete: background stable reads/digests for clean and dirty
files, per-version conflict observations, missing/return handling, deferred Save
conflict checks, startup observation deduplication, and saved-snapshot digests.
Validation: 641 library tests passed (6 ignored), formatting and strict all-feature
library Clippy passed.
Stage 6 is complete: explicit root/node reference constraints, indexed closed-layout
updates, Forget/reset and query invalidation coverage, stable page handover and
viewport anchors, Unicode search, accessible static activity/date information,
Save As Copy identity, retained-path replacement, late path-claim recovery, owned
background replacement text, prompt wording and user documentation. Final validation:
711 tests passed across 35 suites (9 manual/profile tests ignored); the retained
Timeline profile also passed when explicitly run. Formatting, all four feature
combinations, strict Clippy, audit, Turso lock checks and the native Linux check passed.
Windows tooling is unavailable. Fresh strict RQL remains non-passing for the reasons
recorded below; its policy has not been weakened.

Replace History's **All files** view with **Timeline**, showing recorded text
changes, file opens, file closes, and external changes discovered by Scratchpad.
Keep **This file** and the scrubber as views of the existing per-document edit
tree. Use a fresh database schema with no migration, legacy import, compatibility
reader, or dual-write transition.

The implementation should concentrate changes in persistence, file lifecycle
coordination, and History presentation. Preserve the piece tree, reversible edit
operations, branch selection, atomic seek, and ordinary undo/redo behavior.

## Product contract

- Show activity newest first across open, closed, saved, and unsaved documents.
  Include files that were opened and closed without being edited.
- Record successful opens and reopens, completed closes, grouped text edits, and
  observed external changes. An explicitly created untitled document appears as
  **Created** rather than claiming a disk file was opened.
- Retain changes from every branch, including changes no longer applied. A new
  branch does not remove earlier timeline entries.
- Do not record saves, file switches, undo/redo, cursor movement, or individual
  keystrokes. Preserve current typing coalescing.
- Opening an already open document only activates it. Adding or removing another
  view of the same document does not count as opening or closing the document.
  Closing its final workspace view does.
- Automatic session restoration and application shutdown do not manufacture
  open/close events for every restored tab. Explicit file actions do.
- Keep a separate Now position for each document. Timeline order never implies
  a shared cross-file undo cursor.
- Opening History, hovering rows, scrolling, and searching do not change text.
  Explicitly selecting an edit row may change the editor, as today, but never
  writes that state to disk.

### Text states and lifecycle rows

A file's states come only from its edit rows. Open, close, and observation rows
mark when something happened; they are not states.

- **Lifecycle and observation rows are informational.** They cannot be clicked,
  hovered as actions, or reached by keyboard selection. They store no node target.
  The state before or after one is the neighboring edit row for that file.
- **The time a file is closed is a black box.** The last state before closing is
  the last known state. If disk contents differ on reopening, the first state after
  opening is the new disk text, recorded as one **External changes loaded** edit
  in the same document. Scratchpad does not claim to know what happened between
  those two states, or when.
- Observed external changes are timestamped when Scratchpad discovers them.

## Fresh storage model

Keep Turso and the existing edit serialization. Introduce the following ownership
and indexing rules directly in the new schema; the names below describe the
intended tables rather than final SQL declarations.

1. **documents** replaces `buffer_ids` and owns the persistent document ID, current
   name/path, and the per-document metadata currently held in the `SessionBuffer`
   payload. It also stores the last observed disk version: digest and missing
   state. Reuse the existing stable `temp_id` identity through the application
   boundary. Index path so that opening a path resolves to its retained document.
   Rename and ordinary Save As retain identity, while an explicit duplicate receives
   a new one.
2. **history** owns reversible edit nodes, keyed by document ID and node ID, with
   parent, edit timestamp, global sequence, source, summary, and serialized edit
   data. Source and summary are columns because `entry` contains edit text and rows
   must draw without reading it. Keep current parent ordering and replay validation.
   The root remains the implicit empty state. Initial contents remain a replay
   baseline, excluded from timeline edits.
3. **document_history** owns Now, selected tip, and actual edit recency. Lifecycle
   events must not change the recently-edited-file ordering.
4. **activity** stores lifecycle events and external-change observations that do
   not produce an edit. Each record has a global sequence as its ID, observation
   timestamp, kind, document ID, and filename/path at the event. It has no node,
   branch, or related-document reference.

Use one allocator for edits and activity. On startup, restore it above the maximum
sequence across `history` and `activity`. No separate high-water table is needed:
paging cursors do not survive a restart, and the running counter never goes back.

Keep workspace/tab layout and closed-workspace records to restore a closed tab's
split layout. They reference documents but do not own their lifetime. Removing a
closed-tab record must not delete the document, edit tree, or activity. If one
document from a closed split is opened alone, that split's record is consumed as
today. The other documents stay retained and can still be opened by path.

The timeline is a read projection of edit nodes plus activity records. Do not
duplicate text edits into the activity table.

Index global sequence and document/sequence in both sources. Use the sequence and
document/node identity as a total paging order, because an edit's global sequence
has no uniqueness constraint.

Use database constraints and restore validation for document references, node
references, and event kinds. Represent the implicit root explicitly in the
reference contract without requiring a persisted root edit.

## Recording and persistence

Change the save path so that a request no longer defines which documents survive.
`delete_stale_content` currently deletes any ID absent from the request, and each
save re-serializes every closed tab. Saves should write only live documents and
changed closes, and remove a document only for the explicit cases in this plan.
This also retains history when closing a split pane prunes its buffer, which today
is removed without an archive.

Record opens when a document is installed in the workspace. Carry the open intent
through asynchronous I/O so a failed load, stale result, duplicate activation, or
temporary loading preview cannot produce a false event. Batches large enough to
open as cold tabs record an event per installed shell; later hydration records
nothing. Record each affected document when a split workspace opens or closes
several files.

Record **Created** only for an explicit New action, not automatically created
placeholder tabs. Remove a never-edited blank untitled document when it closes,
without leaving rows behind.

Extend session capture with pending activity, assigning each record its sequence
before enqueueing it. Commit document metadata, edit changes, positions, and
associated activity in one transaction. `has_changes` must count pending activity,
and any recorded event must mark the session dirty, so that activity alone produces
a commit. Acknowledge only the records included in a successful capture; a delayed
completion must not clear newer pending events. Retrying a capture must not
duplicate activity.

Extend the existing transactional close path: the closed state, remaining
workspace, retained history, and Closed event commit together before removing the
document from the workspace. Audit final-view close, tab replacement, bulk close,
and split operations as well as ordinary tab close. Opening events use the normal
session durability boundary; report persistence failures and retain pending work.

## Current dirty-file behavior

This records what the application does today, as the baseline for the rules below.

- **Dirty state:** a local edit marks a file dirty. History navigation instead
  compares the text with the saved digest, so returning to saved text is clean.
- **Quit:** there is no prompt. The session is saved, and unsaved text returns on the
  next launch, so no decision about that text has been made.
- **Closing a tab or a file's final view:** the prompt offers Save changes, Discard
  changes, or Cancel. Discard on a tab still archives the text, and reopening with
  unchanged disk contents restores it as dirty. Discard on a split pane uses
  `perform_close_view`, which keeps no archive.
- **Disk changes while open and dirty:** local text stays and the file is flagged
  **Disk conflict**, with a status warning and no modal prompt. Saving then offers
  Overwrite, Reload, Save As Copy, or Cancel. Reload keeps the local text in the edit
  tree. Overwrite replaces a disk version Scratchpad never read into history.
- **Disk changes before a restart:** `refresh_startup_restore_conflicts` does not use
  the flag above. It turns the file into a recovered untitled document and may open
  the disk file separately. A clean file with history is also routed this way.
- **Disk changes while closed:** with unsaved text, `reopen_archive` keeps the text as
  a recovered untitled document and may open the disk file separately. A clean file
  uses the disk version as a new document or a forked copy.

## External changes

Add a distinct external edit source. An explicit Reload whose loaded text matches
the saved baseline, or which only discards local edits, must not be labelled as an
external change.

**File closed, then reopened.** Resolve the path to its retained document, then
compare disk contents with its last known version:

- Unchanged: restore the document as it was, including unsaved text.
- Changed: restore its tree and Now, then apply the disk text with
  `replace_exact_with_history`, sourced as external. This applies whether or not the
  file had unsaved text; that text stays reachable as the preceding state.
- Missing (`NotFound`): reopen the retained text as a recovered untitled document
  with the same identity and no new state. Retained history stays accessible.
- Other read failures: fail the open visibly and leave the retained document,
  path, and history unchanged so the user can retry.

Remove `fork_recovery_identity`, the `tab: None` result of `reopen_archive`, and
archives that remain after their documents reopen. A reopened split keeps its
identities and applies this rule to each document. Remove the `reopen_archives`
setting, because opening a path always resolves to its retained document; Forget
history remains the way to start a new tree.

**Clean file open or restored at startup.** Apply the same replacement. This
includes the startup case that currently detaches a clean file with history as a
recovered copy. A cold restored tab is compared when it hydrates.

**Dirty file open or restored at startup.** Use one rule for both, because neither
has had a decision about its unsaved text:

- Keep the path, local text, and Now unchanged, and flag the file **Disk conflict**.
- Record one **Changed on disk — local work kept** row, once per observed disk
  version, so restarting with an unresolved conflict adds no second row.
- Saving offers the existing Overwrite, Reload, Save As Copy, or Cancel prompt. An
  accepted Reload records the replacement. Overwrite replaces a disk version that
  was not read into history; this is accepted because it follows an explicit warning.
- Remove the conflict branch of `refresh_startup_restore_conflicts` and the
  `open_changed_disk_files` setting. A dirty file missing at startup keeps its
  existing recovery as an untitled document with the same identity.

External changes therefore never create a second document. Removing both recovery
settings also removes `RecoverySettings` and its two toggles; the rest of the
Settings recovery page stays.

**Detecting changes in an open file.** Today's watcher path compares only modified
time and size, and marks a dirty file conflicted without reading it. Change it as
follows:

- Treat a metadata change as a trigger to read and hash the file in the background.
- Compare the digest with the document's last observed version, and record a row
  only when it differs. A later return to earlier contents counts as a new change.
- Identical contents with new metadata, including `touch`, add no row.
- Ignore the application's own completed saves.
- Record a missing file once. When it returns, compare it like any other change.
  Today a returning file is marked in sync without being reloaded.

A file whose contents change while its size and modified time stay identical is not
detected, because reading every file on every watcher event is not justified.

## Selecting a state

Only edit rows are selectable.

- **Live document:** activate it and use the existing atomic navigation.
- **Closed document:** open that document alone through the reopen rules above,
  then navigate to the selected node. Reopening may add Opened and External changes
  loaded rows first. If navigation fails, the document stays at its opened state and
  the failure is shown. Nothing is written to disk.
- **Path claimed by another live document:** recover the document as untitled with
  its identity and tree intact, and apply no external replacement.

Run disk and database reads off the paint thread. `reopen_archive` currently reads
whole files synchronously and must move for this path. Validate document identity
and history generation again when the work completes, because Forget history can
reuse node IDs.

## Timeline interface and queries

Replace the All files toggle with Timeline. Keep the current window, This file
tree, file picker, and scrubber. Add a dedicated timeline model and renderer rather
than making the branch-tree renderer serve two ordering models.

Rows show event icon, action or edit summary, filename, and local time. Use date
separators, path disambiguation, and a closed-file indicator. Search filenames and
paths across retained documents, including documents with no text edits. Edit rows
offer a file action that switches to This file for branch inspection. This file
needs a loaded document, so using it on a closed file reopens that file and records
an open. Keep Forget history in that per-file view with its named target.

Order rows by descending global sequence and use timestamps for display. Wall-clock
changes must not reorder recorded activity. Preserve a stable row identity separate
from its sequence because a coalescing edit can move without becoming a new row.

Fetch older persisted rows with keyset pagination across both sources. Read loaded
documents' rows only from memory and everything else from the database, switching a
document's source when its close or reopen commits. Pending activity is shown ahead
of persistence, deduplicated by sequence. Avoid duplicate or missing rows during
coalescing, reopen, or Forget history. Test `UNION ALL … ORDER BY … LIMIT` in Turso
early; if it materializes the sources, run two limited queries and merge them.

Keep database work off the paint thread and cache metadata. Render visible rows
with the existing virtualized scrolling approach. Do not load every closed edit
tree or sort all retained history on every frame. Show loading and query errors
explicitly rather than presenting an incomplete feed as empty history.

Up/Down, Home/End, and Enter move between and select edit rows, skipping
informational rows; they must not silently traverse the selected file's branch.
Informational rows are styled as static text, with no action cursor or hover state,
and remain readable to assistive technology. Keep mouse selection explicit, preserve
editor focus behavior, and deliver the first typed event back to the editor.
Preserve the viewport while older pages load or new activity arrives; offer a Latest
action instead of repeatedly jumping away from the user's reading position. Each
file's Now remains visible in This file; there is no timeline-wide Now divider.

## Retention and fresh sessions

Retain documents, branches, and activity until explicitly forgotten. Forget history
for a document removes its old edit tree and its activity, then establishes a new
baseline from current contents. Invalidate query caches and in-flight targets so
deleted rows cannot reappear.

Choose a new schema version and session version, and create only the new schema for
new sessions. Keep the explicit incompatible-session reset flow, including its
existing backup and visible errors. Do not add migration code or silently
reinterpret older data. Planning and implementation do not themselves reset the
installed user's database.

## Implementation stages

Each stage leaves the application compiling, with targeted tests passing.

1. **Build the fresh persistence model.** Add the new tables and columns, document
   ownership independent of tabs, the changed save and deletion rules, allocator
   restore, and the new-version reset message. Remove replaced storage paths instead
   of keeping compatibility adapters. Done when closing a tab or split pane leaves
   its document and tree intact through restart.
2. **Use one document per file.** Resolve paths through `documents`, implement the
   reopen and startup rules, add the external edit source, and remove identity forking
   and `RecoverySettings`. Replace tests that encode the old policy, including those in
   `session_store/reopen/tests.rs` and `app_state/lifecycle_tests/retained.rs`. Done
   when a file changed while closed reopens as one document with its earlier states
   reachable, and a dirty file changed before a restart stays one flagged document.
3. **Show an edits-only Timeline.** Add the paged query and virtualized renderer,
   replace the All files path, and wire keyboard and mouse selection, including
   selection in closed documents. Keep This file and the scrubber on their current
   branch-navigation path. Done when Timeline is usable for all retained edits.
4. **Record lifecycle activity.** Add the activity table's write path, Created,
   Opened, and Closed events, and their informational rows. Integrate load/install
   and final-close hooks across file controller, workspace lifecycle, split handling,
   and session manager. Test automatic restore suppression and failed or cancelled
   operations.
5. **Detect external changes while open.** Add background hashing, the persisted
   last observed version, missing-file and return handling, and conflict rows.
6. **Complete retention and validation.** Finish Forget/reset semantics, cache
   invalidation, accessibility, regression coverage, performance measurements, and
   user documentation. Reword the unsaved-changes prompt's **Discard changes** option
   to **Close without saving**, because the text stays in history. Update README.md, SESSION-RETENTION.md, HISTORY-NAVIGATION.md,
   CHANGELOG.md, the configuration docs, and the user manual to describe implemented
   behavior.

Primary code areas are `src/app/services/session_store/`,
`src/app/services/session_manager.rs`, `src/app/services/file_controller/`,
`src/app/app_state/workspace/`, and `src/app/ui/dialogs/text_history/`. Limit domain
changes to the external edit source, history metadata, and reference validation
required by the new contracts. No piece-tree or edit-replay rewrite is part of this
plan.

## Acceptance and validation

- Open A, edit A, open B, edit B, close A, reopen A: one correctly ordered feed,
  with the same retained A history and no duplicate persisted/live rows.
- Open and close an unedited or empty file: both lifecycle entries are visible.
- Open, close, and conflict rows do nothing when clicked, hovered, or reached by
  arrow keys.
- Undo and edit a new branch: both branches appear without undo/redo events;
  selecting either edit moves only its own document.
- Close and reopen repeatedly, then restart: all lifecycle entries survive, while
  restarting itself does not add a batch of synthetic open/close events.
- Close a file, change it several times externally, then reopen it: one document and
  one External changes loaded edit, with the state before closing one step back and
  no rows implying knowledge of the intermediate changes.
- Repeat with unsaved text at close: disk text appears, and the unsaved text is
  reachable as the preceding state. With an unchanged disk file, unsaved text is
  restored as it was.
- Reopen a split containing one changed file: every document keeps its identity, and
  no edit appears twice.
- Close a file, change it externally, then select an old edit row: the exact retained
  text appears and current disk contents stay untouched. Cover a path already claimed
  by another live document.
- Change a clean open file externally: one labelled replacement with earlier states
  reachable. Change a dirty open file externally: local work remains intact with one
  row. `touch` a dirty file: no row.
- Delete an open file and recreate it with different contents: one missing row, then
  the change is handled normally.
- Quit, change a clean open file with history externally, restart: same document with
  one replacement. Repeat with a dirty file: same document and path, local text intact,
  **Disk conflict** flag, one row, and the Save prompt. Restart again before resolving
  it: no second row.
- Cover same-size external edits with changed modified time, missing/unreadable paths,
  external changes during asynchronous reads, splits, duplicate names, rename/Save
  As, cold tabs, and untitled recovery.
- Exercise failed transactions, repeated captures, delayed acknowledgements, activity
  with no other session change, and crashes around close/reopen. No committed event
  may reference a missing document.
- Test paging and coalescing during refresh, equal timestamps, clock changes,
  Forget during an in-flight query, and sequence restoration after deletion.
- Measure first-page latency, scrolling, query count, save latency, and memory with
  many closed documents and deep branching histories. Compare with the current
  History view and save path under the same workload; verify bounded pages and no
  full-payload reads to draw the feed. Run targeted tests after each stage and the
  required CONTRIBUTING.md checks, fresh reliability analysis, and Linux/Windows
  validation at completion.

The feature is complete when a fresh session supports this full sequence through
restart, both history views remain navigable, and lifecycle or external activity
cannot consume retained history or write historical contents to disk implicitly.


## Implementation validation record — 2026-11-05

The six stages are separate commits. Tests use temporary databases and files;
implementation and validation did not reset the installed user's session.

| Check | Result | Evidence in the local ignored `target/` directory |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Passed | `timeline-validated-fmt.log` |
| `cargo check --all-features` | Passed, including the final RQL practice run | `quality-runs/history-timeline-final-20261105-v3/artifacts/rust_practices.json` |
| `cargo hack check --feature-powerset` | All four combinations passed; repeated by final RQL verification | `timeline-final-features.log`, final RQL practices |
| `cargo clippy --lib --all-features -- -D warnings` | Passed; final RQL workspace Clippy also passed | `timeline-final-clippy.log`, final RQL practices |
| `cargo test --all-features` | 711 passed, 0 failed, 9 ignored, across 35 suites; library: 652 passed, 7 ignored | `timeline-validated-tests.log` |
| `cargo audit` | Passed with existing allowed dependency warnings | `timeline-audit.log`, final RQL practices |
| `cargo xtask check-turso-locks` | Passed | `timeline-locks.log` |
| Native Linux target check | Passed | `timeline-validated-platforms.log` |
| Required Windows target check | Unavailable: `x86_64-pc-windows-gnu` is not installed; combined `--require-all` command exits nonzero | `timeline-validated-platforms.log` |
| Fresh RustQualityLens | Producers passed; strict gate did not pass | `quality-runs/history-timeline-final-20261105-v3/summary.json` and `check.stdout.log` |

The final RQL executable was built from the sibling lens with `--locked` and frozen
by `cargo xtask quality`. Source hashes before and after measurement are identical;
the archived executable is unchanged. Practice evidence reports 19 passed checks,
9 skipped, no unavailable tools and no failed checks. Test execution reports no
failures. The strict gate nevertheless rejects partial macro/semantic evidence and
19 unknown inventory test statuses, and reports three pre-existing `pending-human`
review obligations: `path-key-newtype-v1`, `path-key-documentation-v1`, and
`windows-watch-unsafe-necessity-v1`. Its reliability warning inventory also includes
existing panic paths, including the two unchanged worker mutex `unwrap` calls;
partial evidence prevents complete evaluation of the configured reliability limits.
No budgets, exclusions, reviews or waivers were changed to produce a passing result.
The completion status and this documentation record were written after analysis;
production and test source remained frozen for that run. Native Windows runtime
validation is not claimed.

### Retained-history performance

The opt-in `profile_retained_history_timeline` fixture uses 16 and 256 closed
untitled documents, each with 128 recorded replacements and branching every 16
edits, plus one live document. It measures the initial metadata query after seeding,
five edited durable saves (median), 20 warm headless UI frames, 20 scrolling frames,
and process RSS. Baseline source is commit `3ecba38`, unpacked into an isolated
ignored directory; its adapter uses the original All files picker/query/renderer.
Both versions use the same fixture and debug toolchain. The old view initially
shows file headings; Timeline initially shows 100 edit rows, so these numbers
compare the implemented experiences rather than identical row contents. They are
local measurements, not latency guarantees or GPU/display-frame measurements.

| Version | Closed documents / edits | Initial query (ms) | Median edited save (ms) | Warm frame (µs) | Scroll frame (µs) | RSS (KiB) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Old All files | 16 / 2,048 | 3.607 | 6.415 | 2,700 | 2,890 | 144,296 |
| Timeline | 16 / 2,048 | 8.087 | 7.290 | 3,302 | 3,511 | 144,832 |
| Old All files | 256 / 32,768 | 44.759 | 17.005 | 2,806 | 2,824 | 211,664 |
| Timeline | 256 / 32,768 | 7.703 | 11.703 | 3,323 | 3,478 | 222,268 |

The larger fixture improves initial query and save latency. The smaller fixture
has extra query/save overhead, and the richer visible Timeline rows add frame work
and some retained memory. RSS includes database caches and allocator retention;
it is neither a peak measurement nor a count of Timeline metadata alone.

Timeline returned at most 100 records per page. Each page uses two limited source
queries; six pages use 12 queries. Five older pages took 14.030 ms for the smaller
fixture and 22.743 ms for the larger one. Old All files uses one unbounded file
metadata query and cached headings when scrolling. Unchanged Timeline frames make
no new feed queries. Regression tests verify total keyset ordering, an indexed
query plan without a temporary sorter, rendering despite deliberately corrupt edit
payloads, and warm saves that do not decode unrelated closed layouts. Closed edit
trees are not loaded to draw the feed.

Raw comparison logs are `target/timeline-baseline-performance-repeat.log` and
`target/timeline-performance-final.log`. Reproduce the current profile with:

```bash
nix develop -c cargo test --lib --all-features profile_retained_history_timeline -- --ignored --nocapture
```

The profile is intentionally excluded from ordinary test runs because it builds
a 32,768-edit temporary database and reports timings without timing assertions.
