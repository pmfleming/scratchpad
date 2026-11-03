# Undo and history rewrite plan

Status: implemented in seven stages, with a separate commit for each stage.
Linux validation is complete. Windows validation and the strict quality gate
have the limitations recorded below.

Replace the linear undo log and snapshot navigation with a persistent edit tree
for each file. A visible **Now** marker identifies the state currently shown in
the editor. Undo, redo, the scrubber, and History all move this same position.
Editing from an earlier state preserves the old continuation and creates a branch.

Use a new database schema and new session data. No migration, legacy import, or
backwards compatibility is required. Saved source files and settings remain
outside this reset. Creating the plan does not reset the installed database.

## Historical reference

Use `5771513`, immediately before the Turso migration work, as the reference for
the earlier navigation behavior and file grouping. Its History UI explicitly
rendered a Now divider, offered expandable file histories, and provided an
interleaved view of edits across files. Its linear history discarded the redo
tail when editing after undo; preserving branches is a new requirement.

The Turso integration in `a451948` did not change the history list, model, or
document replay implementation. The explicit Now divider was removed in
`dd9adf9`, during the October 30 UI rewrite. Snapshot navigation and later
preview/live-navigation changes added further state and interaction paths.
These findings identify design changes, not a runtime diagnosis of every defect.

## Document history

Every tracked file has a stable identity and one edit tree. Its root is an empty
document. For an existing file, the first change inserts the exact decoded
contents loaded from disk. History before the file was first tracked cannot be
reconstructed. A newly created empty document starts directly at the root.

Each committed change has a stable identifier, exactly one parent, reversible
text operations, cursor selections before and after the change, an edit time,
and a short description. A node represents the state after its change. Different
nodes may contain identical text and still represent different editing histories.
Branches share their common ancestry; they do not duplicate the whole document.

Each file also stores its Now node, selected branch tip, saved-content identity,
and last actual edit time. The selected tip identifies the single root-to-tip
route used by redo and the scrubber, including when Now is at a shared ancestor.
Now must always lie on that route. These identities survive closing and restart.

Navigation never records a new text change. Editing at Now adds a child and makes
that new child both Now and the selected tip. Existing children remain available.
There are no branch merges in this rewrite.

Typing can still be grouped into meaningful undo units. Grouping is restricted to
the active unsealed leaf; navigation, saving, branch selection, and non-typing
operations seal the group. A node with descendants is immutable. A persisted open
group must be updated atomically with its content and cursor, and is sealed on
restart. This prevents coalescing from rewriting a shared ancestor.

## Navigation contract

- Undo moves Now to its parent, retaining the selected branch tip.
- Redo moves Now to the next child on the selected route. It never chooses a
  sibling implicitly. Immediately undoing a new branch and redoing returns to
  that branch.
- Selecting a branch makes its tip the selected tip and navigates to that tip.
  Selecting a node already on the selected route preserves that route. Selecting
  a node elsewhere selects the most recently edited descendant tip of that node;
  the chosen route is visibly highlighted and remains stable until changed.
- Selecting an arbitrary state walks back to the common ancestor and forward to
  the target. Text, Now, selected route, and cursor change atomically. Failure
  leaves all of them at their previous values.
- Selecting Now is a no-op. Root and tip are valid visible positions, including
  in an empty file. Boundary undo/redo leaves the document unchanged.
- Navigation changes the live editor and recalculates whether its content differs
  from the saved file. It does not write the source file. Saving records the saved
  content identity without creating an edit or removing any branches.
- Split views share the file's Now position and content. The initiating view gets
  the history cursor selection; other views retain valid anchored selections.
- Asynchronous results validate file identity and a monotonic revision. Old loads
  cannot overwrite a newer edit, navigation action, or closed/reopened file.

For example, after edits A, B, and C, undoing to A and typing D creates two
continuations from A: B then C, and D. Now is D. Both tips remain selectable, and
each branch can be revisited after restarting the application.

## Scrubber and History

The scrubber shows one file and one selected route, from the empty root to the
selected tip. Its handle is explicitly labelled Now. Undo and redo move the
handle; dragging it moves the document through those same edit states. Choosing
another branch in History updates the scrubber's route.

History groups files and exposes their edit paths, branch junctions, and branch
tips. Expand or collapse file groups and branch sections without changing the
document. Give each file an explicit Now marker, distinguish its selected route,
and keep alternative branches reachable. Default branch labels can use their
latest change and edit time; user-managed branch names are outside this rewrite.

Use clicks and keyboard selection to navigate History. Hover shows information
without changing the document. This removes the implicit navigation that made
ordinary pointer movement change the editor. Scrubber dragging remains live.
Opening either surface does not move Now. Escape and outside clicks dismiss it
without undoing navigation already performed.

Explicitly opening History focuses its navigation. Automatically showing the
scrubber during undo/redo leaves focus in the editor. Typing while a history
surface has focus returns the input to the editor without losing the first event,
and edits at Now. An already visible History view follows undo/redo without an
extra scrubber overlay obscuring it. Scroll requests keep Now visible without
continually overriding deliberate browsing.

The all-files view contains separate file histories, each with its own Now. It
does not imply a shared cross-file undo cursor or replay unrelated files as a
single operation.

## Finding recently edited files

Provide a searchable file index in History, ordered by the most recent actual
edit. Include open files and closed files with retained history. Show the file
name, enough path information to disambiguate it, and its last edit time. Unsaved
documents participate using their stable identity and display name.

Opening, viewing, saving without changes, undoing, redoing, and switching branches
do not change the last-edit ordering. Recording a new edit does. Use a persistent
sequence as a tie-breaker so equal timestamps have stable ordering.

Selecting a closed file reopens its recorded Now state and selected branch. The
existing source-file conflict checks still apply if the disk contents changed.
Rename and ordinary Save As preserve the tracked document's identity and history;
an explicitly created duplicate receives its own identity.

## Persistence and retention

Keep Turso as the storage engine and introduce a fresh schema version. Persist
document identities and metadata, edit nodes and parent links, operation payloads,
Now positions, selected tips, saved-content identities, session/view layout, and
the recently edited file index. Commit an edit and its document position together.
Commit navigation and branch selection together as well.

Keep tree structure and navigation fields explicit in the schema. Text payloads
may use shared immutable chunks stored in the same database. Index parent links,
document identities, and recency. The file's content is reconstructible from its
root and edits; a separately serialized buffer must not become another authority
for the history position.

Remove the user-visible snapshot system, including snapshot lists, visit/return
buffers, snapshot coverage markers, and snapshot-specific replay and retention
paths. Internal materialized states may accelerate recovery or long seeks, but
are disposable caches keyed to edit nodes. They cannot change available history,
create user-visible states, or replace missing edit payloads.

Memory limits control loaded payloads, reconstructed states, and UI caches. Evict
cache entries and reload from the database rather than deleting durable edits.
Do not carry forward automatic undo truncation, payload stripping, snapshot
sampling, or age-based removal of closed-file histories. Retain all branches until
the user explicitly removes that file's history. Storage use will grow; report it
accurately and report persistence failures visibly.

Replace Clear history with an explicit per-file Forget history operation that
removes all branches and starts a new tree containing the current contents as its
initial insertion. The source file remains untouched. Keep this destructive action
out of normal navigation; remove ambiguous scope-dependent clear behavior. Branch
deletion and automatic disk-budget pruning are outside this rewrite.

On the first launch of the new implementation, a recognized old Scratchpad schema
starts a fresh session database and reports that the previous session/history was
reset. Do not attempt to import its snapshots or undo data. Close handles and hold
the ownership lock while replacing the database and journals. Never interpret an
unrelated database, corruption, a lock failure, or another I/O failure as permission
to reset. Remove obsolete migration paths after the fresh schema is integrated.

## Source file changes

Opening a previously untracked file creates its initial insertion. Reopening a
tracked file restores its tree. An accepted external reload records the new disk
contents as an ordinary replacement change from Now, preserving any alternate
continuations. It must not silently replace the tree. Existing prompts for
conflicting external changes remain applicable.

Keep text evolution separate from file encoding and save format. Preserve exact
decoded text, including mixed line endings, during replay. Encoding changes must
not clear history. If an operation actually changes decoded text, record that
change through the same edit transaction path.

## Implementation sequence

1. Establish the behavioral reference. Use the pre-Turso history code and the
   current editor integrations to capture the required Now, undo/redo, selection,
   and focus behavior. Document intentional differences: branch preservation,
   empty root, click navigation, and removal of snapshot navigation.
2. Build the independent edit-tree engine. Implement initial insertion, edit
   grouping, parent/child navigation, route selection, atomic branch switching,
   and explicit Now. Test it against a simple reference model before UI wiring.
3. Implement the fresh Turso schema and repository. Persist complete trees,
   payloads, positions, recency, and session data. Add lazy loading, transactional
   writes, restart recovery, and the recognized-old-schema reset path.
4. Route all editor changes through the engine. Cover typing, IME, paste, cut,
   replace, replace-all, external reload, save/reopen, and split-view updates.
   Remove the old undone-flag/redo-tail model and duplicate navigation authority.
5. Rebuild the scrubber and History from the shared state. Add visible Now,
   branch routes, file groups, recently edited search, and closed-file reopening.
   Replace the snapshot-specific UI and focus handling with the agreed contract.
6. Remove obsolete storage, settings, and tests. Remove snapshot visits and
   retention, old schema upgrades/imports, undo payload stripping, and the global
   timeline cursor. Update navigation documentation, settings descriptions, user
   manual, and changelog to describe the completed implementation.
7. Validate the integrated application. Run the regression suite and repository
   checks, measure performance, and exercise the desktop interactions before
   treating the rewrite as complete.

## Acceptance and validation

The core acceptance sequence is: create a file, make several edits, undo to an
ancestor, create a branch, revisit both paths, close the file, restart, and repeat.
Exact text, Now, selected route, cursor, and saved/dirty state must remain correct.

Automated coverage must include:

- Empty root, initial file import, root/tip boundaries, and same-text nodes.
- Multiple and nested branches, shared ancestors, selected redo paths, and
  coalescing that never mutates an ancestor or deletes another continuation.
- Seeded edit/navigation sequences checked against an independent text/tree model.
- Unicode, mixed line endings, large edits, replace-all, IME, and anchored cursors.
- Atomic failed seeks, stale asynchronous results, interrupted persistence, and
  restart with Now at an ancestor rather than a tip.
- Lazy loading and memory eviction while every durable branch remains reachable.
- Closed-file history, external changes, rename/Save As, and recent-file ordering.
- Visible Now at every position, branch selection, stable hover, keyboard focus,
  first-event delivery, and agreement between editor, scrubber, and History.
- Fresh-schema initialization and old-schema reset without migration; saved files
  and settings survive, and unrelated open failures never trigger a reset.
- Explicit Forget history and correct storage accounting after removal.

Run formatting, all-feature checks/tests, Clippy, dependency/feature checks, Turso
lock checks, and the available cross-target and quality checks described in
CONTRIBUTING.md. Report unavailable checks separately. Desktop validation must
exercise the actual History and scrubber on supported platforms; headless tests
alone do not establish focus and pointer behavior.

Measure ordinary edit/undo latency, long branch seeks, History rendering, restart,
database growth, and memory with long histories, many files, and large documents.
Require ordinary edits and nearby undo/redo to avoid work proportional to the
entire retained tree. Use lazy payload loading and virtualized UI rows; introduce
internal reconstruction caches where measurements justify them. Full-history
retention must not require keeping all text versions in RAM.

## Implementation and validation record — 2026-11-03

Stages 1–6 are commits `c2a1185`, `0a49593`, `f65defe`, `691d4ac`, `ed60e71`,
and `4668174`. Stage 7 adds streaming history reconstruction and capture,
persistent shared metadata, immutable file backing, bounded loaded text chunks,
incremental History row caches, corruption checks, and per-view selection restore.
The database is schema 10 and the session format is version 8.

Validation completed:

- All-feature tests: 645 passed, none failed, eight ignored. The three manual
  edit-tree performance tests were also run separately and passed.
- Formatting, all-feature compilation, strict all-target Clippy, all four
  feature combinations, Linux target compilation, and Turso lock identity checks
  passed. Dependency audit completed with eight allowed advisory warnings.
- An isolated native Linux/Hyprland session exercised undo, branch creation,
  alternate-path clicks, inert hover, the first typed character from History,
  scrubber pointer navigation, and restart at an ancestor. Text, Now, selected
  tip, alternate branches, and caret position survived restart. The existing
  user database was not used for these tests.
- Automated tests cover closed-file branch reopening, recency, explicit Forget,
  external disk changes, corrupt payloads/positions, rollback, split selections,
  large Unicode branches, eviction/reload, and shared metadata mutation.

Debug-build profiles used 1,000, 10,000, and 100,000 short edits. Nearby undo/redo
measured approximately 14 microseconds; with a background capture held, it ranged
from 14 to 23 microseconds. Capture itself measured below 0.1 microseconds. A full
100,000-node root roundtrip took 1.94 seconds. Warm History frames at 1,000 and
10,000 edits measured about 2.7 milliseconds. A 10,000-node first save took
6.76 seconds, restart 421 milliseconds, and the database occupied 9.80 MB.
These are local debug measurements, not release performance guarantees. Long
seeks and initial persistence still perform work proportional to the visited tree.

Validation limits and remaining risks:

- The Windows target toolchain was unavailable; neither Windows compilation nor
  native Windows interaction was validated locally.
- The fresh RustQualityLens run did not pass. Measurement and practice commands
  completed, with no test or practice failures. Strict checking reports partial
  macro/fingerprint evidence, three unmet required human reviews, and 17 module
  threshold findings, including History and session storage. Its partial
  reliability inventory observes 11 `expect` sites against a budget of 10 and
  two existing database-worker `unwrap` sites against zero. No budgets were raised
  and no findings were suppressed.
- One new `expect` is in temporary history-chunk loading: infallible rendering and
  export assume the private backing file remains readable. Seeks preflight I/O
  and preserve Now/content on failure; spill failures preserve the in-memory text
  and report a warning. A later backing-file read failure during an infallible
  rendering/export path can still panic. This remains a reliability limitation.

Local evidence is retained in `target/undo-rewrite-validation/` and
`target/quality-runs/undo-rewrite-stage7-final/`. The latter includes the exact
source patch, stable source hashes, commands, strict diagnostics, and summary.
