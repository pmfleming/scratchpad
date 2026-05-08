# Search/Replace edge-case implementation plan

Reviewed against the current project on 2026-05-08. The existing replace architecture is still sound: replacement work is planned up front, validated against revisions and expected text, then applied in reverse range order per buffer. The remaining work is concentrated around edge-case correctness, stale-target handling, and a few UX/performance follow-ups.

## Confirmed current risks

1. Replace can re-target its own replacement.
   - Current code: `src/app/app_state/search_state/replace.rs` calls `select_next_active_buffer_match_from(replacement_range.start)` after replacing one match.
   - Current selector: `src/app/app_state/search_state/visual.rs` chooses the first active-buffer match whose start is `>= minimum_start`.
   - Failure mode: replacing `foo` with `foobar` can select the newly inserted `foo` at the same offset, so repeated Replace keeps growing the first match instead of advancing.

2. Multi-buffer Replace All is not atomic.
   - Current code validates the whole `ReplacementPlan`, then applies each `ReplacementTargetPlan` in sequence.
   - Failure mode: if a later target fails after an earlier target was mutated, the workspace is left partially replaced.
   - Current validation reduces the risk, but it does not provide rollback.

3. Regex replacement expansion has surprising fallbacks.
   - `SearchProgram::expand_replacement` returns the raw replacement text when regex capture lookup fails.
   - `build_replace_all_plan` calls `replacement_for_match(...).unwrap_or_default()`, which can turn a failed compile/expansion into an empty-string replacement.
   - Failure mode: `$1` or `${name}` can be inserted literally, or a match can be deleted, when the safer behavior is to fail the replace action.

4. Replace All cursor placement uses the reversed replacement list.
   - `build_replacement_targets` stores replacements in reverse document order for safe application.
   - `replace_all_in_active_buffer`, `fallback_selection_for_target`, and `next_selection_for_target` read `target.replacements[0]`, which is the last match in document order.
   - UX concern: after Replace All, the cursor can land near the bottom of the file instead of near the first changed match or the user's prior cursor.

5. Replacement planning recompiles regex per match.
   - `replacement_for_match` compiles `SearchProgram` each time it is called.
   - At high match counts this is avoidable work, especially for Replace All.

6. Search results can survive tab reorder with stale tab indexes.
   - Closing tabs marks search dirty, but tab reorder currently does not.
   - Current global `BufferId`s make many stale-index cases fail closed, but replacing should not depend on that accident of identity design.

## No longer actionable from the original review

- The specific claim that empty zero-width replacements record a no-op undo entry is stale. `push_operation_record` drops empty edits and tests cover this path in `src/app/domain/buffer/document/tests.rs`.
- Zero-width regex search itself is mostly blocked by `SearchProgram::compile`, because regex queries must have a bounded maximum match length and `max_match_chars` is clamped to at least one. Keep this in mind when adding tests: use supported bounded regexes and do not assume `a*`-style queries are accepted.

## Implementation plan

### Phase 1: Make single Replace advance safely

Files:
- `src/app/app_state/search_state/replace.rs`
- `src/app/app_state/search_state/visual.rs`
- `src/app/app_state/search_state/api_tests.rs` or a new focused test module

Steps:
1. Change `replace_current_search_match` to advance from the end of the inserted replacement, not the start.
2. Add a helper name that makes the intent explicit, for example `select_next_active_buffer_match_after_replacement(end_char)`.
3. Keep wraparound behavior: if no later match exists in the active buffer, select the first active-buffer match.
4. Add a test for replacing `foo` with `foobar` in `foo foo`; after one Replace, active match should be the original second `foo`, adjusted to its new char offset.
5. Add a plain deletion test for replacing `foo` with empty text to ensure selection still advances sensibly after shrinkage.

Acceptance criteria:
- Repeated Replace on `foo foo` with replacement `foobar` produces `foobar foobar`, not `foobarbar...`.
- Existing next/previous search navigation remains unchanged.

### Phase 2: Make replacement expansion fallible and compile once

Files:
- `src/app/services/search.rs`
- `src/app/app_state/search_state/replace.rs`
- `src/app/app_state/search_state/helpers.rs`
- `src/app/app_state/search_state/types.rs` if a plan error type is useful

Steps:
1. Change replacement expansion to return a `Result<String, SearchError>` or a small replacement-specific error.
2. For regex mode, treat `regex.captures(matched_text) == None` as an error instead of returning literal replacement text.
3. Compile `SearchProgram` once when building a Replace All plan, then pass the compiled program into target construction.
4. Replace `unwrap_or_default()` in plan construction with fail-closed behavior.
5. Surface a clear status message such as "Search replace failed because results are stale or the replacement could not be expanded."

Acceptance criteria:
- Regex capture replacements still work for valid captures.
- Stale/non-matching `matched_text` cannot silently insert `$1` literally.
- Invalid replacement planning cannot silently delete text.
- Replace All no longer recompiles the regex once per match.

### Phase 3: Harden stale target handling

Files:
- `src/app/commands.rs`
- `src/app/app_state/search_state/replace.rs`
- `src/app/app_state/search_state/helpers.rs`
- `src/app/app_state/search_state/types.rs`

Steps:
1. Mark search dirty after tab reorder, matching close/split behavior.
2. Consider extending `ReplacementTargetPlan` with a stable file identity, not only `tab_index` plus `buffer_id`.
3. Before applying each multi-buffer target, re-run target validation immediately.
4. If any second-pass validation disagrees with the original plan, fail before applying that target and report stale results.

Acceptance criteria:
- Reordering tabs while search is open makes replace unavailable until results refresh.
- A stale target cannot be applied to a different tab by index.
- Failures report stale results instead of a generic partial-update message when validation catches them.

### Phase 4: Decide and implement multi-buffer Replace All failure semantics

Files:
- `src/app/app_state/search_state/replace.rs`
- `src/app/domain/buffer/document.rs`
- `src/app/domain/buffer/history*` only if cross-buffer rollback is implemented

Preferred conservative path:
1. Keep Replace All as separate per-buffer undo records.
2. Add a strict second-pass validation immediately before the first mutation.
3. If the second-pass check fails for any target, abort before changing anything.
4. Keep the existing mid-loop error path as a last-resort guard, but make the message explicit that some buffers may already have changed if that unexpected path is reached.

Alternative larger path:
1. Add cross-buffer replace transaction support.
2. Track applied targets and roll them back if a later target fails.
3. Add UI/status messaging that explains the rollback.

Recommendation:
- Start with the conservative path. It fixes realistic stale-plan cases with much lower risk than introducing cross-buffer transactional undo.

Acceptance criteria:
- Planned stale changes fail before mutation in normal stale/reorder/closed-target scenarios.
- Unexpected mid-apply failure is no longer presented as if nothing changed.

### Phase 5: Fix Replace All cursor placement

Files:
- `src/app/app_state/search_state/helpers.rs`
- `src/app/app_state/search_state/replace.rs`

Steps:
1. Preserve both document-order and apply-order information, or derive the first document-order match from `target.replacements.last()`.
2. Use the first changed match in document order for fallback selection/cursor placement, unless product intent is to preserve the user's previous cursor.
3. Keep apply order descending for document mutation.

Acceptance criteria:
- Replace All in an active buffer does not jump to the last match solely because replacements are stored in reverse order.
- Cursor behavior is intentional and covered by a focused test.

## Suggested test coverage

Add focused tests at the app/search-state layer where possible:
- `replace_current_advances_past_self_matching_replacement`
- `replace_current_advances_after_empty_replacement`
- `replace_all_regex_capture_expansion_uses_single_compiled_program` if measurable without invasive instrumentation
- `replace_all_fails_when_regex_capture_expansion_does_not_match`
- `replace_all_active_buffer_cursor_uses_first_document_order_match`
- `tab_reorder_marks_search_dirty`

Keep existing document-layer no-op edit tests as-is; they already cover the stale item from the original review.

## Recommended implementation order

1. Phase 1: safest, highest user-facing correctness win.
2. Phase 2: removes silent data-corruption fallbacks and improves Replace All performance.
3. Phase 3: closes stale tab-index behavior.
4. Phase 5: small UX correctness pass.
5. Phase 4 alternative rollback only if product requirements demand true cross-buffer atomicity.
