# RQLens style campaign — Step 6 consumer pilot

**Final frozen consumer validation complete; required actual human reviews remain
pending/red. This is not human approval, safety proof or whole-guide acceptance.**

Scratchpad baseline: `8e218cc`. No consumer commits or live Lens edits. The
coordinator owns campaign acceptance, integration and commits. All existing
budgets, source roots, fingerprint inputs, exclusions and rule settings are
unchanged; the new declared-review error rule has `max = 0`.

## Identities and chronology

The initial 35-minute readiness wait ended pending at 10:42:31 CET on
2026-10-28. The coordinator subsequently authorized a **candidate** pilot. Final
readiness was published at 11:04; candidate evidence is not relabelled as final.

Evidence root: `target/rqlens-style-step-6/`.

| Identity | Candidate | Final v1 |
| --- | --- | --- |
| Frontend SHA-256 | `8b050f7f2d037505282a7a9ca64f7d7178ca378ae6a42be16d36fbc7134b4342` | `7185063e5ef551b9bc2fd20b96b21d366ac90f75b9e54c7b362e7781a0ec666b` |
| Source-manifest SHA-256 | `bb613a5fe50770ad29c42bdb0814ed7c03ce86f308a8f7bfb19317b2548747fc` | `375795c581dae277642df10ecdd9625e797deb1c75895072125d0a5b019bc0f3` |
| Frozen source inventory | 181 files | 183 files |
| Evidence directory | `candidate/` | `final/` |

Candidate: copied Cargo manifests/lock, `src`, full `rust_helpers`,
`compiler_helpers`, contracts (including the topic registry), README/license and
available toolchain/config inputs. Live-before, live-after and copied hash maps
were identical. Built **only the copy**, with a private target:

```sh
b="$PWD/target/rqlens-style-step-6/candidate"
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_NET_OFFLINE=true \
  timeout 300 nix develop -c cargo build --locked --offline \
  --manifest-path "$b/lens-snapshot/Cargo.toml" \
  --target-dir "$b/build" --bin rqlens
```

Exit **0**. Source maps and build logs/status are retained. Final: copied the
supplied `/tmp/rqlens-workers/step6-frozen` frontend and every source-manifest
entry, including complete helpers/contracts and the core's own pending drafts.
Verified supplied hashes and complete inventories before/after copying and after
execution. No final frontend rebuild or live helper workspace was used.

Unchanged accepted probe SHA-256:
`c5982410c954694c6e1b3ff7e1be5de760acf3e996842a4154a723cec9094353`.

## CLI regression evidence

```sh
b="$PWD/target/rqlens-style-step-6/final"
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_NET_OFFLINE=true \
  CARGO_TARGET_DIR="$b/build" timeout 600 nix develop -c python3 \
  scripts/check-rqlens-design-review.py "$b/rqlens" "$b/probe" \
  --helper-manifest "$b/lens-snapshot/rust_helpers/Cargo.toml"
```

Final: **exit 0, 35/35 cases pass** (`final/probe.log`, `probe/summary.json`).
The same unchanged probe passes **35/35** on the candidate in
`candidate/probe-corrected/`. New output directories are required for reruns.

Positive qualifying decisions are **SYNTHETIC TEST FIXTURES ONLY**, inside toy
projects: identities/rationales explicitly identify them as synthetic and not
real people or Scratchpad approval. They exercise declared-human qualification,
not authentication or actual adequacy/applicability judgments.

Cases cover all seven topic IDs; accepted/not-applicable declarations; AI/pending,
missing manifest/record, changes-requested, AI/synthetic-kind decisions, wrong
topic and actual changed scope (identical contents at different relative paths),
source edit/add/remove/restoration, separate manifest/requirement freshness,
unsupported algorithms, future/malformed times, unbounded validity, short
rationale, duplicate/unknown records, future schema, malformed JSON, disabled
legacy/required modes, enforced empty scope, unsupported/self-referential scope,
and read-only commands without executing Cargo/compiler shims. Every case checks
policy/SARIF counts, units and stable obligation IDs as applicable. Incomplete
coverage requires `observed = null`, never accepted zero.

The expiry case waits 45 seconds and checks again, without changing source or
manifest bytes. A previously qualifying synthetic declaration becomes expired;
check blocks and SARIF reports the same obligation. Time is evaluated live,
not hidden in a source digest or reused from an old policy total.

### Retained first-attempt failure

`candidate/probe-first/` and its exit **1** remain intact: **34/35** passed; the
metadata test initially assumed SARIF would export alongside an intentionally
stale generic `api_health.json`. SARIF correctly refused with:

> SARIF input api_health.json is stale or has incomplete provenance; rerun measurement

This was a harness assumption, not a Lens defect. The corrected test requires
that refusal, preserves the stale-artifact policy observation, refreshes generic
API evidence, then asserts live design findings/counts. It also proves that
manifest metadata changes invalidate artifact freshness while reviewed-source
bindings remain identical. No stale guard or negative assertion was weakened.

No new concrete Lens defect was found by this consumer. Feedback was appended
with Scratch attribution, preserving other workers' entries.

## Actual pending records and bounded improvement

`rqlens.toml` declares three required IDs, with records generated through CLI
`design-review prepare` and enriched only as AI-authored questions in
`.config/rqlens-design-reviews.json`:

- `path-key-newtype-v1`: `newtypes`, exact `src/app/mod.rs` scope.
- `path-key-documentation-v1`: `documentation-usefulness`, the same exact file.
- `windows-watch-unsafe-necessity-v1`: `unsafe-necessity`, exact
  `crates/windows_file_watch/src/lib.rs` scope.

Every actual record is `pending-human`, reviewer kind `ai`, and has null human
review/expiry timestamps. No actual record is accepted or not-applicable; none
is attributed to the user. The three requirements live in configuration and
cannot disappear merely by deleting manifest entries. Binding excludes the
manifest; its raw metadata is fingerprinted separately. FNV source freshness is
not cryptographic attestation and has no commit/self-hash dependency.

### Questions, not conclusions

The path-key drafts ask whether owned lossy strings should instead preserve
`OsString`/`PathBuf`, whether exact derived string equality/hashing are suitable
for file selection, how canonicalization errors and filesystem changes should
be exposed, and whether Windows lowercasing is justified. The documentation
draft asks a human to judge usefulness, examples, alternatives and boundary
clarity, not merely presence/length.

The Windows draft asks why direct FFI is necessary over safe wrappers or other
watchers, and requests assessment of handle ownership, wait/rearm/shutdown,
ignored failure returns and recovery behavior. Existing rationale comments are
not treated as a necessity/soundness decision. Native Windows tests and human
analysis remain outstanding.

Historical Step-5 R2 evidence grounds these questions: frontend
`8893d3bc3bdcdd5f1ea155a1230f1c6f5cf5a493071bd6c788b97f168d32ef74`,
fingerprint `43e360fdd0a912b3`, selected Linux libraries/features, `cfg(doc)`,
Rust 1.95.0/JSON57 with explicit experimental bootstrap. It observed the four
path-key API definitions without own docs and an actual standard Debug
implementation. Debug does not prove identity/equality/hash design quality.
That historical artifact was globally partial due to unresolved Cargo
project/helper path-dependency provenance; it does not validate Windows runtime
code or constitute fresh Step-6 compiler/human evidence. The extracted context
is preserved in `prior-semantic-review-context.json` under the evidence root.

### Implemented improvement and same-candidate observations

Clarified `CanonicalPathKey`, `from_path`, `as_str` and `paths_match` in
`src/app/mod.rs`: filesystem canonicalization is attempted, errors fall back
silently, relative/dot spelling can remain, Windows text is lowercased, lossy
conversion can collide, and independently constructed keys are neither a shared
filesystem snapshot nor persistent file identity. Runtime behavior is unchanged;
the design trade-offs remain questions for a human.

Same frozen candidate, same full source roots/configuration:

| Native `src/app/mod.rs` observation | Before | After |
| --- | ---: | ---: |
| Observed public items | 23 | 23 |
| Own documented items | 0 | 4 |
| Observed missing docs | 23 | 19 |

Both native observations remain **incomplete** (attribute/derive and ownership
uncertainty), not a complete expanded API denominator or usefulness judgment.
Captured via `measure api-health --config candidate/root-native.toml`, each
bounded at 300 seconds, exits **0**; preserved as `native-before.json` and
`native-after.json`. These are **candidate** before/after measurements, not final
measurements. The final source is unchanged from the tested after-state.

Actual review transition, using that same candidate:

| State | Pending | Source-stale | Qualified human | Rule observed | Check | Design SARIF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Before source edit | 3 | 0 | 0 | 3 | 1 | 3 |
| After edit, old drafts retained | 1 | 2 | 0 | 3 | 1 | 3 |
| Refreshed AI-only pending drafts | 3 | 0 | 0 | 3 | 1 | 3 |

Two topic obligations refer to one changed file, so two bindings become stale;
counting remains one unmet obligation per configured ID, not per issue or file.
Refreshing pending drafts is not approval. Candidate manifests, fingerprint,
inspect/check JSON, SARIF and statuses for each transition are preserved.

## Application validation

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 timeout 180 nix develop -c \
  cargo test --lib app::tests::canonical_path_keys -- --nocapture
# Retained cold-build timeout: exit 124.
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 timeout 300 nix develop -c \
  cargo test --lib app::tests::canonical_path_keys -- --nocapture
timeout 60 nix develop -c cargo fmt --all -- --check
timeout 60 nix develop -c python3 -m py_compile scripts/check-rqlens-design-review.py
```

Warm retry: **exit 0, three tests pass**, including two new tests of actual
existing-path canonicalization and missing-path dot spelling; 551 unrelated tests
filtered out. Warm build completed in 66 seconds. Formatting and Python syntax
checks pass; `git diff --check` passes. Logs/statuses are retained. No full
practices, coverage, MSRV, compiler/platform matrix or GUI suite was repeated.
Completed unchanged application tests were not rerun for final acceptance.

## Final live root validation and handoff

Final `root-live.toml` preserves production source roots, all old policy settings
and new max-zero human obligations; only project/output locations and explicit
frozen helper/offline identity are isolated. Its output directory intentionally
contains live review/policy results, not the separately retained native API
measurements. This permits a live declared-review SARIF without pretending
partial generic artifacts are exportable or dropping any policy obligation.

Inside pinned `nix develop`, jobs=2/debug=0/offline/private target:

```sh
b="$PWD/target/rqlens-style-step-6/final"
"$b/rqlens" design-review prepare --config "$b/root-live.toml"
"$b/rqlens" design-review fingerprint --config "$b/root-live.toml"
"$b/rqlens" design-review inspect --config "$b/root-live.toml"
"$b/rqlens" check --config "$b/root-live.toml"
"$b/rqlens" sarif --config "$b/root-live.toml" --output "$b/root-final.sarif"
"$b/rqlens" check --config "$b/root-live.toml" --fail-on partial
```

The first five commands were captured by `root-capture.py` inside a 240-second
outer bound with 120-second individual bounds; strict check had a separate
120-second bound. Prepare/fingerprint/inspect **0**; check **1**; SARIF **0**;
strict check **1**, failing `configured-rules` and `partial`. Other missing/
incomplete required evidence remains explicit; no green repository gate claimed.

Final live JSON/SARIF agree: **3 required, 0 qualified human declarations,
3 pending, 3 unmet findings**; review rule `observed = 3`, `max = 0`, `exceeded`.
`required_coverage_complete = true` means machine qualification was evaluated,
not that a human completed review: `required_human_declarations_satisfied = false`
and `identity_authenticated = false`. Manifest/source binding sets agree between
candidate after-state and final; final drafts are independently prepared and
archived, not relabelled candidate outputs.

Final tool context: rustc/Cargo 1.95.0, Linux, `RUSTC_BOOTSTRAP` unset. No new
experimental semantic capture is claimed in Step 6. The historical JSON57
experiment remains explicitly separate. File scopes cover only their declared
files, not descendant modules, every platform or the whole project. Regular
UTF-8/size-bounded local reads are not a sandbox against concurrent malicious
filesystem replacement. Neither minimum rationale length nor declared identity
authenticates a person or proves necessity, usefulness, safety or design quality.

Final verification: `final/root-verification.json`; immutable 183-file inventory
matches after execution. Source SHA-256 after improvement:
`1c11347f94e52c3be9d97256e9dbd3b8a34783c449e56536e8f4dd9b56837fee`.
Final actual pending manifest SHA-256:
`fd5a20503becee44affb4727453a3f1644400810132486b584a8bf053d9afcba`.

Coordinator handoff: review `rqlens.toml`, `src/app/mod.rs`,
`.config/rqlens-design-reviews.json`, `scripts/check-rqlens-design-review.py` and
this report. `/docs/` is ignored; the new report requires force-add. Consumer
made no commits and created no human approvals. Human obligations stay red until
a human actually acts; final campaign acceptance remains the coordinator's task.
