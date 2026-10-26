# Quality review after session quarantine

Baseline: `3b93f11`. Focused follow-up to [the earlier review](rustqualitylens-refactoring-review.md); not an audit of patched dependencies or proof that every quality goal is met.

## Changes

- `domain/selection.rs`, `domain/buffer/history.rs`: domain ownership of shared selections and edit records, preserving editor reexports and egui adapters. Removed domain imports of these UI-owned types and the identical edit-record conversion.
- `domain/buffer/document/history_ops.rs`: direct range validation replaces fingerprinting and temporary vectors; removed a replay-record clone and boxed iterator, and defer checkpoint cloning until used. Retained the transactional candidate clone and recovery/conflict tests.
- `domain/buffer/analysis/inspection.rs`: normalize newline-delimited slices rather than a nested character state machine, preserving lone CRs, Unicode and borrowed fast paths.
- `shortcut_keymap.rs`, `shortcut_tooltips.rs`: one exhaustive definition dispatch replaces repeated default mappings and an unreachable fallback. Runtime defaults borrow static bindings. The remaining 43-action match is intentional compiler-checked exhaustiveness, not a hotspot hidden behind indexing or macros.
- `shortcuts/utility.rs`: ordered handlers share consumption/dispatch. Application-level tests exercise all ten actions through configured keys, including collision priority; explorer tests use untitled buffers and never launch an external application.
- Search/path workers: borrowed result sender, iterator-based request coalescing, exact nonempty batch bounds, and removal of a redundant progress counter. Preserved ordering, cancellation and channel-disconnection behavior.
- Consolidated shortcut, diagnostic and cursor tests while adding real context/cache, application dispatch, Unicode and modifier coverage. Removed three test glob imports. Session recovery, integrity, concurrency and platform fallback coverage was retained.

## Measurements

Same saved RQLens 0.1.0 binary before/after, normal `rqlens.toml`, unchanged policies. Complexity/effort/SLOC below sum **function rows only**, avoiding double counting module rows. Coverage uses the tool's existing scope; test inventory also includes integration tests.

| Metric | Before | After |
| --- | ---: | ---: |
| Cognitive complexity | 4,732 | 4,727 |
| Cyclomatic complexity | 9,629 | 9,546 |
| Function-scope Halstead effort | 28,214,794 | 28,253,372 |
| Non-test-module Halstead effort | 24,289,136 | 24,139,101 |
| Function SLOC | 60,773 | 60,601 |
| Physical Rust lines under `src` | 77,065 | 77,040 |
| RQLens nonblank source lines | 69,158 | 69,183 |
| Duplicated lines / percentage | 5,309 / 7.68% | 5,160 / 7.46% |
| Escape-hatch occurrences | 17 | 14 |
| Reliability findings | 525 | 516 |
| Inventoried tests | 578 | 560 |
| Line coverage | 64.35% | 64.53% |
| Function coverage | 66.95% | 67.32% |
| Region coverage | 63.85% | 64.06% |

Non-test-module filtering excludes `tests`/`*_tests` module and path components; it is not a compiler-proven production-only inventory. The escape-hatch reduction is three glob imports, **not fewer unsafe operations**. Coverage gains include normalization **50% → 96.88%** and utility shortcuts **26.51% → 85.53%**. Branch coverage is unavailable.

### Unmet goals and architecture limits

Aggregate effort increased with consolidated test tables despite lower non-test-module effort. Physical lines fell slightly, but nonblank source grew: no broad LOC win is claimed. Lower test count does not mean fewer behavior cases or prove that tests are better.

Automatic architecture scores changed in untouched modules as semantic resolution varied (locality **95.23 → 94.72**, leverage **15.15 → 16.46**). These are not reliable improvement evidence. An additional paired, syntax-only comparison against a clean baseline worktree gives locality **93.64 → 93.75**, leverage **21.96 → 21.81**. Existing-module leverage scores were unchanged; the three new modules lower the unweighted mean. Thus **numeric leverage improvement is not established**. Domain/view locality improves, but the domain facade remains a coupling hotspot.

Inventories remain partial around macros, cfg and unresolved references. Allocator unsafe boundaries, platform fallback allowances and history rollback protection were deliberately retained. Remaining priorities include startup orchestration, history application and the domain facade. No Windows runtime validation, GUI session or performance benchmark was performed; allocation reductions described above are structural, not measured throughput/memory claims.

## Validation and reproduction

All-feature tests passed (**498 library tests**, plus binaries/integrations), strict all-target/all-feature Clippy passed, coverage tests passed, and formatting/diff checks passed. RQLens correctness records show no compile/run failures. Its two remaining test advisories concern the intentionally ignored `abrupt_writer` subprocess helper, which the parent recovery test invokes and verifies; they were not suppressed.

Evidence: `target/quality-review/{baseline,after,baseline-syntax,after-syntax,checks}/`. Earlier exploratory passes are retained separately. Only the following producers were refreshed; other copied analysis files are not new acceptance evidence:

```sh
nix develop --command bash -c '
  for metric in hotspots clones escape-hatches reliability locality leverage coverage correctness-run test-quality; do
    target/quality-review/rqlens measure "$metric" --config rqlens.toml
  done
'
```

Syntax-only configurations are in `target/quality-review/*-syntax.toml`; they use identical scopes with `rust.identity_resolution = "disabled"`, not weakened project policy. Recreate baseline worktree `/tmp/scratchpad-quality-baseline` at `3b93f11` before repeating that comparison. Saved tool SHA-256: `14e5f995bbe2b5dd26213dd7ba1de1903af8be90e2575f062d557efa828b9f64`.
