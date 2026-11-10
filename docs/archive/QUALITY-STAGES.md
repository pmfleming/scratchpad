# Follow-up quality stages

Baseline: `d8fc4c4` (the preceding recovery refactor). Each stage is committed
separately. No sibling repository changes are included.

## 1. Asynchronous reload/reopen safety

- One pending-load model and completion path for explicit reload, automatic
  refresh and encoding reopen. Removed four validated-location `expect` calls,
  redundant metadata clones and the unused reload-error wrapper.
- Requests capture document identity plus generation, not a numeric revision
  alone: replacing a document can reset that number. Capturing the revision key
  does not clone text or runtime anchors.
- Completion validates buffer ownership, requested/result paths, result cardinality,
  disk baseline and document identity. Explicit reload can discard pre-existing
  edits, but not edits made while the request was pending. Auto-refresh conflicts
  and failed-load status remain mode-specific.
- A 3-mode × 11-scenario completion matrix and real queued-load journeys cover
  edits (including edits marked clean), replacement, rename, closure, newer disk
  baseline, errors, mismatched paths, empty/extra results and duplicate requests.
- Passed: focused save tests, full locked all-feature tests, formatting and strict
  all-target/all-feature Clippy. Logs: `target/quality-stage1-tests.log`.

## 2. History preview actions

- Separated rendering from explicit restore/new-tab/replay actions. Text truncation
  is computed once per preview, not allocated every UI frame; recovery still uses
  the complete Unicode text.
- Replay checks both history revision and document identity, preventing a replaced
  document with coincidentally matching history IDs from accepting a stale action.
- Recovery mutation owns follow-focus behavior; rendering no longer restores views
  itself. Failed actions keep the preview open.
- UI journeys cover undo/redo, edits without new history, document replacement,
  closed sources, both focus policies, full-length recovery and disk preservation.
- Passed: focused history tests, full locked all-feature tests, formatting and
  strict all-target/all-feature Clippy. Log: `target/quality-stage2-tests.log`.

## 3. Persistence phases and commit-only inventory

- Preparation now names inventory refresh, history preparation and recovery-token
  filtering. Transaction orchestration names history, snapshot, metadata, version
  expiry and content cleanup phases, preserving their original order and timers.
- Each save attempt owns its inventory until commit; failure discards it and
  success explicitly returns it to the committed cache. Removed three inventory
  `expect` calls without cloning inventory sets. No schema/transaction change.
- Extended the existing rollback matrix from six to eleven write boundaries,
  including recovery versions, closed archives and final blob collection. Tests
  assert cache invalidation before any reload can mask it, unchanged committed
  data, same-generation retry and successful cache acceptance.
- Passed: 61 database tests (2 existing ignored probes), full locked all-feature
  tests, formatting and strict all-target/all-feature Clippy.
  Log: `target/quality-stage3-tests.log`.

## 4. Shared startup policy

- Blocking and background startup share settings adoption, fallback workspace,
  focus and file-target/notice policy. Execution remains explicitly blocking or
  background; progressive restore/hydration and save fences are unchanged.
- Settings-file values take precedence over legacy session values in both the
  streamed-start and terminal-restore paths. Explicit restore and file-target
  options continue to override saved preferences.
- Three scenario matrices exercise both modes with saved/missing/broken sessions,
  settings precedence, explicit overrides, file targets and failure fencing.
  The existing initial untitled-buffer behavior is preserved (not silently removed
  when opening files); streamed-prefix and close/drop safety tests remain passing.
- Passed: startup matrices, lifecycle restore tests, full locked all-feature tests,
  formatting and strict all-target/all-feature Clippy.
  Log: `target/quality-stage4-tests.log`.

## Follow-up: view-only copy-on-write

Final review found that storage-pointer identity was too conservative for UI
requests: view-anchor COW can change it without changing text. Commit `649dfe1`
uses a document identity plus text generation for reload/preview guards instead.
Clones and whole-text replacements receive fresh document identities; persistence
snapshot keys retain their original semantics. The matrices now also assert that
anchor-only COW permits completion/replay. Full all-feature tests and strict
all-target Clippy pass (`target/quality-content-identity-tests.log`).

## 5. Fresh, fail-closed quality evidence and CI

- `cargo xtask quality <rqlens-executable> <new-output-directory>` freezes the
  executable, copies policy into an isolated configuration, records toolchain and
  tool/source hashes, and runs `measure all`, `verify`, then strict `check`.
  Existing directories are rejected; old `target/analysis` reports are never
  merged. Dirty source is identified, not claimed to be a reconstructible archive.
- Summary publication distinguishes unavailable tools, execution failures, partial/
  missing/invalid/unknown artifacts and the engine's rule/test/practice diagnostics.
  Even successful commands cannot certify partial required reports, a malformed or
  failing gate, or source/tool changes during measurement. Stdout and stderr are
  separate so gate JSON stays machine-readable. RustQualityLens still owns metric,
  schema, policy and fingerprint semantics; this task does not reimplement them.
- Declared assets and standalone Rust source roots in `rqlens.toml`, enabled locked
  verification and reduced the expect budget from 14 to 12. Broader scanning exposes
  two pre-existing Windows watcher expects and four undocumented unsafe blocks;
  these findings are **not** suppressed or described as repaired.
- Nix now supplies cargo-audit and workflow validators. CI audits application and
  xtask lockfiles, preserves audit JSON, runs all-target Clippy on native Windows
  and Linux, and requires both cross-check targets rather than silently skipping
  Windows. Native Windows execution is configured, not claimed locally tested.
- Expensive strict RQL evidence runs on manual CI dispatch using tool commit
  `d28f3d936fa3c0d4c0a7ea753f436517077cb37d`, uploading reports even on failure.
  Normal PR correctness/security jobs remain independent. No `continue-on-error`
  disguises the strict gate. Aggregate heuristic score thresholds are not enabled
  by the task; explicit rule, evidence, practice and test policies are enforced.
- Thirteen xtask tests pass, including command/report classification, configuration
  preservation, exclusive output creation, source hashing, and a summary-publication
  matrix covering operational/evidence/gate/input failures. Actionlint, shellcheck,
  locked xtask Clippy, Turso policy and the xtask dependency audit pass. The required-
  target negative test exits unsuccessfully as intended.

### Evidence results and limitations

The fresh run at `target/quality-stages-final/` successfully executed measurement
and verification with unchanged tool/source hashes. Verification reports **18
passed, 9 skipped, 1 unavailable (MSRV)**, with no failed checks. Dependency audit
now executes successfully in `nix develop`. Root all-feature tests pass; the RQL
catalog still has unknown statuses for standalone/non-root tests and incomplete
macro/fingerprint evidence. Its configured-rule evaluations remain incomplete.
The strict gate returns **failure**, not a green quality certification. The final
runner is also exercised at `target/quality-stages-final-v2/`; both runs returned
failure with unchanged input/tool hashes and no observed test/practice failures.
These runs precede the anchor-only follow-up. Post-commit evidence is collected in
`target/quality-stages-committed/`; use that summary for the final source revision.

Paired application line coverage from the preceding refactor and the first fresh
stage-5 run: **35,535/52,824 (67.27%) → 36,039/52,871 (68.16%)**. This is partial LLVM
line evidence, not branch coverage or a performance result. Per-file changes:

| File | Before | After |
| --- | ---: | ---: |
| `file_controller/save/reload.rs` | 31.72% | 94.68% |
| `ui/dialogs/text_history/preview.rs` | 46.67% | 96.63% |
| `session_store/database/persist.rs` | 99.75% | 99.55% |
| `app_state/startup_state.rs` | 65.13% | 92.39% |

Selected cognitive complexity: preview `show` **17 → 2**, `prepare_changes`
**15 → 0**, `write_prepared` **14 → 0**, `build_app` **17 → 3**, runtime restore
completion **12 → 8**. Extracted helpers still carry real work. The additional
safety matrices and evidence tooling increase source/test volume; these five
stages are not presented as an aggregate LOC or complexity reduction.

Only the standalone xtask lockfile changes. Application and Turso contract locks
are unchanged; sibling repositories remain untouched.
