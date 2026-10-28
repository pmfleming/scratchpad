# RQLens style-guide pilot — step 4

## Status and ownership

The separately built **candidate and distinct final implementation both pass the
same unchanged 25-case consumer probe**. The candidate-driven documentation
improvement is implemented, tested, and confirmed by the final focused root
measurement. Consumer validation is complete; main still owns campaign acceptance,
full stable validation and commits. Candidate evidence is not relabeled as final.

Started from clean Scratchpad `72e2f2a`. The first invocation waited approximately
35 minutes without readiness and reported preparation only. Main subsequently
authorized overlapping candidate work from a consistent copied source snapshot.
No live lens file was edited or rebuilt. No commits were made. Root budgets,
exclusions, waivers, `rqlens.toml` and tracked lockfiles are unchanged. This report
is ignored by `/docs/` and requires coordinator force-add.

Changed paths:

- `scripts/check-rqlens-api-policy.py`: durable real CLI/policy/SARIF probe.
- `src/lib.rs`: crate, module and reexport documentation; no runtime changes.
- `docs/rqlens-style-step-4.md`: this report.

## Candidate identity and isolation

Ignored evidence lives under `target/rqlens-style-step-4/candidate/`. Lens base
revision was recorded in `lens-revision.txt`; the workspace includes uncommitted
candidate changes. Candidate executable SHA-256:

`8850edc6bf7267a3ba3a779ceef28d8bc8711c646c1c300433c5b54cb8f70f5b`.

Copied root Cargo files, `src/`, `rust_helpers/` and `contracts/` into
`candidate/lens-snapshot-1/`. The snapshot exactly matches the 163 captured
pre-copy source hashes; those captured live files also matched immediately after
copying. Full snapshot inventories before build, after build and after the pilot
match (`live-before-1.json`, `live-after-1.json`, `source-after-build.json`,
`source-after-pilot.json`). Build outputs are outside the snapshot source tree.

Compiled **both frontend and helper binaries from the copy**, without running a
build against the live lens checkout:

```sh
b="$PWD/target/rqlens-style-step-4/candidate"
s="$b/lens-snapshot-1"
CARGO_TARGET_DIR="$b/build" timeout 240 cargo build --offline --locked \
  --manifest-path "$s/Cargo.toml" -p rust-quality-lens --bin rqlens \
  -p rust-quality-lens-helpers --bins
cp "$b/build/debug/rqlens" "$b/rqlens"
```

Build exit **0**. All measurements explicitly select the copied helper manifest;
a frontend-only copy can otherwise keep using mutable checkout helper sources.
Rust/Clippy are 1.95.0 / 0.1.95 on Linux x86_64. No binary identity equivalence
with a future final build is claimed.

## Stable rules and counting/scope contract

The frozen registry supplies these stable IDs:

| Rule | Counting/evaluation unit |
| --- | --- |
| `rust.api.public-docs` | One production source **file** with missing public item/member and/or actual Cargo-root docs; missing-item totals are details |
| `rust.api.public-debug` | Exact projected `missing_debug_implementations` diagnostic, library-only profile scope |
| `rust.api.must-use-candidate` | Exact projected `clippy::must_use_candidate` diagnostic |
| `rust.api.must-use-self` | Exact projected `clippy::return_self_not_must_use` diagnostic |
| `rust.imports.wildcard` | Exact projected `clippy::wildcard_imports` diagnostic, with the installed Clippy's real exceptions |

Native known positives can have complete counts. A failing compiler invocation
retains real observed positives but cannot establish complete absence across
remaining lints/packages/targets: policy total stays `null`/incomplete while
`observed_findings` and SARIF retain the same stable observation. Repaired compiler
negatives require an actually executed **successful** selected profile command;
the final contract additionally requires registry-tested stable Rust/Clippy for
certified negatives. Other releases may retain recognized positives without
negative certification. No selected profile, disabled/unselected rule backend or unavailable tool/lint
must become a clean numeric zero.

## Finalized consumer probe

```sh
b="$PWD/target/rqlens-style-step-4/candidate"
s="$b/lens-snapshot-1"
CARGO_TARGET_DIR="$b/build" timeout 600 python3 scripts/check-rqlens-api-policy.py \
  "$b/rqlens" "$b/probe-finalized" \
  --helper-manifest "$s/rust_helpers/Cargo.toml" \
  --blocking-rules "$s/contracts/blocking-rules.json"
```

**Candidate exit 0; all 25 cases PASS.** New output directory required per run.
Every individual CLI command is bounded to 180 seconds; fixture verification
commands use 60-second timeouts. Fixtures contain real source/manifests/offline
locks; no artifact is fabricated or edited into a clean report. Expected policy
failures and abstentions are assertions, not permanent script failures.

Coverage:

- Seven positive → repaired negative journeys: top-level docs, crate-root docs,
  field/method/variant docs, public Debug, must-use candidate, Self-returning
  method and module wildcard imports.
- Each journey checks stale evidence after both a violating and a genuinely
  passing state, then regenerates final passing evidence.
- Hidden items/members and syntactically test-only public helpers; three missing
  members are correctly one violating file, with all three details preserved.
- Genuine inline-test `super::*`, enum variant, prelude and public reexport import
  exceptions; library Debug scope with an undebuggable binary-only type.
- Missing native/compiler artifacts; all four compiler rules without a profile;
  selected application profile not enabling Debug; simulated missing Clippy.
- Real `include_str!` documentation unsupported by the native backend, never
  silently interpreted as zero missing docs.
- A real unavailable Cargo graph with custom library root, standard documented
  binary root and unreadable path dependency. This guard derives from main's
  discovery, not an independent claimed discovery. Policy abstains and SARIF
  refuses incomplete provenance.
- Shared physical source mounted through hidden/test and production contexts.
  A scope-certified production mount must report the violation; unresolved
  ownership must be explicitly incomplete, never discarded as hidden/test-only
  or certified clean. Repairing literal docs cannot itself fabricate ownership
  proof. Candidate conservatively abstains before and after repair in both cases.

For actual fresh evidence, the script compares policy/SARIF stable IDs, canonical
finding identities, coverage and counts. Compiled results retain original
compiler identities, package and compiled scope. File-level native results have
no invented item line region. Unknown scopes require `observed: null` and
`evaluation_complete: false`, not zero. The same API coverage view must agree with
the actual policy evaluation, including staleness. Successful negatives must be
real producer/backend outcomes rather than absence of diagnostic metadata.

Finalized probe SHA-256 (must remain unchanged for final rerun):
`28e43c75667615fd20a1c4d03288c7480167a14f59762eca7e8182b2a5e225f8`.

### Preserved candidate failures / harness corrections

All exploratory logs and summaries remain; none were deleted or relabeled green:

1. `probe/` exited 1 because a **probe-local variable shadowing error** compared
   SARIF rule coverage with the entire native file inventory. Actual policy and
   SARIF rule coverage already agreed. Renamed the local inventory variable;
   assertions were not removed.
2. `probe-revised/` aborted when rustfmt tried resolving a shared module before
   fixture setup created its file. Fixed setup ordering and retained the failure.
3. `probe-complete/` exited 1 on an **overstrict exploratory requirement** for
   complete shared-mount ownership. Candidate correctly returned unknown scope,
   not a false clean. The finalized regression accepts only two safe outcomes:
   scope-certified real positive, or explicit ownership abstention. It never
   accepts a clean positive or converts unknown ownership into zero. This aligns
   with the published native limitation, rather than demanding step-5 semantics.
4. `probe-contract/` passes; `probe-finalized/` additionally uses the final shared-
   scope assertions and passes all 25. Only the latter probe version is the one
   to run unchanged for final acceptance.

These are not claimed new lens defects. The preparation-time source concern
about API-rule freshness was corrected in the shared guard before the candidate
snapshot; the frozen probe's stale-negative tests now pass. Main's shared-mount
false-clean is guarded without replacing unavailable evidence with fake clean
fixtures.

## Actual Scratchpad measurement and focused improvement

Used an isolated absolute-path copy of Scratchpad's configuration, preserving
all original source roots, fingerprint inputs, budgets and exclusions, with the
copied helper and output paths. Added only an opt-in zero-budget
`rust.api.public-docs` rule to this **isolated** config. No root policy was relaxed.
No full root practices/MSRV/platform/coverage campaign was replayed.

```sh
b="$PWD/target/rqlens-style-step-4/candidate"
CARGO_TARGET_DIR="$b/build" timeout 180 nix develop -c "$b/rqlens" \
  measure api-health --config "$b/rqlens.toml"
CARGO_TARGET_DIR="$b/build" timeout 60 nix develop -c "$b/rqlens" \
  check --config "$b/rqlens.toml"
```

Producer exits **0** before and after. Policy exits **1** before and after, as it
must for remaining incomplete/missing evidence. The candidate observed a concrete
native file finding at **`src/lib.rs`**: all three public declarations lacked docs,
and the actual Cargo library root lacked crate documentation.

Added meaningful crate guidance for shared desktop/test/benchmark/profiling
consumers, descriptions for the editor and profiling modules, and documentation
for the `ScratchpadApp` reexport. Explicitly avoids promising a stable embedding
SDK, all-platform support or isolated attribution of process-wide counters.
`#![forbid(unsafe_code)]`, exports, signatures and executable behavior are unchanged.

| Candidate observation | Before | After |
| --- | ---: | ---: |
| `src/lib.rs` known public obligations | 3 | 3 |
| Documented obligations | 0 | 3 |
| Missing obligations | 3 | 0 |
| Missing Cargo-root documentation | true | false |
| That file's native scope | complete | complete |
| Retained root native file observations | 15 | 14 |

The whole-root count remains **unknown**, not 14 certified total violations.
Root fingerprint confidence is incomplete because Cargo metadata did not resolve
the complete project/helper path-dependency graph; many generated/conditional/
ancestor scopes also remain unsupported. Native policy reports
`status: incomplete`, `observed: null`, and the provenance limitation rather than
passing. Existing configured reliability/license evidence is not manufactured.

The earlier candidate review also saw three missing literal function docs in
`src/app/app_paths.rs`, but that file's ancestor scope was unknown. Chose the
actual observed `src/lib.rs` file finding for the improvement instead; no speculative
path-query change or extra production exclusion was added.

## Validation performed on unchanged-behavior documentation change

| Command / scope | Actual result |
| --- | --- |
| Finalized candidate 25-case probe | 0; all 25 pass |
| Candidate native API producer before / after | 0 / 0 |
| Candidate root configured policy before / after | 1 / 1; incomplete, never advertised green |
| `nix develop -c cargo fmt --all -- --check` | 0 |
| Bounded locked all-feature `profile::process_metrics::tests` | 0; **2 passed** |
| Bounded locked all-feature `--doc profile::process_metrics` | 0; **2 passed**, including compile-fail result-use contract |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps --lib` under Nix | 0; public root/module/type links validate |
| `git diff --check` | 0 |

Focused tests/rustdoc have 120-second outer bounds; no timeout was reached.
Current `src/lib.rs` SHA-256:
`f5c5b1ccd7b23b85a6efc81a52d6d9f9feddedf799f0e83ca750e69ce132010e`.
No need to repeat expensive unchanged application tests for the final tool swap.

## Final frozen-tool validation

The required `/tmp/rqlens-workers/step4-ready` marker arrived, announcing frozen
core/helper sources and the independently validated immutable archive. Copied
`/tmp/rqlens-workers/step4-frozen/` frontend and workspace/helper/contract inputs
into **separate** ignored `target/rqlens-style-step-4/final/` paths. No live checkout
was rebuilt or mutated. The archive and copied sources passed its supplied
source-manifest checks before the pilot; copied sources passed again afterward.
Consumer probe and `src/lib.rs` hashes remained unchanged from the candidate run.

Final frontend SHA-256:
`19e06f205147bdbbfa628fa8a1ca4ca9cf02c7db2f00c3eb70fff6d5d235248f`.

Final source-manifest SHA-256:
`92fe5d4e1d516bcdf053958c67251d861513e8ed6dca07154f079ebe9409bf27`.

These identities are **not** the candidate frontend identity. Final inputs and
readiness/interface/freeze metadata are preserved under `final/`; the copied
helper manifest is `final/lens-snapshot/rust_helpers/Cargo.toml`. Helper execution
builds only from that snapshot into its separate `final/build` cache.

```sh
b="$PWD/target/rqlens-style-step-4/final"
s="$b/lens-snapshot"
CARGO_TARGET_DIR="$b/build" timeout 600 python3 scripts/check-rqlens-api-policy.py \
  "$b/rqlens" "$b/probe" \
  --helper-manifest "$s/rust_helpers/Cargo.toml" \
  --blocking-rules "$s/contracts/blocking-rules.json"
CARGO_TARGET_DIR="$b/build" timeout 180 nix develop -c "$b/rqlens" \
  measure api-health --config "$b/rqlens.toml"
CARGO_TARGET_DIR="$b/build" timeout 60 nix develop -c "$b/rqlens" \
  check --config "$b/rqlens.toml"
```

Actual final results:

| Final command/observation | Result |
| --- | --- |
| Unchanged 25-case consumer probe | **0; all 25 PASS** |
| Root native API measurement | **0**, current source report published |
| Root configured check | **1**, correctly incomplete |
| `src/lib.rs` native file scope | complete, known Cargo `lib` root |
| Public items / documented / missing | **3 / 3 / 0** |
| Missing Cargo-root documentation | **false** |
| Root native retained file observations | **14**, total still unknown |
| Both aliased hidden/test + production mount cases | **incomplete before and after literal doc repair**, never false clean |
| Probe/source and frozen source-manifest verification | unchanged / pass |
| `git diff --check` | 0 |

Final root native policy remains `status: incomplete`, `observed: null`,
`observed_findings: 14`; the recorded reason is
`API evidence has stale/incomplete provenance`. Its actual fingerprint read error
is `Cargo metadata did not resolve the complete project/helper path dependency
graph`. The final tool therefore confirms the local source-documentation repair
without converting remaining unknown root scope into a verified zero or a green
repository gate. No expensive unchanged application tests were replayed for this
tool-only rerun.

No remaining new consumer defect was found by the final probe. Preparation-time
freshness and coordinator-reported metadata/mount failure modes have durable real
CLI regressions; original candidate/harness failures remain available rather
than overwritten. Main's readiness reports 253 passing workspace tests, nine
existing ignored tests, and passing Clippy/rustdoc/format/bundle validation; those
are main/worker results, not tests rerun by this consumer.

Missing native Windows runtime validation, existing macro/metadata uncertainty,
source thresholds and full-project gate limits remain explicit. No alternate
compiler-release matrix, arbitrary wrappers or every Google import exception was
certified here. A scoped native doc result or successful selected lint invocation
is not semantic API reachability, whole-guide compliance or proof of design/
safety. Main's final review, both workers' accepted outputs and required stable
validation remain prerequisites to committing. This worker made no commits.
