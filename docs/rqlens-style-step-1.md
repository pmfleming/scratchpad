# RQLens style-guide pilot — step 1

## Scope and reproducibility

Consumer checkout started clean at `60f639182e8c9830fd08e4c0f6d3d263e6ba45df`.
Read the sibling lens's `docs/style-guide-implementation.md`; this invocation
covers **explicit enforcement only**, not the remaining five campaign steps.
No lens sources, policy thresholds, exclusions, waivers or lockfiles were changed.
No commits were made. The existing `/docs/` ignore rule also ignores this report;
the coordinator must explicitly force-add `docs/rqlens-style-step-1.md` when
committing (the ignore rule was not broadened).

Ignored raw evidence, copied configuration, executable and logs are under
`target/rqlens-style-step-1/`. The copied configuration preserves every Scratchpad
rule and verification option; only project/output paths are made absolute and
isolated from `target/analysis`. `missing.toml` changes only the output directory
again, to test an empty evidence store.

The existing sibling executable initially lacked `enforce`. Built it with:

```sh
cargo build --locked --manifest-path ../rust-quality-lens/Cargo.toml \
  -p rust-quality-lens --bin rqlens
```

The sibling repository was based on `d28f3d936fa3c0d4c0a7ea753f436517077cb37d`
with in-progress campaign changes. Other workers rebuilt its executable during
this pilot. Final runs use the frozen copy, SHA-256:
`5610934faff7e427dc015f4dc5864b9591092ef23a99e69dc05332fe16741e69`.
Early discovery reports are not a controlled frozen-tool comparison; they remain
available in `before/`. Rust is 1.95.0, Linux x86_64, LLVM 21.1.8.

## Commands and outcomes

For commands below:

```sh
q="$PWD/target/rqlens-style-step-1/rqlens"
c=target/rqlens-style-step-1/rqlens.toml
```

| Command | Exit / outcome |
| --- | --- |
| Sibling Cargo build above | 0 |
| `rqlens enforce --help` after build | 0; distinguishes practices-only from a full-project pass |
| `rqlens measure hotspots --config "$c"` | First attempt 1, input drift; retry and frozen after-run 0 |
| `rqlens measure escape-hatches --config "$c"` | 0; 15 records, used as supplementary discovery |
| `rqlens measure reliability --config "$c"` | 0 before/after; final run also under `nix develop -c` |
| `timeout 360 nix develop -c "$q" enforce --practices-only --config "$c"` | Final run 1 **with a fresh policy report**, as expected for this repository |
| `timeout 360 nix develop -c "$q" verify --config "$c"` | 0 despite incomplete MSRV evidence; explicit evidence-only warning printed |
| `"$q" check --config "$c" --fail-on practice-failure` | 1; evaluates stored artifacts, does not repair them |
| `timeout 360 nix develop -c "$q" enforce --practices-only --config target/rqlens-style-step-1/missing.toml` | 1 on both frozen-tool attempts, input drift before publication; see integration issue below |
| `"$q" check --config target/rqlens-style-step-1/missing.toml --fail-on practice-failure` | 1; all six configured rules retained, missing practices/reliability evidence explicitly blocks |
| `nix develop -c cargo test --locked --all-features --lib app::services::file_controller::open::tests` | 0; **6 passed**, including duplicate opens and staged/background hydration |
| `nix develop -c cargo fmt --all -- --check` | 0 |
| `git diff --check` | 0 |

Practices execute bounded child commands using the unchanged configured
1,200-second per-command timeout; outer enforcement/verification runs were also
bounded. No timeout was reached. Initial attempts used 420/480-second outer
bounds; final artifacts/log names use `final-*` / `enforce-final.*`.

The final practices summary is **18 passed, 9 skipped, 1 unavailable, 0 failed
errors/warnings**, 28 checks total. Passed checks include locked workspace,
all-feature Cargo check/tests, Clippy with warnings denied, doctests, rustdoc,
rustfmt, four feature-powerset checks, and RustSec audit. This is the configured
scope (`all_targets = false`), not an all-target validation claim.

A first focused file-watch test filter selected zero tests; it was not counted as
validation and was replaced by the six-test open-controller run above. A standalone
`cargo test --locked --manifest-path crates/windows_file_watch/Cargo.toml` exited
101 because that standalone crate has no lockfile. No lockfile was generated to
work around this. Windows std/tooling is absent; root Linux tests do not compile or
execute the Windows FFI branch.

## Findings and focused improvement

Fresh reliability findings identified four Windows change-notification calls
without safety rationales. Added source-linked, substantive `SAFETY:` comments
in `crates/windows_file_watch/src/lib.rs` explaining:

- exclusive ownership and eventual release of valid notification handles;
- lifetime/NUL termination of the UTF-16 path passed to the synchronous API;
- same-thread ownership preventing concurrent close during rearm;
- nonempty, at-most-64 handle arrays and their lifetime during a wait.

This is a documentation-only change: no control flow, FFI signature, flags,
timeouts, ownership or runtime behavior changed. Existing application tests and
practice checks remain passing in their supported Linux scope.

| Reliability inventory | Discovery before | After |
| --- | ---: | ---: |
| `rust.safety.undocumented-unsafe` | 4 | 1 |
| Total records | 989 | 986 |
| `unwrap` / `expect` / explicit panic records, all inventoried scopes | 857 / 110 / 18 | 857 / 110 / 18 |

These are authored-syntax inventories, including tests and profiling code, not
application policy counts or proof of safety. All four actual call sites now have
rationales. The residual finding is a measurement limitation, described below.
The existing application-policy evaluation still observes 14 eligible `expect`
findings against maximum 12; no attempt was made to disguise these invariants or
change budgets. Partial evidence prevents a complete rule evaluation.

`CONTRIBUTING.md` now explains the direct combined command, its expense and lack
of the `xtask` archive, evidence-only `verify`, stored-evidence `check`, and the
limited regeneration scope of `--practices-only`. The existing fail-closed `xtask`
workflow remains intact.

## Enforcement result and integration issues for the coordinator

**Scratchpad is not a green quality gate.** The final combined command generates
fresh evidence, reports `passed: false`, and exits 1. Its final reliability and
practices artifacts have identical input fingerprints. Configured rules remain
active; the failure categories are `configured-rules` and `practice-failure`.

1. **MSRV evidence unavailable despite successful host checks.** The dependency
   MSRV probe executes unfiltered `cargo metadata --format-version 1 --offline`.
   It fails trying to download `accesskit_ios v0.2.2`, a non-host dependency absent
   from the local cache. The practices report correctly marks
   `rust.project.msrv-compatible` unavailable and evidence partial. Audit and host
   builds/tests pass. Recommend explicit target scope or documented cache-fetch
   prerequisites, not converting this result into a pass.
2. **Freshness failures need actionable diagnostics.** Initial measurements and
   practices attempts returned only `quality inputs changed during measurement`.
   Early stored artifacts showed changed helper/source digests and file counts;
   concurrent executable/helper builds confound those attempts. More importantly,
   both practices-only runs against the separate empty output directory failed
   the same way even with the frozen executable and no intervening source edits.
   No practices or policy report was published there by enforcement (the later
   explicit `check` created a missing-evidence policy report). The primed main
   output succeeds in publication. Root cause is not established. Recommend
   reproducing empty-output consumer runs and reporting before/after fingerprint
   components, rather than recommending repeated blind reruns. Do not remove the
   fail-closed snapshot check.
3. **Safety false positive for a four-line rationale.** The remaining finding at
   `crates/windows_file_watch/src/lib.rs:227` is `WaitForMultipleObjects`. Its
   immediately preceding four-line rationale starts at line 223 with `SAFETY:`.
   The current native scanner searches only three preceding lines. This is not an
   undocumented call; retain the rationale and fix item/comment association in
   campaign step 2 instead of shortening comments merely to satisfy the scanner.
4. **Macro limitations block configured rules.** Reliability confidence remains
   partial for unexpanded authored macros (theme/accessor/counter/criterion and
   other generated items). Both zero-budget safety rules and the license rule
   therefore remain incomplete even where their observed findings are zero.
   The attempted stale-evidence probe also reported `Partial`, not `Stale`, so
   this consumer run does **not** establish stale-report discrimination. Missing
   evidence is separately confirmed as blocking. Preserve these abstentions;
   expose freshness and scope limitations independently where possible.

## Limits and next steps

No full `enforce`/coverage/performance campaign was run: step 1 used focused native
measurements, bounded practices regeneration and existing tests. No application UI
was launched. No positive Scratchpad enforcement outcome is claimed, nor native
Windows validation, complete generated-code coverage, or API/style-guide
compliance. Passing-command versus failing-gate behavior is demonstrated by
`verify` versus `enforce` on the same real repository; a positive complete gate
must be covered independently by the lens worker's controlled fixtures.

Coordinator should review the three changed paths, incorporate the integration
findings in the lens repository, and commit this step only after independent
validation. Rebuild/freeze the final reviewed tool before later campaign steps.

### Coordinator follow-up before commit

The final stable RQLens build adds actionable before/after fingerprint-component
diagnostics on publication drift, with a passing source-mutation regression test.
A bounded rerun of `nix develop -c <final-rqlens> enforce --practices-only --config
target/rqlens-style-step-1/missing.toml` successfully published practices and policy
into the previously empty output store. Exit 1 reflects unavailable practice /
missing reliability evidence, not a publication-drift error. The earlier drift
was not reproduced with the stable tool; no freshness protection was removed.
Full RQLens enforcement was also corrected to run practices explicitly because
`measure all` excludes that producer. The worker's findings informed both fixes.
