# RQLens style-guide pilot — step 5 (R2 final acceptance)

## Result and ownership

**R2 is the final consumer acceptance implementation:** the unchanged 13-case
probe passes, and a separate actual R2 root after-state capture confirms **1,813
nodes / 129 documented observed nodes**, including all nine repaired snapshot
docs. Updated attached-implementation dependencies are checked against raw compiler
IDs and same-invocation Cargo artifacts. Root strict check remains correctly red.

The **120 → 129** before/after improvement below is preserved **v1 history**, not
R2 before/after evidence. Main subsequently found a v1 dependency omission beyond
the original probe's coverage and required R2. No executable behavior, budget,
exclusion, suppression or tracked lockfile changed. No lens edits or commits were
made; step 6 was not started.

Started from Scratchpad `a48b2ad`. Initial preparation used the early interface;
that process ended before completion. Resumed against core commit `4cc52e5` and
the original v1 readiness archive, then separately froze the supplied R2 archive
(base `4cc52e5` plus R2 changes). No mutable helper inputs or separately built
candidate were used for acceptance. Main owns campaign acceptance/ledger/commits.
This report is ignored by `/docs/` and requires coordinator force-add.

Changed paths:

- `scripts/check-rqlens-semantic-api.py`: durable real semantic-backend probe.
- `src/profile/process_metrics.rs`: type/eight-field documentation and one doctest.
- `docs/rqlens-style-step-5.md`: this report.

## Historical v1 frozen implementation and shared experimental scope

Ignored evidence: `target/rqlens-style-step-5/final/`. Copied the immutable
frontend, Cargo workspace files, helpers, sources and contracts from
`/tmp/rqlens-workers/step5-frozen/`. All supplied source-manifest hashes passed
before and after the pilot. The explicit helper manifest always points into
`final/lens-snapshot/`, never the live checkout.

- Frontend SHA-256:
  `f0304b03a09fb569dcace1ff2c5560773b0cffa61eab88530988a355491d805e`.
- Source-manifest SHA-256:
  `e536e9d28662ed4982899f1f8521dce5c0145c8219aa97e7e6a76014428f629f`.
- Shared unchanged probe SHA-256 (v1 and R2):
  `335316627c2873edefe7940e91273a14be8ec9306f0b9bbfe9f05fa02d1383fc`.

Tools are from the pinned **`nix develop -c`** environment: rustc/rustdoc/Cargo
**1.95.0**, actual rustdoc JSON **format 57**, Linux
`x86_64-unknown-linux-gnu`. The installed compiler is not nightly. Ordinary stable
JSON capture is refused; **stable Rust is not claimed to support this unstable
format**. Successful experimental captures explicitly inherit
`RUSTC_BOOTSTRAP=1` from the consumer. Their reports record
`bootstrap_inherited: "1"` and `bootstrap_set_by_rqlens: false`.

For bounded resource use, the probe/root captures additionally use
`CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_NET_OFFLINE=true`.
These settings are explicit, recorded compilation context, not suppressed lints
or relaxed quality thresholds. Runtime profile `debug_assertions` is not treated
as proof of rustdoc's cfg: the backend discloses `cfg(doc)` and the runtime/profile
versus rustdoc distinction. No nightly/alternate compiler matrix was tested.

## Durable real CLI/compiler probe

The following command/path records the historical **v1** pilot. The same unchanged
probe is rerun under R2, with its distinct command/results recorded below.

```sh
b="$PWD/target/rqlens-style-step-5/final"
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_NET_OFFLINE=true \
  timeout 600 nix develop -c python3 scripts/check-rqlens-semantic-api.py \
  "$b/rqlens" "$b/probe-final" \
  --helper-manifest "$b/lens-snapshot/rust_helpers/Cargo.toml"
```

**Both v1 and R2 actual exits: 0; all 13 cases PASS on each.** Each command is independently bounded to
180 seconds and fixtures configure 120-second backend commands. Every output
directory must be new. All positive assertions inspect **actual embedded compiler
JSON** (`contexts[].capture.raw_document`), not fabricated reports or source-pub
metadata substitutes. The graph's name/docs/IDs are compared with that captured
JSON, while scope, Cargo artifacts, features, provenance and failures are checked
separately. Earlier direct compiler/oracle captures remain under `compiler-prep/`
and `cycle-prep/`: ordinary stable exit 101/no JSON; explicit experimental
captures exit 0/format 57, including a valid cyclic-glob program.

| Case | Actual verified behavior |
| --- | --- |
| Disabled backend | Disabled, nullable total, no verified API zero |
| Legacy signatures separate | No compiler contexts/IDs manufactured from cargo-public-api signature rows; legacy tool is unavailable here |
| Ordinary stable refusal | Real `-Z` refusal, zero observed items but **unknown total**, bootstrap not silently set |
| Expanded public graph | Macro-generated type, private-module renamed reexport, alias, public field and local trait recovered from actual JSON |
| Source/compilation freshness | Source and each changed `CARGO_PROFILE_DEV_DEBUG_ASSERTIONS`, `RUSTDOCFLAGS`, `RUSTC_BOOTSTRAP` reject reused semantic evidence as **stale** |
| Selected feature capture | Real second capture adds `FeatureOnly`; actual Cargo features change from `[default]` to `[default, extra]` |
| Failed real compilation | Unresolved reexport cycle after successful capture cannot reuse old JSON; unknown total, no old items/raw document parsed |
| Valid cyclic glob graph | Real compiler succeeds, terminal item retained, uncertified glob expansion remains partial/null total; traversal terminates |
| Workspace selection | Actual Cargo members minus configured exclusion equal actual captured package IDs; distinct library contexts remain distinct |
| Missing rustdoc | Real nonexistent executable; unavailable, not zero |
| Unsupported format | Explicit **negative-only** format-version mutation to 999 rejected |
| Malformed decoder input | Explicit **negative-only** malformed JSON rejected |
| Private-item scope | Actual compiler `--document-private-items` JSON has `includes_private=true` and is refused as unsupported scope |

The two decoder wrappers first execute real rustdoc, retain its original JSON,
and only then deliberately damage the decoder input. They are named and marked
negative fixtures, never positive compiler proof. Positive cases use the real
unmodified compiler document supplied by the backend.

### Concrete graph/identity observations

- The private module's `Reexported` type is exposed as `PublicAlias`, not by its
  inaccessible `implementation::Reexported` export path. The use item's outer
  name can be null; the actual inner use name/target ID is used.
- `Unreachable`, private fields/types, hidden root items and hidden private-module
  items are not counted as rustdoc-visible definitions. This deliberately
  nonhidden scope is **not** every reachable item in the Rust language.
- `Generated` exists after macro expansion, with compiler docs. Its span points
  to the macro invocation; source attribution remains a candidate, **not** proof
  of an authored struct declaration or expansion ownership.
- Local `Debug` implemented by `LocalDebugOnly` is not standard Debug. Positive
  Debug observations on generated/reexported types require actual
  `[core, fmt, Debug]` trait identity and a probed-sysroot origin. No general trait
  solver or missing-trait certificate is inferred.
- `SecondAlias` is retained as a type-alias definition pointing at the original
  type ID, not falsely converted into another nominal type.
- Definition docs and use/reexport docs remain different strings; rendered or
  inherited docs are not guessed.
- The external reexport's package/version is bound by canonical rmeta paths to
  **same-invocation Cargo compiler-artifact package IDs**, not crate-name/URL
  guessing. Its absent external definition body/inherited docs remain unknown.
  Thus the primary fixture is honestly partial: **8 observed nodes** by default,
  **9 with extra**, with nullable total—not a clean complete graph.
- `doc_only` is present and `runtime_only` absent, matching actual `cfg(doc)`.
  Native source-authored observed counts remain **11/11**, totals unknown,
  unchanged by semantic capture; they are not replaced with compiler counts.

Counting unit is explicitly
`rustdoc-definition-node-per-library-capture-including-root-and-members`.
Compiler IDs are capture-local, not stable global source identities; use artifact
fingerprint/capture provenance alongside the scoped IDs. Definition text presence
is not proof of useful human documentation, soundness or privacy correctness.

### Preserved first-run harness failure

`probe/` retains the initial 12-pass/one-failure run. The harness incorrectly
expected one selected library in the workspace fixture. Actual Cargo metadata
auto-enrolled the path dependency despite the fixture's attempted workspace
exclude; excluding only the secondary package correctly yielded **two** contexts.
This was not a lens defect. The final assertion derives selected IDs from the
actual metadata membership minus the configured exclusion, checks exact capture
membership and distinct context IDs, and still rejects the excluded `Secondary`
API. No production exclusion or expected safety failure was weakened. Only
v1 `probe-final/`, with the hash above, was its completed pilot evidence; the
separate **`r2/probe/`** run of that unchanged file is current final acceptance.

## Historical v1 actual Scratchpad library captures

This paired source-improvement history belongs only to frontend `f0304b03...`.
Its reported completion did not validate R2's later-expanded dependency
relationships. No v1 capture below is relabeled as an R2 observation.

Created isolated `final/root-rqlens.toml` from the existing config, retaining every
source root, input, budget, exclusion and feature/verification selection. Changed
only project/output/helper paths, explicit offline identity, and opt-in
`verification.public_api_backend = "rustdoc-json-v1"` /
`public_api_target = "x86_64-unknown-linux-gnu"`. Production `rqlens.toml` remains
unchanged.

```sh
b="$PWD/target/rqlens-style-step-5/final"
RUSTC_BOOTSTRAP=1 CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 \
  CARGO_NET_OFFLINE=true timeout 1500 nix develop -c "$b/rqlens" \
  measure semantic-api --config "$b/root-rqlens.toml"
```

Ran this exact frozen-tool/config/environment command before and after the
bounded source change. Both producers exit **0**; both execute an actual successful
Cargo rustdoc command. Backend command timeout stays at the original configured
1,200 seconds; outer bound is 1,500 seconds. No timeout occurred for semantic
captures. Fresh private target directories force dependency recompilation;
recorded capture times were **156,384 ms** and **219,675 ms**. They are compiler
capture cost, not application runtime performance.

Actual selected context: one Cargo library `scratchpad`, host target above,
`cfg(doc)`, all features with actual artifact features **`count-lines, inspection`**,
public/nonhidden docs, `document_private_items=false`, runtime dev profile with
`debug_assertions=true`, debuginfo 0, opt-level 0. Rustdoc cfg is not inferred from
that runtime profile. Metadata uses matching `--filter-platform`; offline host
metadata succeeds while unfiltered metadata's uncached `accesskit_ios v0.2.2`
failed during early preparation. That prerequisite feedback was incorporated
before final readiness.

| Actual compiler observation | Before | After |
| --- | ---: | ---: |
| Definition/member nodes, including root | 1,813 | 1,813 |
| Raw compiler index entries (not reachable API count) | 10,231 | 10,231 |
| Observed definitions with nonempty own docs | 120 | 129 |
| `ProcessSnapshot` + eight fields with own docs | 0/9 | 9/9 |
| Standard Debug observation on `ProcessSnapshot` | present | present |
| Backend selected-graph status | completed | completed |
| Whole artifact/provenance complete | **false** | **false** |

Raw captures and all join/component facts are embedded in `root-before.json` and
`root-after.json` (approximately 117 MiB each). Actual output digests:
**8926be42de0891ae → 827f18db291a8e85**. Private capture directories differ, and
`output_reused_after_failure=false` in both. Source input fingerprints differ:
**7f3d9ea785373f9c → 90b24337ddf9e492**. No source metadata guesses substitute for
these actual executions.

## Source improvement and historical v1 validation

The real reachable API identified `ProcessSnapshot` and its eight public fields
with null own docs. Added type/field contracts for best-effort availability,
units, process-wide versus operation scope, Linux high-water/fault counters,
platform-dependent I/O accounting and open-file/handle counts. Added a runnable
counter-delta example preserving unavailable write counts as `None` rather than
zero. No fields, serialization names, arithmetic, sampling calls, ownership or
runtime behavior changed.

| Focused validation under pinned Nix shell | Actual result |
| --- | --- |
| V1 13-case semantic probe | **0; 13 pass** |
| V1 root before/after semantic producer | **0 / 0**, real successful compiler captures |
| `cargo fmt --all -- --check` | 0 |
| Locked all-feature `profile::process_metrics::tests` | **0; 2 pass** on bounded warm retry |
| Locked all-feature `--doc profile::process_metrics` | **0; 3 pass**, including new example and existing compile-fail result-use contract |
| Stable HTML rustdoc `RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps --lib` | 0, without experimental JSON/bootstrap |
| Strict root `check --fail-on partial` | **1**, correctly retains unknown/missing evidence |
| Frozen source-manifest verification / `git diff --check` | pass / 0 |

The initial affected-unit-test build hit its **150-second outer build bound**
(exit 124) while compiling cold GUI dependencies; its log is preserved as
`affected-tests-initial-timeout.log`. This is not a passing test or a lens defect.
The unchanged tests passed on a 300-second bounded warm retry (12.37-second build,
two tests). Other focused test/rustdoc bounds were 150 seconds. No full root
practices/MSRV/coverage/platform campaign or application UI run was added. Source
bytes remain identical for R2, so these application unit/doctest/doc validations
were not unnecessarily repeated for a lens-only tool swap.

## R2 final acceptance — separate immutable implementation and after-state

Main's real counterexample found that v1 omitted public dependencies occurring
only in attached public trait implementations. The original v1 snapshot, probe
outputs and 120→129 source-improvement captures remain untouched under
`target/rqlens-style-step-5/final/`. R2 is separately preserved under
`target/rqlens-style-step-5/r2/`, copied from the supplied
`/tmp/rqlens-workers/step5-frozen-r2/` archive, including complete Cargo workspace,
helper and contract sources. Source-manifest checks passed before/after this
pilot; neither live checkout nor frozen core was edited or rebuilt.

- **R2 frontend SHA-256:**
  `8893d3bc3bdcdd5f1ea155a1230f1c6f5cf5a493071bd6c788b97f168d32ef74`.
- **R2 source-manifest SHA-256:**
  `6f9480e15431729f12c3c2f9acff0124d177975b72495cc3f3bddb39f65e2ff7`.
- **Unchanged probe:**
  `335316627c2873edefe7940e91273a14be8ec9306f0b9bbfe9f05fa02d1383fc`.
- **Unchanged repaired source:**
  `63b4f9c54cdeab2deeeb1bd5f1f394cc8cb2a678be2a5d11c24085e7140ebcf0`.

R2 config retains all original budgets/exclusions/features and the same explicit
host/experimental/resource context; only isolated tool/helper/output paths differ
from the historical v1 pilot.

```sh
b="$PWD/target/rqlens-style-step-5/r2"
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_NET_OFFLINE=true \
  timeout 600 nix develop -c python3 scripts/check-rqlens-semantic-api.py \
  "$b/rqlens" "$b/probe" \
  --helper-manifest "$b/lens-snapshot/rust_helpers/Cargo.toml"
RUSTC_BOOTSTRAP=1 CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 \
  CARGO_NET_OFFLINE=true timeout 600 nix develop -c "$b/rqlens" \
  measure semantic-api --config "$b/root-rqlens.toml"
RUSTC_BOOTSTRAP=1 CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 \
  CARGO_NET_OFFLINE=true timeout 120 nix develop -c "$b/rqlens" \
  check --fail-on partial --config "$b/root-rqlens.toml"
```

| R2-only final result | Actual observation |
| --- | --- |
| Unchanged real CLI/compiler probe | **0; all 13 pass** |
| Single actual root after-state producer | **0**, successful fresh compiler capture |
| Capture scope | Same library/host/all-features/nonhidden `cfg(doc)` scope, explicit inherited bootstrap |
| Compiler capture duration | **144,956 ms**, within the 600-second outer bound |
| Definition/member nodes / own documented nodes | **1,813 / 129** |
| `ProcessSnapshot` and eight fields | **9/9 documented**, verified from R2 item docs/raw compiler nodes |
| Strict root report gate | **1**, partial/configured evidence not silently clean |
| Frozen sources and consumer source/probe hashes | unchanged / pass |

R2 after-state output digest: **786aafec706d7430**; fingerprint:
**43e360fdd0a912b3**. Its actual private directory was
`/tmp/nix-shell.vKTorm/nix-shell.F2jjYM/rqlens-rustdoc-4adVv4`;
output reuse after failure remains false. No R2 pre-improvement measurement was
made, so the 120→129 delta remains exclusively v1 evidence.

### Updated dependency relationships verified, not merely counted

R2 root data records **31 observed external crate origins**: **28 exact Cargo
artifact-path package bindings and three probed-sysroot origins**. Literal
relationship observations comprise **11,527 attached implementation headers**,
**3,505 attached member signatures** and **607 exported-definition signatures**.
This is an expanded projection versus historical v1's signature/export inputs,
**not growth in the source API or 31 necessarily required runtime dependencies**.
Conditional/generic/blanket relationships are compiler observations; they do not
prove applicability or solve trait bounds. All are delimited by the new
`scope.public_dependency_relationship_scope`; docs/private bodies are not inputs.

Independently checked a concrete previously omitted class of relationship:

```text
ProcessSnapshot definition 10433
  -> attached impl 10481, component=trait, ownership_known=true
  -> compiler path [serde_core, ser, Serialize], external crate 25
  -> exact Cargo package serde_core 1.0.228
```

The actual raw impl's `for` type ID equals the captured `ProcessSnapshot` ID;
its trait ID resolves through that same raw document's paths to the dependency's
crate ID. The exact package ID is present in same-invocation dependency
compiler-artifact evidence. Also verified attached member **`serialize` (10480)**
is in that raw impl's member list, retaining known owner and member-signature
relationship. These are R2 capture-local IDs, not joins to v1 IDs. Machine-readable
checks are preserved in `r2/relationship-verification.json`.

The unchanged 13-case probe alone did not cover main's original attached-only
counterexample; this additional real-root relationship check supplies direct
consumer verification of the correction. No new R2 defect appeared. Main's
separate attached-only/generic-bound/member regression suite is not claimed as
executed by this consumer.

## Honest remaining limits and feedback

The selected compiler graph/docs/required dependency joins report complete for
this host/features/nonhidden `cfg(doc)` context; trait solving and inherited/
rendered docs remain explicitly unknown. This **does not** make the root artifact
or repository gate complete: the shared fingerprint still reports
`Cargo metadata did not resolve the complete project/helper path dependency graph`.
`semantic_api.json` has `artifact_status: partial`; strict check exits **1** with
`configured-rules` and `partial`. Existing budgets and unsupported native/generated
scope remain untouched. Captured compiler observations are not a universal
freshness certificate for unresolved project inputs.

The original v1 consumer probe passed but did not establish attached-impl-only
dependency completeness; main subsequently found that genuine omission. R2's
unchanged probe plus the explicit after-state relationship checks pass, with no
new R2 defect found. Append-only Scratch-attributed feedback in
`/tmp/rqlens-workers/step5-feedback.md` preserves early preparation, historical v1
results and separate R2 acceptance without replacing main's notes.
The host-filter, exact artifact dependency binding and embedded raw capture
improvements resolved the early preparation concerns before freeze.

Untested: other toolchain/format releases, native Windows runtime, every alias or
export shape, ambiguous external core-name combinations beyond the supplied
implementation's scope, and arbitrary wrapper/build-script/proc-macro attestation.
Actual compiler JSON does not prove human doc usefulness, unsafe soundness,
semantically necessary traits or design quality. Step-4 units/policy behavior are
unchanged; no human approval/review record or step-6 feature was manufactured.
Main reviews the three paths, force-adds this report and commits only after its
required campaign acceptance; this worker made no commits.
