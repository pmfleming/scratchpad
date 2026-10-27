# RQLens style-guide pilot — step 2

## Result

Consumer validation and the focused Scratchpad improvement are complete. The final
native extractor fixes both the original four-line Windows safety-comment false
positive and the nested-argument defect discovered by this consumer. **The
unchanged consumer probe now passes**, including documentation counts, real versus
bogus safety comments, and populated versus empty Safety sections. Original
failing evidence is preserved below alongside the corrected follow-up results.

Started from clean Scratchpad `49a6b82`. Preparation initially waited approximately
15 minutes without a ready marker and was reported pending. Resumed after
`/tmp/rqlens-workers/step2-ready` arrived. No lens sources, application thresholds,
exclusions, waivers or tracked lockfiles were changed. No commits were made.
This report is ignored by `/docs/` and needs coordinator force-add.

Changed paths:

- `scripts/check-rqlens-native.py`: repeatable public-CLI consumer regression.
- `crates/linux_file_watch/src/lib.rs`: substantive API documentation and one
  runnable doctest; no runtime behavior changes.
- `docs/rqlens-style-step-2.md`: this report.

## Frozen inputs and reproduction

Evidence is under ignored `target/rqlens-style-step-2/`. The sibling lens checkout
was based on `16a41464780804b57521d93acb713239937d30b2`, with the ready step-2 changes
not yet committed. The initial ready-snapshot executable SHA-256 is:

`69fb34cdb8d2eef905e10d95d85ac094b23d7b95dc74af9bcbf17483dfdf9d29`.

A checkout-built lens prefers its compile-time checkout's `rust_helpers` sources:
**copying only the binary does not freeze the measurement implementation.** Both
the executable and helper workspace were therefore copied after readiness:

```sh
lens=/home/laufan/Projects/rust-quality-lens
s=target/rqlens-style-step-2
mkdir -p "$s/lens-snapshot"
cp "$lens/target/debug/rqlens" "$s/rqlens"
cp "$lens/"{Cargo.toml,Cargo.lock} "$s/lens-snapshot/"
cp -r "$lens/"{src,rust_helpers} "$s/lens-snapshot/"
```

The complete hierarchy retains workspace-inherited package and lint settings.
`helper-source-sha256.json` records all 136 snapshot files; their hashes were
checked unchanged after the paired root measurements. Executable hash is also
unchanged. `rqlens.toml` copies every Scratchpad source root, fingerprint input,
verification option and policy budget; only absolute project/output paths and
`rust.helper_manifest` pointing into this snapshot differ. Rust is 1.95.0 on
Linux x86_64. No full practices/MSRV, coverage or UI run was repeated.

```sh
q="$PWD/target/rqlens-style-step-2/rqlens"
c=target/rqlens-style-step-2/rqlens.toml
for metric in reliability api-health; do
  timeout 180 "$q" measure "$metric" --config "$c"
done
```

Both producers exited 0 before and after the API-documentation change. Pre-edit
reports are preserved in `before/`; post-edit reports are in `artifacts/`.

## Safety and documentation accuracy

### Original Windows regression fixed

The entire `crates/windows_file_watch/src/lib.rs` file remained byte-for-byte
unchanged, SHA-256:
`fd8fee29ca593b500dbaadfd41914e4f48f042bfcc122c61e850facd3d936a64`.
Its four-line `SAFETY:` rationale at lines 223–226 still precedes
`WaitForMultipleObjects` at line 227. Neither fresh root reliability report
contains an undocumented-unsafe finding for this file. No rationale was moved,
shortened or suppressed to achieve this result.

### Initial isolated public-CLI regression probe

This section preserves the first ready snapshot's discovery. The separately
frozen final correction and passing rerun are recorded below.

```sh
python3 scripts/check-rqlens-native.py \
  target/rqlens-style-step-2/rqlens \
  target/rqlens-style-step-2/probe-final \
  --helper-manifest \
  "$PWD/target/rqlens-style-step-2/lens-snapshot/rust_helpers/Cargo.toml"
```

Use a new output directory on every run. The script copies the executable,
creates a dependency-free Rust fixture and runs bounded reliability/API-health
measurements. It preserves source, raw artifacts, logs, identity and summary on
failure. It requires complete fixture evidence rather than treating partial data
as success. Both fixture reports have complete confidence in the supported
authored-syntax scope. Generated fixture Rust also passes locked Cargo check.

Actual `probe-final/summary.json` output (excerpt):

```json
{
  "passed": false,
  "documentation": {
    "public_item_count": 10,
    "documented_public_item_count": 6,
    "missing_documentation_count": 4
  },
  "undocumented_unsafe_lines": [16, 21, 26, 31],
  "missing_safety_section_lines": [44, 51],
  "errors": [
    "undocumented unsafe lines: [16, 21, 26, 31]; expected [21, 26, 31]"
  ]
}
```

Documentation assertions pass: struct/field/method/enum/variant/function
obligations are counted, hidden fields/methods/variants/functions excluded,
empty/whitespace docs remain missing, and meaningful `#[doc = "..."]` works.
Safety assertions also correctly accept the four-line and block-comment
rationales, reject an `UNSAFETY:` substring, a `SAFETY:` string literal and an empty
rationale, and distinguish a populated `# Safety` section from prose or an empty
section. These tests exercise extraction, not semantic reachability or proof that
arbitrary unsafe code is sound. Fixture unsafe functions are never executed.

### Original consumer defect: nested unsafe expression (subsequently fixed)

The first ready snapshot's only failed assertion was this fixture source at lines 14–16:

```rust
fn inline_argument(pointer: *const u8) -> u8 {
    // SAFETY: the caller supplies a live readable byte for this synchronous read.
    std::convert::identity(unsafe { *pointer }) // probe: inline_argument
}
```

The comment belongs to the containing statement and explains its only unsafe
operation, but the ready extractor emits `rust.safety.undocumented-unsafe` on
line 16. The same defect affects existing real source at
`src/bin/resource_probe/alloc_metrics.rs:30,36`:

```rust
// SAFETY: This allocator delegates the actual allocation to the system
// allocator with the same `layout` it received from the runtime.
record_allocated_ptr(unsafe { System.alloc(layout) }, layout)
```

Both fresh root reports contain exactly those two safety findings, and no
missing-Safety-section findings; total reliability records remain **987 → 987**.
Recommend associating comments with the containing statement/expression rather
than requiring the unsafe token to immediately follow comment whitespace. Keep
rejecting unrelated comments, string literals and substring markers. The
regression is retained in the probe, not hidden with a waiver or source rewrite.

## Lens-driven Scratchpad improvement

Fresh API-health identified **10 observed public documentation obligations and
0 observed documented items** in `crates/linux_file_watch/src/lib.rs`. Added:

- crate/type guidance distinguishing invalidation hints from a complete file log;
- variant/field contracts, including overflow invalidation and error path scope;
- asynchronous startup and non-restarting worker-failure semantics;
- replacement (not append), nonrecursive roots, retries and queued old-root events;
- nonblocking event draining, with no false health/freshness guarantee;
- non-joining drop behavior and a runnable event-consumption example.

After the change, the same frozen tool observes **10 obligations / 10 documented
items**, versus **10 / 0** before. Complete `public_item_count`, documented count,
missing count and percentage correctly remain **null**, because this source
contains derives and macros that can affect inventory. This is an observed
literal-documentation improvement, **not** a claim of complete API coverage or
100% project documentation.

No executable logic, data layout, signatures, ownership, filesystem operations,
thread/channel behavior or timeout changed. Tuple-variant formatting only adds
its field's documentation. The separate Windows safety rationale is untouched.

## Final corrected-tool follow-up

Waited for the **new** `/tmp/rqlens-workers/step2-final-ready` marker, which arrived
within the bounded eight-minute wait (22:49 +01:00). Copied the updated executable
and full helper workspace into a **separate** ignored snapshot:
`target/rqlens-style-step-2/final-followup/`. The original failing snapshot remains
intact. The final executable SHA-256 is:

`00f9db2d1584982020ce84eaabf1d84f9a9e729ecd991c518bc6695224e77ee3`.

The lens base revision remains `16a41464780804b57521d93acb713239937d30b2`.
The final snapshot again contains the root manifests, `src/` and `rust_helpers/`;
all 136 snapshotted file hashes were verified unchanged after validation. Its
copied consumer config retains all original scope and budgets, changing only the
snapshot/output paths from the prior isolated config.

```sh
s=target/rqlens-style-step-2/final-followup
python3 scripts/check-rqlens-native.py "$s/rqlens" "$s/probe" \
  --helper-manifest "$PWD/$s/lens-snapshot/rust_helpers/Cargo.toml"
for metric in reliability api-health; do
  timeout 180 "$s/rqlens" measure "$metric" --config "$s/rqlens.toml"
done
```

All three commands exited **0**. Final `probe/summary.json` excerpt:

```json
{
  "passed": true,
  "errors": [],
  "documentation": {
    "public_item_count": 10,
    "documented_public_item_count": 6,
    "missing_documentation_count": 4
  },
  "undocumented_unsafe_lines": [21, 26, 31],
  "missing_safety_section_lines": [44, 51]
}
```

The real four-line comment, block comment and nested-argument comment now all
pass. Bogus substring/string/empty rationale and improper Safety sections remain
flagged. Both fixture reports retain complete confidence in their supported
scope. **Neither probe assertions nor consumer source files changed** between
failing and corrected runs; `consumer-source-sha256.json` records these hashes:

- probe: `fb41f3ee8b34575ea9d1ecd4c999d02462a3998581e210900d82772c63ad8cfe`;
- Linux watcher: `0ab31c919633eff33a3707e67fb26d7acb99ab95937863b357657959d7faf75b`;
- Windows watcher: `fd8fee29ca593b500dbaadfd41914e4f48f042bfcc122c61e850facd3d936a64`.

Root reliability records decrease **987 → 985**, with **zero safety findings**.
Comparing rule/path/line/message multisets shows exactly the two allocator false
positives removed and no added findings. The unchanged Windows rationale remains
accepted. Root reliability confidence correctly remains partial.

API-health records are identical to the previous post-documentation report when
excluding per-record confidence metadata. The watcher still has **10 observed /
10 documented**, with complete totals/percentage unknown. This also checks that
the coordinator's separation of structural-risk and documentation counters did
not change these API-health outputs. No hotspot/risk-score measurement was run,
so this pilot does not independently certify risk-score invariance.

Prior application tests, watcher tests/doctest and formatting were not needlessly
rerun: their source bytes are unchanged. `git diff --check` passed again. No full
practices/MSRV replay or application run was added.

## Validation and limitations

| Validation | Actual result |
| --- | --- |
| Frozen root `measure reliability` before/after | 0 / 0 |
| Frozen root `measure api-health` before/after | 0 / 0 |
| Initial ready-tool consumer probe | 1, solely the nested-expression false positive; discovery preserved |
| Final corrected-tool consumer probe | **0**, unchanged assertions |
| Final corrected-tool root reliability / API-health | **0 / 0**; zero safety findings, unchanged API documentation outputs |
| Probe Python compilation / CLI help | 0 / 0 |
| Generated probe Rust locked Cargo check | 0 |
| Linux watcher existing tests | **2 passed**, covering real inotify changes, recreation, root replacement, error deduplication, overflow and invalidation |
| New watcher documentation example | **1 doctest passed** |
| Watcher rustdoc with `RUSTDOCFLAGS='-D warnings'` | 0 |
| Locked all-feature application open-controller tests | **6 passed**, including deduplication and staged/background hydration |
| `nix develop -c cargo fmt --all -- --check` | 0 |
| `git diff --check` | 0 |

Cargo cannot directly test a dependency with dev-dependencies outside the root
workspace: `cargo test --locked -p linux_file_watch` exited 101. The actual watcher
tests use an isolated copy under ignored `target/`, with source bytes checked
identical to the edited crate. Its final lock is a pruned **copy** of the root
lock, retaining application versions `rustix 1.1.4` and `tempfile 3.27.0`:

```sh
w=target/rqlens-style-step-2/linux-watch-tests
mkdir -p "$w"
cp crates/linux_file_watch/Cargo.toml "$w/"
cp -r crates/linux_file_watch/src "$w/"
cp Cargo.lock "$w/Cargo.lock"
CARGO_TARGET_DIR="$PWD/target" nix develop -c cargo check --offline --manifest-path "$w/Cargo.toml"
CARGO_TARGET_DIR="$PWD/target" nix develop -c cargo test --locked --manifest-path "$w/Cargo.toml"
CARGO_TARGET_DIR="$PWD/target" RUSTDOCFLAGS='-D warnings' \
  nix develop -c cargo doc --locked --no-deps --manifest-path "$w/Cargo.toml"
nix develop -c cargo test --locked --all-features --lib \
  app::services::file_controller::open::tests
```

The initial preparatory watcher fixture generated its own offline lock and also
passed two tests; final validation above was rerun with the root-version subset.
Root/source-tree lockfiles are unchanged.

Earlier preparatory results are retained but are not controlled old/new-tool
comparisons: even the unchanged step-1 binary consumed the sibling's changing
helper sources. Its initial simple probe saw 5/5/0 documentation counts and
incorrectly flagged the four-line rationale, illustrating why helper snapshots
are required. Only post-readiness frozen measurements support this report's
before/after claims.

Native Windows tooling remains unavailable; syntax inspection is not Windows
FFI compilation/runtime validation. Root API/reliability confidence remains
partial for macros/attributes/generated scope. The previously recorded offline
non-host MSRV metadata limitation was not replayed. No full-project gate pass,
compiler proof or general style-guide compliance is claimed. The reported
nested-expression defect is now fixed and independently retested with the
unchanged consumer probe. Coordinator can review and commit the step changes in
both repositories; this worker made no commits.
