# Leverage & Locality Report Improvements

Review scope: `scripts/locality_bench.py`, `scripts/leverage_metrics.py`, `scripts/leverage_ast.rs`, `scripts/open-overview.ps1`, `scripts/measurement_catalog.py`, `viewer/index.html`, `viewer/data-viewer.js`, and the current `target/analysis/locality_metrics.json` / `target/analysis/leverage_metrics.json` outputs.

## North Star

The reports exist to surface two specific design properties. Every metric, weight, and view in the dashboard should be judged against these definitions, not against what is easy to count.

**Locality** is how much of the codebase a developer must hold in their head to understand or safely change one piece. High locality means: a module reads top-to-bottom; its inputs and outputs are explicit; a change inside it does not force chasing implications through unrelated files; conventions and assumptions are consistent with what is physically near it. Low locality is action-at-a-distance — hidden state, implicit coupling, far dependencies, behavior that depends on something three modules away.

**Leverage** is how much useful behavior comes from a unit of code, and how broadly a single change ripples. Good leverage centralizes a real shared concern so that callers benefit uniformly and invariants hold by construction. Bad leverage is centralization without genuine sharing — callers diverge, the abstraction grows conditionals, and a "small" change to the shared piece breaks unrelated features. High leverage is not the same as high reuse count: a heavily-called helper that callers keep working around has bad leverage.

Two implications follow:

- The metrics today are *proxies* for these properties, not measurements of them. The plan below is structured around how well each proxy tracks its definition, and what to do when it does not.
- Locality and leverage are in tension. Centralizing for leverage costs locality at the call sites; keeping things local costs leverage. The dashboard should make that tradeoff visible, not pretend a single combined score captures it.

## Summary of Current State

The reports are wired into the Quality Review dashboard. Locality is correctly framed as code locality (not CPU/cache locality). Leverage is generated from a Rust AST heuristic that counts type paths, iterator methods, `for` loops, and unsafe surfaces, then derives a score.

The locality side has the right shape — its raw inputs (far dependencies, layer violations, churn, contributor spread, test proximity) all map plausibly to "how much you must hold in your head." The leverage side does not. Counting `Vec`, `Box`, iterator chains, and `for` loops measures *style*, not leverage as defined. None of those signals tell you whether a module is shared by many callers, whether those callers genuinely need the same thing, or how far a change ripples.

## Highest-Impact Fixes

### 1. Re-anchor leverage on its definition, not on syntactic style.

`scripts/leverage_ast.rs` produces `heap_allocating_type_count`, `inline_type_count`, `iterator_method_count`, `for_loop_count`, and unsafe counts. These are style metrics. They do not answer "how much behavior does this module deliver per line, and how far does a change ripple."

The signals that *do* track leverage are mostly already computable from the architecture mapper and git history that locality already uses:

- **Reach (good-leverage signal):** inbound dependency count for a module, weighted by how many distinct layers/features the callers belong to. A module called by many callers across many areas is delivering broad behavior per unit of code.
- **Divergence pressure (bad-leverage signal):** number of callers that import the module *and* override, wrap, or branch around it (heuristic: callers that import and also define functions whose names mirror or shadow the module's exports). A high-reach module with high divergence is a leaky abstraction.
- **Change ripple (bad-leverage signal):** average number of files co-changed in the same commit as this module, taken from git history. A module whose edits routinely drag many other files along has poor leverage even if it looks well-factored.
- **Invariant surface (good-leverage signal, harder):** number of `pub` items that are types vs. number that are free functions. Centralizing through types (so callers cannot violate invariants) is higher-quality leverage than centralizing through helpers callers can ignore.

Keep the existing AST counts as a separate "code style" report, or fold them into a code-quality view, but stop labeling them as leverage. The current artifact mislabels style for the property the dashboard claims to track.

### 2. Sharpen locality so it measures hidden coupling, not just dependency count.

The current locality inputs (outbound/inbound dependencies, far dependencies, layer violations, churn, contributors, test proximity) already track the definition reasonably well. Two gaps weaken it:

- **Implicit coupling is invisible.** A module that touches a global, reads a static, or mutates shared state via a singleton has worse locality than its dependency count suggests, because none of that coupling appears in `use` statements. Add a signal for: references to module-level `static`/`thread_local`, `lazy_static`/`OnceCell` reads, and calls to functions returning singletons. Each one is a hidden input that breaks the "reads top-to-bottom" property.
- **Interface explicitness is unscored.** A function whose behavior is fully determined by its parameters has higher locality than one that reads `self` fields or globals. A cheap proxy: ratio of `pub` functions whose signatures take their primary inputs as parameters vs. those that read large `&self` state. This will be noisy, but even a coarse signal would distinguish "small explicit functions" from "methods on a god-object."

Layer violations and far dependencies already measure non-local coupling well — keep them, but rename the combined score so it reads as "non-locality risk," not "locality score" (where higher-is-better is ambiguous). The goal is for a high score to clearly mean "you have to hold a lot in your head."

### 3. Show locality and leverage as two axes, not one combined number.

The viewer currently presents per-metric tables and a `total_leverage_score` field. Because the two properties trade off, the most useful single view is a scatter plot or quadrant chart with locality-risk on one axis and leverage on the other:

- High leverage + high locality: ideal. Shared abstraction that callers don't have to peer inside.
- High leverage + low locality: risky centralization. A change here ripples far AND the module is hard to reason about. These are top triage candidates.
- Low leverage + high locality: fine. Self-contained leaf code. Don't refactor for leverage's sake.
- Low leverage + low locality: dead weight or accidental coupling. Either delete or split.

A single combined score hides which quadrant a module is in.

### 4. Rank tables by what triage actually needs, and label the direction.

`scripts/leverage_metrics.py` sorts by ascending `total_leverage_score`, but the viewer table is labeled "Ranked by total Leverage score" with no direction. The CLI `render_cli` shows the first ten rows. For a triage report, sort by *risk* (worst-first) and label it explicitly: "Highest non-locality risk" and "Worst leverage tradeoff (high reach + high divergence)." If the dashboard wants both ends, expose a toggle, but make the default match the triage purpose.

### 5. Remove duplicate viewer IDs and choose one surface.

`viewer/index.html` defines `locality-leverage-summary`, `locality-table`, and `leverage-table` twice — once inside Quality Review and once inside an unreachable `locality-leverage` section with no nav button. `getElementById` returns the first match, so the second section is dead. Either delete it, or add the nav tab and make all IDs unique. This is a correctness bug independent of the metric overhaul.

### 6. Make the architecture map honest about what it shows.

The map dropdown lists Locality Risk and Leverage Risk, and `data-viewer.js` has labels for them, but `mapMetricValue()` does not return locality or leverage values, so selecting either silently falls back to total-score behavior. Either join leverage metrics by module key into `map.py` and expose `locality_risk` / `leverage_risk` as map overlays, or remove the options. A dropdown that lies about what it visualizes is worse than not having it.

### 7. Harden the Rust analyzer CLI.

`scripts/leverage_ast.rs` returns success on missing arguments, unreadable path-list input, and malformed per-file inputs. Convert it to a `run() -> Result<(), Box<dyn Error>>` shape, keep `main()` as a thin wrapper, support `--help`, and exit non-zero on top-level failure. Per-file read/parse problems can stay row-level warnings with a `parse_status` field, but the command should not silently produce a partial artifact when setup failed. This matters more once the analyzer is generating signals the dashboard actually depends on.

## Measurement Improvements

- **Provenance on both schemas.** Add `source`, `measured_at`, `command`, `host`, and `mock`/`estimated` fields. Stale or synthetic artifacts being mistaken for live measurements is a recurring failure mode.
- **Always preserve raw inputs alongside derived scores.** For locality: far dependency count, inbound/outbound counts, layer violations, churn, commit count, contributor count, hidden-coupling count (per fix #2), inline test evidence, external test references. For the new leverage signals: inbound count, caller-area count, co-change count, divergence count, public-types vs. public-functions count. Scores are interpretive; raw counts let future formulas change without re-running analysis.
- **Explain low scores.** The current artifact has 17 files with `iterator_leverage_score == 0` and no signals. Every score below a threshold should attach the specific signals that drove it, so a viewer can answer "why is this red" without reading source.
- **Stable module keys.** Windows backslashes currently flow into `module_name`. The map uses Rust-style targets (`app::domain::buffer`). Normalize `src/app/domain/buffer/state.rs` into a module key and preserve the original path in a separate field, so locality, leverage, and map data can join.
- **Document score formulas in `scripts/metrics.md`.** The overview metrics page does not yet define locality or leverage. Without a written formula, dashboard numbers are uninterpretable and the team cannot push back on scoring choices.

## Rust Implementation Practices

These apply if the AST analyzer continues to exist (either repurposed for code-style metrics, or extended with the new leverage signals from fix #1).

- Split `scripts/leverage_ast.rs` into collection and scoring concerns: `LeverageCounts`, `LeverageScores`, `LeverageRecord`. Keep `syn::Visit` focused on counting; move score logic into a pure `score_leverage(&LeverageCounts)` function.
- Add focused tests around source snippets: generic `Vec<T>` / `Box<T>` paths, iterator chains, `for item in collection.iter()`, raw index loops, `unsafe fn`, `unsafe impl`, `unsafe {}` blocks. Cheap to write, catches scoring regressions.
- Avoid `unwrap()` in report generation. `serde_json::to_string_pretty(&results).unwrap()` should propagate even though serialization is unlikely to fail.
- If analyzer tooling grows, move `syn`/`proc-macro2` analyzer binaries into a small workspace member so the main app dependency graph stays focused.

## Suggested Sequence

The order matters: data trustworthiness first, then re-anchoring on the definitions, then UI.

1. **Harden the analyzer and lock down provenance.** Patch `leverage_ast.rs` (`run()`, error propagation, `parse_status`, unit tests around `score_leverage`). Add provenance and stable module keys to both schemas. After this step, every artifact is interpretable and explainable.
2. **Fix viewer correctness.** Deduplicate IDs, decide the navigation model, remove the false map options. This is independent of the metric overhaul and unblocks any future view.
3. **Re-anchor leverage on reach, divergence, and ripple.** Add the new signals from fix #1 alongside the existing AST counts. Don't remove the AST counts yet — let the new signals run in parallel for a few weeks so changes can be evaluated against real code.
4. **Add hidden-coupling signals to locality.** Statics, singletons, `OnceCell` reads. Rename the combined score to "non-locality risk" with explicit direction.
5. **Switch the dashboard to two-axis presentation.** Replace the combined-score table with a quadrant view. Keep ranked tables for triage but label them by what they prioritize.
6. **Retire or relabel the AST style metrics.** Once the new leverage signals have been validated against real refactors, either move the AST counts into a "code style" report or drop them. Document the final formulas in `scripts/metrics.md`.
7. **Wire the validated metrics into the architecture map** as module-level overlays, so locality and leverage become navigable, not just tabular.
