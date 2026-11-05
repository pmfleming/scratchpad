# RustQualityLens refactoring review

Compared baseline `86ad24d` with the implemented refactor and model-test cleanup.
The local tool is `/home/laufan/Projects/rust-quality-lens/target/debug/rqlens`.
Both runs use the same frozen binary and unchanged `rqlens.toml`; binary and source
BLAKE3 identities are retained in [measurements.json](measurements.json).

## Changes

- Storage had three copies of original/add span traversal. A private, indexed
  [walker](../../src/app/domain/buffer/piece_tree/storage/ranges.rs) now serves
  character counting, piece reconstruction and bounded streaming. Whole committed
  chunks retain their metadata-only path; partial reads remain fallible and UTF-8
  checked. The new module's measured line coverage is 95.73%.
- Editor input shared too little between reset and normal preparation. One event
  classifier and deferral path now preserve order, widget input and admission
  checks. Events move with `extract_if`; preparation queues borrow buffers directly,
  avoiding ID vectors and repeated searches.
- Restore used a separate test-only whole-string/disk implementation and obsolete
  status branches. Runtime and tests now reconstruct the same reference-backed
  documents. Missing hydration returns `InvalidData` instead of empty text, with
  errors propagated through active, streaming, parallel and retained restore paths.
- Retained reopening moves paths and preserves small display preferences instead
  of cloning entire buffer records. Clone-family call sites in this operation fall
  from seven to two; this is a source-level count, not an allocation benchmark.
- Reused xtask binaries could retain another checkout's compiled-in manifest path.
  Runtime root discovery and a two-checkout regression test fix this. The initial
  invalid after run is excluded from every comparison.
- Database rejection and retained reopening now use broader scenario matrices.
  String-model oracles check bounded extraction, UTF-8, line metadata and character
  columns after mixed edits. The tab-index oracle also rejects stale entries.
  Three redundant fixed examples were removed after their assertions moved into
  the model oracle. No ignored test was added.

## Measurements

| Measure | Before | After |
| --- | ---: | ---: |
| All-function cognitive complexity | 7,977 | 7,918 |
| All-function cyclomatic complexity | 13,519 | 13,477 |
| All-function Halstead effort | 55,221,782 | 54,825,538 |
| Edited-module cognitive complexity | 286 | 205 |
| Edited-module cyclomatic complexity | 309 | 258 |
| Edited-module Halstead effort | 1,880,415 | 1,253,521 |
| Source nonblank lines | 100,499 | 100,260 |
| Duplicated lines | 7,288 | 7,075 |
| Test entry points | 786 | 777 |
| Test-function SLOC | 18,408 | 18,407 |
| High-CRAP functions | 66 | 64 |
| Line coverage | 48,353/64,425 (75.05%) | 48,297/64,250 (75.17%) |
| Function coverage | 5,410/7,010 (77.18%) | 5,404/6,996 (77.24%) |
| Region coverage | 68,548/91,715 (74.74%) | 68,458/91,475 (74.84%) |
| Glob imports | 40 | 35 |
| Mean leverage score (partial graph) | 11.0338 | 10.5952 |
| Mean locality score (partial graph) | 96.4781 | 96.8318 |

Complexity/effort rows sum function records, rather than counting both module and
function records. Edited-module rows cover the same seven module paths, including
new `storage/ranges.rs` and inline test functions; paths are listed in the JSON.
Test-function SLOC excludes shared test helpers. Halstead effort is a syntax-based
proxy, not engineering hours. Coverage counts include the actual executable-line
denominators; deleting obsolete code also changes those denominators. Branch
coverage is unavailable.

Bulk-input interception provides a behavioral coverage check independent of the
whole-project percentage: coverage rises from **30.56% to 89.29%**, cognitive
complexity falls **33 to 4**, cyclomatic complexity **30 to 9**, and CRAP **331.41 to
9.1**. The complete edited-module totals above include the extracted helpers.

The graph-based scores remain partial, with thousands of unresolved dependency
references and unexpanded macros. Their raw means are reported, including the
leverage result; these data do **not** demonstrate a quantitative leverage gain.
The concrete reuse improvement is the shared private traversal and input-routing
logic. Unsafe/FFI inventories and lint suppressions are unchanged; the escape-hatch
reduction comes from explicit imports.

## Validation and remaining findings

The all-feature root tests, integration tests and doctests pass under the final
RustQualityLens run. The standalone xtask suite passes 14 tests. Application Clippy
passes for all targets/features with warnings denied; standalone xtask Clippy also
passes. Source identities remained stable during both accepted measurements.

The strict quality gate remains **failed**: 14 partial-evidence artifacts, 3 required human reviews and 33 threshold violations (baseline 38). It reports zero test failures, practice failures, selected reliability findings and operational failures. No threshold, waiver or review acceptance was changed. Native Windows runtime validation remains pending under the existing authorization.


Remaining review priorities are `Database::read_layouts` (cyclomatic 54) and
`render_editor_text_edit` (cognitive 24, cyclomatic 25). Typed row decoding and
separating editor state transitions are candidates for later work; extracting
helpers alone would not establish a reduction in aggregate complexity.

Raw accepted runs are under `target/quality-runs/refactor-before` and
`target/quality-runs/refactor-complete`. They contain source patches, tool copies,
coverage, correctness reports and gate output. Reproduce from each desired source
checkout with a fresh output directory:

```sh
direnv exec . cargo xtask quality /home/laufan/Projects/rust-quality-lens/target/debug/rqlens /absolute/path/to/new-quality-run
```

The command intentionally returns nonzero while the strict gate remains failed.
When sharing build caches across checkouts, remove stale application artifacts
before comparing instrumented coverage; retain dependency caches.
