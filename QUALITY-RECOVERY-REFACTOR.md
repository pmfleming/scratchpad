# Recovery quality review

## Measurement basis

Fresh baseline: `03f6691`, not the earlier `a3f4cd2` review in
`QUALITY-REFACTOR.md`. Tool: local `../rust-quality-lens` at `d28f3d9`, frozen
before both measurements. Raw baseline/current reports, LLVM JSON, configuration,
revision records and the executable are in `target/quality-review-03f6691/`.
Source roots: `src`, `tests`, `crates`, `benches`, `examples`, `scripts`, `xtask/src`.
Identity resolution is `auto`, with a 60-second timeout. Coverage uses locked,
workspace/all-feature verification and the same LLVM target directory.

| Metric | Before | After |
| --- | ---: | ---: |
| Function cognitive complexity, summed | 5,450 | 5,435 |
| Function cyclomatic complexity, summed | 10,630 | 10,595 |
| Function Halstead effort, summed | 35,024,048 | 34,998,585 |
| Functions with cognitive complexity >= 20 | 4 | 1 |
| Nonblank source lines | 80,950 | 80,688 |
| Duplicated nonblank lines | 5,879 (7.26%) | 5,728 (7.10%) |
| Escape-hatch findings | 17 | 15 |
| Registered tests in source inventory | 633 | 621 |
| LLVM line coverage | 35,385/53,167 (66.55%) | 35,535/52,824 (67.27%) |
| LLVM function coverage | 4,089/5,899 (69.32%) | 4,114/5,866 (70.13%) |
| LLVM region coverage | 48,075/72,511 (66.30%) | 48,268/72,084 (66.96%) |
| Mean module locality score, advisory | 88.527 | 88.592 |
| Mean module leverage score, advisory | 31.077 | 31.051 |

Function sums select `kind == "function"`; module Halstead records must not be
added again. Metrics describe authored syntax before macro expansion, not runtime
speed or a proof that all remaining code is reachable. Leverage/locality use an
incomplete observed dependency graph. **Whole-tree mean leverage is slightly
worse**, despite the concrete reuse improvements below; this is not a claim of
uniform improvement.

Coverage is partial because the quality-input fingerprint is incomplete. Branch
coverage is unavailable. LLVM includes some lifecycle tests/support in its
reported denominator. Excluding paths matching
`/([^/]*_tests|tests)(/|\.rs$)|/test_support\.rs$` from both reports still improves
line coverage: **35,020/52,802 (66.32%) → 35,189/52,478 (67.05%)**.

## Changes and rationale

- Removed the unreachable startup-conflict dialog, queued comparison actions,
  conflict descriptors and history-revalidation wrapper. The only conflict-list
  setter had become a clear operation. Automatic recovery now visits buffers
  directly instead of allocating/cloning descriptors and looking them up again.
- `BufferState::recover_as_untitled` owns detachment for startup recovery, archive
  reconstruction and archive installation. Path/key, disk baseline, freshness and
  settings-file identity now reset consistently while text/undo remain intact.
  Archive installation shares duplicate-identity checks and persistence handling.
- Retention orchestration delegates durable quarantine/rollback preparation and
  interior-version selection to narrow helpers in the same database module.
  Compaction → interior thinning → whole-archive expiry order is unchanged.
  Protected roots remain excluded; quarantine failure still aborts safely. There
  is no schema or transaction-boundary change.
- UI recovery queries return `Arc<T>` and no longer require `T: Clone`. Rendering
  reuses archive/version lists; only the selected recent disk path is cloned.
  Archive-row controls have a local renderer. Recently closed file buttons now
  expose their painted labels to accessibility clients.
- Both history views use one boundary function instead of separate forwarding
  wrappers. Disk-usage traversal delegates file accounting locally and no longer
  unwraps an optional allocation counter. Two wildcard imports were removed.
  `rqlens.toml` now caps application `unwrap` findings at zero, retaining the
  existing profiling-path exclusions.

Selected function cognitive/cyclomatic changes:

| Function | Before | After |
| --- | ---: | ---: |
| `apply_archive_policy` | 31 / 23 | 19 / 16 |
| `reopen_archived_tab` | 22 / 13 | 8 / 10 |
| `recover_archived_copy` | 9 / 8 | 3 / 5 |
| `render_recently_closed_files` | 26 / 14 | 7 / 9 |
| `scan_directory_excluding` | 15 / 16 | 10 / 13 |

Extraction does not erase work: the new quarantine, thinning, installation and
archive-row helpers have cognitive scores 2, 6, 10 and 12. The direct startup
recovery function itself rises from 7 to 11 while its obsolete surrounding
workflow disappears. Its observed outbound dependencies drop from 21 to 7, and
its advisory leverage rises from 0 to 20. The new tests also add observed incoming
edges; those must not be mistaken for additional production consumers.

## Coverage and test volume

Consolidated history preference/boundary/timeline tests, single/group drag cases,
and storage-usage fixtures. Existing distinct cases remain. The duplicate direct
text-replacement test is covered by the actual preview-button journey, including
undo and verification that disk contents were not written.

Expanded coverage exercises startup recovery with disk opening enabled/disabled
and missing files; archive pin/unpin, both recovery actions and both file-open
dispositions; path ownership and stale settings/disk metadata; preview restoration
and new-tab creation; symlink exclusion and missing/invalid settings roots; and
asynchronous cache sharing, in-flight deduplication, stale-success refresh and
cached errors. The cache fixture deliberately uses a non-`Clone` payload and
asserts pointer identity. Existing retention/quarantine/rollback tests still pass.

The volume reduction is **12 registrations**, not a test-LOC deletion: summed
registered test-function SLOC rises slightly, **11,701 → 11,763**, to accommodate
new integration journeys. Ignored tests (5 in the inventory), assertionless-test
heuristic findings (3), and total test-quality findings (8) are unchanged.

| Production file | Line coverage before | After |
| --- | ---: | ---: |
| `app_state/workspace/lifecycle.rs` | 44.53% | 52.51% |
| `app_state/workspace/restore_conflict.rs` | 67.60% | 100% |
| `session_store/database/retention.rs` | 95.38% | 95.93% |
| `session_store/usage.rs` | 92.59% | 94.34% |
| `ui/recovery.rs` | 68.33% | 90% |
| `ui/dialogs/text_history/preview.rs` | 40.67% | 46.67% |
| `ui/tab_strip/context_menu/file_actions.rs` | 0% | 34.50% |

The startup file's coverage increase includes deletion of dead paths. These are
not branch-coverage or end-to-end performance results.

## Validation and reproduction

Passed: formatting, locked all-feature check/tests, strict all-target/all-feature
Clippy, locked feature-powerset checks, Linux cross-target check, Turso lock policy,
and `git diff --check`. Root library: **544 passed, 3 ignored**; binary/integration/
doc suites also passed. Application/probe lockfiles are unchanged.

Unavailable: `cargo audit` is not installed; Windows target/toolchain is not
installed and the target-check script explicitly skips it.

RustQualityLens is **not a green repository gate**. The repository-config check
exits 1 with incomplete/incompatible/missing evidence and project-wide threshold
findings; its other pre-existing artifacts are not this review's baseline. The
isolated default check exits 0 but its `--fail-on partial` check exits 1. Do not
interpret the permissive exit code as complete evidence or certified policy
compliance.

Reproduce the paired measurements with the frozen configuration:

```sh
q=target/quality-review-03f6691
for metric in hotspots clones escape-hatches reliability locality leverage test-quality; do
  "$q/rqlens" measure "$metric" --config "$q/rqlens.toml"
done
CARGO_LLVM_COV_TARGET_DIR=target/llvm-cov-target \
  "$q/rqlens" measure coverage --config "$q/rqlens.toml"
CARGO_LLVM_COV_TARGET_DIR=target/llvm-cov-target \
  cargo llvm-cov report --json --output-path "$q/current-llvm.json"
"$q/rqlens" check --config "$q/rqlens.toml" --fail-on partial
```

The remaining archive reconstruction hotspot and untested preview replay/error
branches deserve separate work. No native Windows behavior or end-to-end latency
improvement is claimed. The sibling performance lens's unrelated work is untouched.
