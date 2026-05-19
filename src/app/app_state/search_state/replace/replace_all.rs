use super::{
    finalize_tab_buffer_mutation, rebuild_active_buffer_search_matches,
    replace_ranges_in_active_buffer,
};
use crate::app::app_state::StatusDomain;
use crate::app::app_state::search_state::helpers::{
    build_replacement_targets, cursor_range_from_char_range, fallback_selection_for_target,
    first_document_order_replacement, next_selection_for_target,
};
use crate::app::app_state::search_state::{
    ReplacementPlan, ReplacementTargetPlan, ScratchpadApp, SearchScope, runtime, visual,
};
use crate::app::domain::CursorRevealMode;
use crate::app::services::search::{SearchError, SearchProgram};

pub(super) fn replace_all_search_matches_in_scope(app: &mut ScratchpadApp) -> bool {
    let plan = match build_replace_all_plan(app) {
        Ok(Some(plan)) => plan,
        Ok(None) => return false,
        Err(error) => {
            app.state
                .status
                .set_error_status_in_domain(StatusDomain::Search, error.message());
            return false;
        }
    };
    if plan.total_match_count == 0 {
        return false;
    }

    if plan.scope == SearchScope::ActiveBuffer && plan.targets.len() == 1 {
        return replace_all_in_active_buffer(app, &plan);
    }

    if plan.requires_confirmation() && !confirm_replace_all_plan(app, &plan) {
        return false;
    }

    let replaced = replace_all_in_multiple_buffers(app, &plan);
    if replaced {
        app.state.status.set_info_status_in_domain(
            StatusDomain::Search,
            format!(
                "Replaced {} matches across {} buffers.",
                plan.total_match_count,
                plan.affected_buffer_count()
            ),
        );
    }
    replaced
}

fn replace_all_in_active_buffer(app: &mut ScratchpadApp, plan: &ReplacementPlan) -> bool {
    if !validate_replacement_plan(app, plan) {
        report_stale_replace_all_plan(app, true);
        return false;
    }

    let target = &plan.targets[0];
    let previous_selection = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.layout.view(target.view_id))
        .and_then(|view| view.cursor_range);
    let (first_range, first_replacement) = first_document_order_replacement(target);
    let previous_selection =
        previous_selection.unwrap_or_else(|| cursor_range_from_char_range(first_range.clone()));
    let next_selection = cursor_range_from_char_range(
        first_range.start..first_range.start + first_replacement.chars().count(),
    );
    let Some(buffer_label) = replace_ranges_in_active_buffer(
        app,
        target.view_id,
        target.buffer_id,
        &target.replacements,
        previous_selection,
        next_selection,
        "Search replace-all failed for the active buffer.",
    ) else {
        return false;
    };
    if let Err(error) = rebuild_active_buffer_search_matches(app) {
        app.state
            .status
            .set_error_status_in_domain(StatusDomain::Search, error.message());
        runtime::mark_search_dirty(app);
        runtime::refresh_search_state(app);
        return false;
    }
    visual::select_first_match_in_active_buffer(app);
    runtime::mark_search_dirty(app);
    runtime::refresh_search_state(app);
    app.state.status.set_info_status_in_domain(
        StatusDomain::Search,
        format!(
            "Replaced {} matches in {}.",
            plan.total_match_count, buffer_label
        ),
    );
    true
}

fn replace_all_in_multiple_buffers(app: &mut ScratchpadApp, plan: &ReplacementPlan) -> bool {
    if !validate_replacement_plan(app, plan) {
        report_stale_replace_all_plan(app, false);
        return false;
    }

    for target in &plan.targets {
        if !validate_replacement_target(app, target) {
            report_stale_replace_all_plan(app, false);
            return false;
        }
        if !apply_replacement_target(app, target) {
            app.state.status.set_error_status_in_domain(
                StatusDomain::Search,
                "Search replace-all stopped after some targets may already have been updated.",
            );
            return false;
        }
    }
    runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    app.state
        .search_state
        .replace
        .pending_replace_all_confirmation = None;
    runtime::refresh_search_state(app);
    true
}

fn confirm_replace_all_plan(app: &mut ScratchpadApp, plan: &ReplacementPlan) -> bool {
    let replacement = app.state.search_state.query.replacement.clone();
    let requested_generation = app.state.search_state.runtime.requested_generation;
    if app
        .state
        .search_state
        .replace
        .pending_replace_all_confirmation
        .as_ref()
        .is_some_and(|confirmation| {
            confirmation.matches_plan(plan, &replacement, requested_generation)
        })
    {
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
        return true;
    }

    let confirmation =
        super::super::ReplaceAllConfirmation::from_plan(plan, &replacement, requested_generation);
    let replacement_preview = if replacement.is_empty() {
        "empty text".to_owned()
    } else {
        format!("\"{replacement}\"")
    };
    app.state
        .search_state
        .replace
        .pending_replace_all_confirmation = Some(confirmation);
    app.state.status.set_info_status_in_domain(StatusDomain::Search, format!(
            "Replace all will change {} matches across {} buffers with {replacement_preview}. Run Replace All again to confirm.",
            plan.total_match_count,
            plan.affected_buffer_count()
        ));
    false
}

fn report_stale_replace_all_plan(app: &mut ScratchpadApp, refresh_search: bool) {
    app.state
        .search_state
        .replace
        .pending_replace_all_confirmation = None;
    app.state.status.report_search_results_stale_for_replace();
    if refresh_search {
        runtime::mark_search_dirty(app);
        runtime::refresh_search_state(app);
    }
}

fn validate_replacement_plan(app: &ScratchpadApp, plan: &ReplacementPlan) -> bool {
    plan.targets
        .iter()
        .all(|target| validate_replacement_target(app, target))
}

fn validate_replacement_target(app: &ScratchpadApp, target: &ReplacementTargetPlan) -> bool {
    let Some(tab) = app.tab_manager.tabs.as_slice().get(target.tab_index) else {
        return false;
    };
    let Some(buffer) = tab.buffer_by_id(target.buffer_id) else {
        return false;
    };
    if buffer.document_revision() != target.target_revision {
        return false;
    }
    if buffer
        .validate_char_replacements(&target.replacements)
        .is_err()
    {
        return false;
    }
    target.expected_matches.iter().all(|(range, expected)| {
        buffer.document().piece_tree().extract_range(range.clone()) == *expected
    })
}

fn build_replace_all_plan(app: &ScratchpadApp) -> Result<Option<ReplacementPlan>, SearchError> {
    if app.state.search_state.results.matches.is_empty() {
        return Ok(None);
    }
    let program = SearchProgram::compile(
        &app.state.search_state.query.query,
        app.state.search_state.search_options(),
    )?;
    let replacement = app.state.search_state.query.replacement.clone();

    Ok(Some(ReplacementPlan {
        scope: app.state.search_state.query.scope,
        total_match_count: app.state.search_state.results.matches.len(),
        targets: build_replacement_targets(
            &app.state.search_state.results.matches,
            |search_match| program.expand_replacement(&search_match.matched_text, &replacement),
        )?,
    }))
}

fn apply_replacement_target(app: &mut ScratchpadApp, target: &ReplacementTargetPlan) -> bool {
    let Some(tab) = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .get_mut(target.tab_index)
    else {
        return false;
    };
    let previous_selection = tab
        .layout
        .view(target.view_id)
        .and_then(|view| view.cursor_range)
        .unwrap_or_else(|| fallback_selection_for_target(target));
    let next_selection = next_selection_for_target(target);
    let Some(buffer) = tab.buffer_by_id_mut(target.buffer_id) else {
        return false;
    };
    if buffer
        .replace_char_ranges_with_undo(&target.replacements, previous_selection, next_selection)
        .is_err()
    {
        return false;
    }
    if let Some((buffer, view)) = tab.buffer_and_view_mut(target.view_id) {
        view.set_cursor_range_anchored(buffer, next_selection);
        view.set_pending_cursor_range_anchored(buffer, next_selection);
        view.request_cursor_reveal(CursorRevealMode::Center);
    }
    finalize_tab_buffer_mutation(app, target.tab_index, target.buffer_id);
    true
}
