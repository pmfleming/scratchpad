# Leverage & Locality Report Improvements

Review scope: `scripts/locality_bench.py`, `scripts/leverage_metrics.py`, `scripts/leverage_ast.rs`, `scripts/open-overview.ps1`, `scripts/measurement_catalog.py`, `viewer/index.html`, `viewer/data-viewer.js`, and the current `target/analysis/locality_metrics.json` / `target/analysis/leverage_metrics.json` outputs. There is no `scripts/overview` directory in this checkout, so the review used the overview launcher and viewer integration instead.

## Summary

The reports are wired into the Quality Review dashboard. Locality should be understood as code locality, not CPU/cache locality: related code should stay nearby, dependencies should not sprawl across layers, tests should be close to behavior, and change history should not imply broad edit blast radius. Leverage is generated from a Rust AST heuristic and should continue to expose explainable raw counts alongside scores.

## Highest-Impact Fixes

1. Keep locality focused on quality semantics.
   `scripts/locality_bench.py` should remain a static code-locality report based on dependency spread, layer violations, test proximity, churn, and contributor spread. Runtime cache locality belongs in Performance Review, where the project already has broader performance metrics.

2. Fix the leverage ranking direction.
   `scripts/leverage_metrics.py` sorts by descending `total_leverage_score`, so the CLI and table show the best modules first even though the report is meant for triage. The viewer labels the table "Ranked by total Leverage score," but operationally it should rank lowest score / highest risk first, or clearly offer separate "best" and "worst" modes.

3. Remove duplicate viewer IDs and choose one surface.
   `viewer/index.html` defines `locality-leverage-summary`, `locality-table`, and `leverage-table` twice: once inside Quality Review and once inside a separate `locality-leverage` section. There is also no tab button for `data-tab="locality-leverage"`. Because `getElementById` returns the first match, the separate section is effectively unreliable. Either keep the metrics inside Quality Review and delete the unused section, or add the nav tab and make all IDs unique.

4. Make map integration real or remove the options.
   The map dropdown includes Locality Risk and Leverage Risk, and `data-viewer.js` has labels for them, but `mapMetricValue()` does not return locality or leverage values. Selecting those options currently falls back to total score behavior. Either join leverage metrics by module path into `map.py` and expose `leverage_risk`, or remove the options until the data exists.

5. Harden the Rust analyzer CLI.
   `scripts/leverage_ast.rs` currently returns success for missing arguments, unreadable path-list input, and malformed per-file inputs. Convert it to a `run() -> Result<(), Box<dyn std::error::Error>>` shape, keep `main()` as a thin wrapper, support `--help`, and exit non-zero for top-level failures. Per-file read and parse problems can be row-level warnings, but the command should not silently produce a partial artifact when setup failed.

## Measurement Improvements

- Add provenance fields to both schemas: `source`, `measured_at`, `command`, `host`, and `mock` / `estimated`. This prevents stale or synthetic artifacts from being mistaken for live measurements.
- For locality, preserve raw structural inputs beside the score: far dependency count, inbound/outbound dependency count, layer violations, churn, commit count, contributor count, inline test evidence, and external test references.
- For leverage, include raw counts used in the score: `heap_allocating_type_count`, `inline_type_count`, `iterator_method_count`, `for_loop_count`, `unsafe_expr_count`, `unsafe_fn_count`, `unsafe_trait_or_impl_count`, and `parse_status`. The current score hides why many files land at `40.0`.
- Revisit the static heuristic. Treating every `for` loop as "low leverage" is too coarse in Rust because indexed loops, `for item in items.iter()`, and range loops have different implications. Likewise, `Vec`, `Option`, and `Result` should not all be interpreted as the same kind of "inline" signal. Separate concerns into clearer fields: unsafe surface, ownership/allocation pressure, iterator/control-flow style, and abstraction pressure.
- Add warning signals for low scores even when the current threshold misses them. The latest leverage artifact has 17 files with `iterator_leverage_score == 0` and no signals, so the table can show a bad score without explaining why.
- Store both the original file path and a normalized Rust module key. Windows backslashes currently flow into `module_name`, while map metadata uses module-like targets such as `app::domain::buffer`. Normalize `src/app/domain/buffer/state.rs` into a stable key and preserve the original path separately.
- Document score formulas in `scripts/metrics.md`. The overview metrics page does not yet define locality or leverage, which makes dashboard numbers harder to interpret.

## Rust Implementation Practices

- Split `scripts/leverage_ast.rs` into collection and scoring concepts: `LeverageCounts`, `LeverageScores`, and `LeverageRecord`.
- Keep the `syn::Visit` implementation focused on counting syntax; move score calculation into a pure `score_leverage(&LeverageCounts)` function.
- Add focused Rust tests around source snippets: generic `Vec<T>` and `Box<T>` type paths, iterator chains, `for item in collection.iter()`, raw index loops, `unsafe fn`, `unsafe impl`, and `unsafe {}` blocks.
- Avoid `unwrap()` in report generation. `serde_json::to_string_pretty(&results).unwrap()` should propagate an error even if serialization is unlikely to fail for the current structs.
- If the analysis tooling grows, consider moving `syn` / `proc-macro2` analyzer binaries into a small workspace member so the main app dependency graph stays focused.

## Suggested Sequence

1. First patch `scripts/leverage_ast.rs`: add `LeverageCounts`, `LeverageScores`, `LeverageRecord`, `run()`, error propagation, and unit tests around `score_leverage()`.
2. Change leverage output and ranking to worst-first, with raw counts and normalized module keys.
3. Fix viewer correctness: deduplicate IDs, decide the navigation model, and remove false map options.
4. Refine code-locality scoring with examples from real refactors so the risk weights match maintenance pain.
5. Add schema provenance and update `scripts/metrics.md`.
6. Only after the data is trustworthy, wire locality/leverage into the architecture map as module-level risk.
