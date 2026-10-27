# RQLens style-guide pilot — step 3

## Result and scope

Completed the independent consumer pilot against a frozen ready implementation.
**All 16 isolated profile/CLI/gate cases pass.** Actual Scratchpad Google-v1
findings drove two reviewed result-use contracts and better counter-semantics
coverage. Selected-profile diagnostics decrease **25 → 23**; existing baseline
Clippy still passes. The remaining profile findings are retained, not suppressed.
No new lens defect was found by this consumer matrix.

Started clean at Scratchpad `4335c17`. Preparation initially waited about 15
minutes without readiness and was reported pending; resumed only after
`/tmp/rqlens-workers/step3-ready` arrived. No lens edits or commits were made.
Root `rqlens.toml`, budgets, exclusions, waivers and tracked lockfiles are unchanged.
This report is ignored by `/docs/` and needs coordinator force-add.

Changed paths:

- `scripts/check-rqlens-profiles.py`: durable public-CLI consumer regression.
- `src/profile/process_metrics.rs`: two must-use contracts, API documentation,
  positive/compile-fail doctests and expanded existing delta test.
- `docs/rqlens-style-step-3.md`: this report.

## Frozen implementation and public interface

Ignored evidence: `target/rqlens-style-step-3/`. Lens base revision:
`50de84dda8d39bb965fac9f407eea5f56d15f0f8`, with ready step-3 changes not yet
committed. Frozen executable SHA-256:

`8aa3c43bc03338cbb2f3b9f1b4bfc8a1e92edcb9787bcab702e853c5b4568aa2`.

Copied the executable and a workspace-preserving snapshot of lens `Cargo.toml`,
`Cargo.lock`, `src/`, `rust_helpers/` and `contracts/`. All 158 snapshot file hashes
in `source-sha256.json` were verified unchanged after validation. A binary-only
copy would still prefer mutable checkout helper sources, as discovered in step 2.
Every consumer config points explicitly at the frozen helper manifest.

Current interface:

```toml
[verification]
lint_profile = "google-v1" # or application-v1 / library-v1; absent preserves baseline
```

`rqlens lint-profiles [PROFILE]` exposes the finite registry. Registry identity:
`rqlens.curated-lint-profiles`, version **1**, digest **9c7e8df2f5ca97cf**. Profile
IDs and profile versions are recorded separately in the executed check. Installed
backend evidence is **rustc 1.95.0**, **clippy 0.1.95**, Linux x86_64, LLVM 21.1.8;
`tested_backend_version_match` is true. Registry metadata is not cryptographic
attestation or proof of diagnostic equivalence on another compiler release.

Opt-in adds **`rust.official.clippy-profile`**; it does not replace or silently
change **`rust.official.clippy`**. Selected check evidence includes its exact
planned command, execution-attempt status, requested individual lints, observed
default levels, backend inventory/versions, Cargo target/features scope, inherited
environment and authored local lint attributes. Local attributes are observations,
not compiler-resolved per-item suppression provenance.

## Repeatable isolated consumer matrix

```sh
s=target/rqlens-style-step-3
timeout 480 python3 scripts/check-rqlens-profiles.py "$s/rqlens" "$s/probe" \
  --helper-manifest "$PWD/$s/lens-snapshot/rust_helpers/Cargo.toml"
```

**Actual exit: 0; all 16 cases PASS.** Use a new output directory on rerun. The
script copies the executable, generates dependency-free Cargo fixtures, formats
them, generates offline locks, and runs real bounded
`enforce --practices-only`. It asserts actual practices status, diagnostics,
policy report and command exit—not merely registry metadata. Each invocation is
bounded to 180 seconds, with 60-second fixture child-command timeouts. Registry,
real tool versions, fixture source, logs and complete per-case evidence are kept.

| Case | Actual enforce exit | Result checked |
| --- | ---: | --- |
| Default compatibility | 0 | No profile check/identity; original baseline command preserved; dbg example passes baseline |
| Application clean | 0 | Selected check and policy pass |
| Application dbg failure | 1 | Baseline passes; selected `clippy::dbg_macro` error and policy failure |
| Library clean | 0 | Selected check and policy pass |
| Library missing Debug | 1 | Baseline passes; selected `missing_debug_implementations` error |
| Application Debug exception | 0 | Same public type is not forced to implement library Debug obligations |
| Library target scope | 0 | Extra `--lib` check excludes binary-only Debug obligation; baseline still covers all configured targets |
| Library profile on binary-only package | 1 | Missing library target is a blocking Cargo failure, not silent success |
| Google clean | 0 | Selected check and policy pass |
| Google missing must-use | 1 | Baseline passes; selected `clippy::must_use_candidate` error |
| Google enum-glob exception | 0 | Enum-variant glob remains permitted; unselected enum-glob rule is not inferred |
| Justified local allow | 0 | Actual reasoned fixture attribute permits dbg; source location/lint/reason inventoried |
| Unknown source lint | 1 | Real installed-compiler `unknown_lints` diagnostic; baseline and selected checks fail |
| Unavailable Clippy | 1 | Baseline and selected checks unavailable; policy blocks |
| Unavailable selected lint | 1 | Baseline passes; profile abstains without execution; policy blocks |
| Unknown profile ID | 1 | Config rejection before practices publication |

The script verifies target/features scope, versioned identity, registry/check
agreement, exact `-W` flags and baseline compatibility, and rejects blanket
restriction/pedantic/all/nursery groups. Fixture baseline command is exactly:

```text
cargo clippy --all-targets --locked --message-format=json -- -D warnings
```

Real source cases use ordinary Cargo/Clippy and inspect compiler JSON codes. Two
unavailability cases are explicitly simulations: a Cargo shim emits the standard
missing-Clippy error; a driver shim filters `clippy::dbg_macro` from otherwise
real inventory while preserving version behavior. They do **not** claim execution
on an actual old compiler or removal of installed components. The unknown-source-
lint case is distinct from a selected registry lint missing from inventory.

Precise observed negative messages:

```text
Error: unknown verification.lint_profile "rqlens-nonexistent-profile-v1"; inspect rqlens lint-profiles
Clippy/rustc version evidence unavailable, unknown or inconsistent; no selected profile command executed
unsupported selected lint(s) in active Clippy inventory: ["clippy::dbg_macro"]; no selected profile command executed
```

The intentional fixture-only exception is
`#[allow(clippy::dbg_macro, reason = "debug tracing is the purpose of this isolated fixture")]`.
Its actual formatted attribute is recorded at `src/lib.rs:4` with the reason,
`conditional: false`, and `level: allow`. No production suppression was added.
Passing this case demonstrates that Rust local overrides remain effective and
visible, **not** unsuppressed guide conformance. All fixture policy passes apply
only to those isolated fixtures, never to Scratchpad as a whole.

## Actual Scratchpad selected-profile measurement

Created an isolated absolute-path copy of `rqlens.toml` under the evidence
directory, preserving **all** original source roots, fingerprint inputs, rule
budgets/exclusions and verification settings. Only project/output/helper paths
and explicit `verification.lint_profile = "google-v1"` differ. Google-v1 is an
opt-in configured-target recommendation pilot suitable for examining Scratchpad's
exposed result-producing APIs; it is not an assertion that all Google rules, or
library obligations on private application internals, are mandatory.

```sh
timeout 360 nix develop -c "$PWD/target/rqlens-style-step-3/rqlens" \
  measure practices --config target/rqlens-style-step-3/rqlens.toml
```

Producer exit **0**, not a gate pass. `root-before.json` preserves actual child
outcomes: **18 passed, 9 skipped, 1 unavailable, 1 failed error**, 29 checks total.
Baseline Clippy passes. Extra Google-v1 check fails with **25 observed
`clippy::must_use_candidate` diagnostics**, all in profiling APIs. The unchanged
MSRV dependency probe remains unavailable because offline metadata cannot fetch
non-host `accesskit_ios v0.2.2`; practices confidence is partial. No budget was
changed to hide this or make the repository gate green.

Recorded scope: workspace **true**, all-features **true**, all-targets **false**,
locked **true**, no excluded packages, Cargo host/default or inherited target
configuration. Dependencies remain subject to Cargo lint caps. The root baseline
command is unchanged:

```text
cargo clippy --workspace --all-features --locked --message-format=json -- -D warnings
```

Selected command:

```sh
cargo clippy --workspace --all-features --locked --message-format=json -- \
  -D warnings -D unknown_lints -D renamed_and_removed_lints \
  -W clippy::undocumented_unsafe_blocks -W clippy::wildcard_imports \
  -W clippy::must_use_candidate -W clippy::return_self_not_must_use \
  -W clippy::new_without_default
```

The lints are individual curated recommendations. `new_without_default` is
already warn-by-default on this backend; selecting it records intent, not newly
enabling it. Library-v1 adds stronger exported-library obligations only to its
extra library-target check. The successful role/enum/local-override cases above
are concrete limits on any whole-guide interpretation.

## Lens-driven improvement and after validation

Two diagnostics identified useful result-producing APIs in
`src/profile/process_metrics.rs`:

- `process_snapshot()` exists to return sampled data; dropping it loses that work.
- `ProcessMeasurement::between()` consumes snapshots and returns their deltas;
  dropping it loses the measurement and both retained snapshots.

Added explicit **`#[must_use = "..."]`** contracts, not lint suppressions. Reviewed
and documented best-effort/unsupported counters, process-lifetime peak scope,
missing/reset counters versus genuine zero deltas, and background/sampling
overhead. No runtime algorithm, system call, scope, output schema or arithmetic
changed.

Expanded the existing counter-delta test from one reset example to nine pairs,
covering missing-before/after, decreases, equality, growth and `u64` boundaries
across all five delta fields. Added a positive usage doctest and a compile-fail
regression using `#![deny(unused_must_use)]` to verify that discarding the
measurement is actually diagnosed.

Ran the exact recorded selected command above under `timeout 180 nix develop -c`
for the focused after-change check, without replaying root practices/MSRV. Actual
exit **101** remains expected: diagnostics fall **25 → 23**, with **zero** in the
affected module. All 23 remaining diagnostics are retained. This is a count of
observed compiler diagnostics, not proof that every selected binary/target was
fully checked after a compilation failure. Raw compiler JSON is in
`root-after.stdout.jsonl`, with parsed observations in `root-after-diagnostics.json`.

| Validation | Actual outcome |
| --- | --- |
| 16-case consumer probe | **0; all 16 pass** |
| Root selected practices producer | 0; selected child check failed, MSRV evidence partial |
| Focused Google-v1 after-change command | 101; 23 remaining findings, affected module clear |
| `cargo clippy --workspace --all-features --locked -- -D warnings` | **0**, original baseline remains passing |
| `cargo test --locked --all-features --lib profile::process_metrics::tests` | **0; 2 passed**, including expanded delta cases and Linux RSS unit parsing |
| `cargo test --locked --all-features --doc profile::process_metrics` | **0; 2 passed**, one positive and one compile-fail doctest |
| `cargo fmt --all -- --check` | 0 |
| Probe Python compilation and CLI help | 0 / 0 |
| `git diff --check` | 0 |

Rust commands were run under `nix develop -c`, with 120-second outer bounds for
focused tests/baseline Clippy. No timeout was reached. Root practices ran once;
no repeated MSRV, whole-application UI, coverage or platform campaign was added.
The full root test suite had already passed inside that initial configured
practices run; after edits, only affected tests and relevant lint checks were
rerun.

## Limits and recommendations

No new consumer defect requires a lens change from this run. Keep the successful
matrix as a regression for default compatibility, explicit extra checks,
availability abstention and role/local exceptions. The coordinator's additional
version-comparison and Cargo-home freshness contracts are separate tests; this
pilot records the installed tested backend but does not independently establish
all compiler-version or wrapper/configuration equivalences.

Scratchpad's selected profile still fails, and the existing full repository gate
is **not** claimed green. Existing source thresholds, macro/generated-scope
uncertainty, offline non-host MSRV limits and missing native Windows runtime
validation remain. No native metrics/coverage gate or non-host target run was
repeated. Local suppression inventory is authored syntax, not proof of effective
per-item levels or absence of generated/dependency overrides. A profile selection,
passing compiler run or rustfmt check is never whole Rust/API/Google-guide
compliance or compiler proof of human design quality.

Coordinator can review these three paths and commit this step after independent
lens validation; this worker made no commits.
