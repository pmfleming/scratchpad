# Correctness Review Page Audit

## Findings

The Correctness Review page renders the contents of `target/analysis/correctness_review.json`. It does not independently discover tests. If that artifact is stale, the page is stale.

Current checked artifact:

- `target/analysis/correctness_review.json` reports 41 tests.
- Running the catalog generator without executing tests discovers 52 tests from source.
- `cargo test -- --list` listed at least 52 tests before failing on a profiling binary that requires elevation on this machine.

So the current page can display all tests present in the artifact, but the artifact is not currently capturing every test in the workspace snapshot.

## Display Gaps

- Passed tests are hidden by default, which is useful for triage but makes coverage look empty when all tests pass.
- The page previously did not show catalog row count versus summary count.
- The table did not show module ownership, only layer and path.
- The layer matrix ignored skipped tests visually.
- Categorization is mostly path-based and coarse: `App Shell and State`, `Buffer and Text Storage`, `Services and Persistence`, and `UI and Editor Interaction`.

## Improvements Made

- Added a correctness overview that shows catalog count, payload rows, modules, layers, status counts, test-kind counts, visible rows, and last-run state.
- Added table accounting text showing how many tests are visible after filters.
- Added module column to the test table.
- Added skipped counts to the layer matrix bar and count row.

## Recommended Next Steps

1. Regenerate `target/analysis/correctness_review.json` during the dashboard refresh before judging coverage.
2. Improve `scripts/test_catalog.py --run` so one binary that cannot execute does not prevent status collection for the rest of the suite.
3. Add a source/cargo-list reconciliation field to the JSON, for example `discovered_from_source`, `listed_by_cargo`, and `uncataloged_tests`.
4. Split the coarse path-based layers into functional categories such as file I/O, session persistence, diagnostics, text history, encoding, search, editor input, and app shell.
