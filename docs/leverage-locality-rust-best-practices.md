# Rust Best-Practice Review: Leverage & Locality Reports

Scope: the Rust side of the Code Locality & Leverage reports, primarily `scripts/leverage_ast.rs` and its `Cargo.toml` binary wiring.

## Main Recommendation

Treat the Rust analyzer as a normal production CLI, not a throwaway script. The current `leverage_ast` binary works for happy-path JSON generation, but it silently returns success on usage errors, read failures, parse failures, and JSON serialization failure paths. That makes the Python wrapper and dashboard prone to accepting incomplete or misleading artifacts.

## Improvements

1. Use a `Result`-returning entry point.
   Convert `main()` into a thin wrapper around `run() -> Result<(), Box<dyn std::error::Error>>`, print usage for `--help`, and exit non-zero on invalid arguments or unreadable input. Per-file parse/read failures can remain row-level warnings, but top-level failure should fail the command.

2. Emit typed raw counts, not only derived scores.
   Add fields such as `heap_allocating_type_count`, `inline_type_count`, `iterator_method_count`, `for_loop_count`, `unsafe_expr_count`, `unsafe_fn_count`, `unsafe_trait_or_impl_count`, and `parse_status`. This makes the dashboard explainable and lets future score formulas change without rerunning old analysis.

3. Separate scoring from AST collection.
   Keep the `Visit` implementation focused on counting syntax. Move formula logic into a pure function like `score_leverage(&LeverageCounts) -> LeverageScores`. That function should have unit tests for edge cases such as no loops, only `for` loops, mixed iterator chains, and unsafe-only modules.

4. Prefer path-aware output.
   Store both `path` and a normalized module key. Windows backslashes currently flow into `module_name`, while the map uses Rust-style targets such as `app::domain::buffer`. A normalization layer should convert `src/app/domain/buffer/state.rs` into a stable module key and preserve the original path separately.

5. Tighten the AST heuristic before expanding its influence.
   Counting every `for` loop as low leverage is too blunt in Rust: indexed loops, iterator-style `for item in items.iter()`, and range loops have different meanings. Likewise, `Vec`, `Option`, and `Result` are not "inline types" in the same sense. Split this into clearer Rust categories: ownership/allocation pressure, control-flow style, unsafe surface, and abstraction pressure.

6. Add focused Rust tests for the visitor.
   Put small source snippets in tests and assert counts. The minimum useful set is: generic `Vec<T>` / `Box<T>` type paths, iterator chains, `for item in collection.iter()`, raw index loops, `unsafe fn`, `unsafe impl`, and `unsafe {}` blocks. These tests would catch most scoring regressions cheaply.

7. Avoid `unwrap()` in report generation.
   `serde_json::to_string_pretty(&results).unwrap()` should propagate an error. Serialization should not fail for the current structs, but production tooling should not panic when the fix is trivial.

8. Keep runtime locality out of the Quality report.
   Cache and branch behavior should stay in Performance Review. Code Locality should remain a static quality signal based on dependency spread, test proximity, and change spread.

9. Keep generated report binaries out of app dependency weight if this grows.
   `syn` and `proc-macro2` are currently regular dependencies because script binaries live in the package. If analyzer tooling grows, consider moving analysis binaries into a small workspace member so the main app dependency graph stays focused.

## Suggested First Patch

Start with `leverage_ast.rs`: add `LeverageCounts`, `LeverageScores`, `LeverageRecord`, a `run()` function, and tests around `score_leverage()`. That gives immediate confidence in the report without touching the dashboard or Python wrapper.
