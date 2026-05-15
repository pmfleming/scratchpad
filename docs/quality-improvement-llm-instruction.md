# Quality Improvement LLM Instruction

You are improving this project systematically across the Quality Review metrics. Treat the dashboard metrics as a triage system, not as a scoreboard to game. Your goal is to make the shipped end-user application simpler, safer, faster to maintain, easier to test, and less coupled.

## Scope Rule

Focus on code that can be included in the final application shipped to end users. You may ignore, de-prioritize, or explicitly exclude bins that represent non-shipping code, such as measurement scripts, dashboards, throwaway probes, generated reports, temporary diagnostics, experiments, benchmark-only harnesses, mock fixtures, or developer-only tooling. If you exclude a bin, say why and make sure it is genuinely outside the shipped runtime.

Do not hide real product risk by reclassifying application code as tooling. When in doubt, improve the code or leave a clear note explaining the uncertainty.

## Operating Loop

1. Run or inspect the current quality artifacts.
2. Identify the highest-impact bins across all metrics, not just the largest single number.
3. Separate shipping application code from non-shipping support code.
4. Pick a small batch of changes that reduces multiple risks at once.
5. Implement the changes conservatively, following existing project architecture and style.
6. Run focused tests or checks for the touched areas.
7. Re-run the relevant measurement artifacts when practical.
8. Record what improved, what stayed risky, and what was intentionally excluded.

Prefer fixes that remove real complexity over fixes that only move numbers. Do not perform broad rewrites unless the code is already isolated, tests cover the behavior, and the benefit is clear.

## Metrics To Improve

### Maintainability Hotspots

Reduce high quality scores by addressing cognitive complexity, cyclomatic complexity, low maintainability index, high Halstead effort, dense complexity per SLOC, and large files or functions.

Useful moves:
- Split long functions by domain step, not by arbitrary line count.
- Replace nested conditionals with early returns, helper predicates, or small state objects.
- Pull repeated decision logic into named helpers.
- Keep public APIs stable unless the caller cleanup is part of the same safe change.
- Prefer simpler control flow over clever abstractions.

Avoid:
- Moving code into helpers with vague names just to reduce a metric.
- Adding indirection that makes the call graph harder to understand.
- Touching generated or non-shipping analysis code unless it affects the shipped app.

### Duplication And Clones

Reduce clone groups when repeated code creates maintenance risk.

Useful moves:
- Extract shared behavior only when the repeated code has the same reason to change.
- Consolidate repeated UI/table/rendering logic behind local helpers.
- Keep small intentional duplication when it preserves clarity across unrelated domains.
- For clone bins in tests, fixtures, generated code, or measurement-only scripts, exclude them unless they directly obscure product behavior.

Avoid:
- Creating an abstraction that couples two workflows with different semantics.
- Optimizing away harmless repetition in one-off scaffolding.

### Structural Type Health

Reduce wide structs, large enums, broad method surfaces, declaration span, many impl blocks, and impl spread.

Useful moves:
- Split state by ownership, lifecycle, or domain boundary.
- Move behavior closer to the data it owns.
- Replace overloaded enums with smaller domain enums when variants serve different workflows.
- Keep type splits compatible with serialization, persistence, and UI state expectations.

Avoid:
- Splitting types only by field count while increasing cross-type coordination.
- Creating tiny wrapper types without domain meaning.

### Escape Hatches

Reduce or justify unsafe, FFI, raw memory, global mutability, layout/linkage attributes, lint suppressions, glob imports, container reference returns, and risky deref patterns.

Useful moves:
- Replace unsafe code with safe library APIs when available.
- Minimize unsafe blocks and document invariants at the boundary.
- Move unsafe/FFI code behind narrow modules with tests.
- Remove stale lint suppressions instead of broadening them.
- Replace glob imports in application code with explicit imports.

Allowed exclusions:
- Low-risk escape hatch bins in measurement tooling, benchmark harnesses, or build-only code that never ships can be deprioritized after noting the reason.

### Locality

Improve code locality by reducing far dependencies, hidden coupling, weak interface explicitness, scattered tests, and high churn on coupled modules.

Useful moves:
- Move behavior into the module that owns the data or invariant.
- Replace hidden cross-module knowledge with explicit interfaces.
- Put tests near the module or workflow they validate.
- Reduce dependency direction violations rather than adding adapter layers everywhere.

Avoid:
- Moving files mechanically without reducing coupling.
- Adding facades that merely hide the same dependency spread.

### Leverage

Improve leverage by making modules easier to reuse safely without broad ripple effects. Reduce excessive reach, divergence pressure, co-change ripple, weak invariant surfaces, low iterator/style leverage, and unnecessary heap-heavy structures.

Useful moves:
- Strengthen small stable APIs around important invariants.
- Reduce broad caller reach by introducing narrower operations.
- Prefer iterator-style transformations when they clarify ownership and flow.
- Replace heap-heavy or indirect structures with simpler inline forms when it improves clarity and performance.

Avoid:
- Making an API more generic than the current product needs.
- Trading local clarity for theoretical reuse.

### Churn x Complexity

Prioritize modules that are both frequently changed and structurally risky. These are often the best refactor targets because they generate repeated maintenance cost.

Useful moves:
- Stabilize the highest-churn, highest-complexity paths first.
- Add focused tests before refactoring behavior-heavy code.
- Prefer small sequential improvements over one large rewrite.

### Module Rollup

Use the module rollup to find modules that accumulate several moderate risks. A module with medium hotspots, clone touches, escape hatches, locality risk, and leverage risk can be more important than a module with one isolated high score.

## Exclusion Policy

You may exclude a metric bin from improvement only when at least one of these is true:
- The bin is not part of the final shipped end-user application.
- The bin is generated code or generated report output.
- The bin belongs to measurement, benchmark, diagnostic, migration, or one-off tooling.
- The bin is intentionally duplicated test or fixture code and does not affect product maintainability.
- The bin reflects accepted risk with a clear invariant, owner, or tracking note.

When excluding, record:
- The bin or module name.
- The metric it appeared under.
- Why it does not affect the shipped app.
- Whether any follow-up is still needed.

## Prioritization

Prefer this order:
1. Shipping code with high churn and high complexity.
2. Shipping modules with multiple risk categories in the module rollup.
3. Unsafe or FFI risks in application runtime code.
4. Locality/leverage problems that cause repeated changes across modules.
5. Large structural types that are actively changing.
6. Clone groups in application code with shared semantics.
7. Non-shipping tooling only when it blocks measurement accuracy or developer velocity.

## Change Discipline

Keep changes reviewable. Do not mix unrelated refactors. Preserve behavior unless the task explicitly includes behavior change. Add or update tests when refactoring logic, ownership, unsafe boundaries, or user-visible workflows.

Before finishing, report:
- Which metrics were targeted.
- Which files or modules changed.
- Which bins were intentionally ignored because they are not part of the shipped app.
- What validation ran.
- Which risks remain.
