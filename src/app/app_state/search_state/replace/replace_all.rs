use super::{ScratchpadApp, stale_replacement_match_error};
use crate::app::app_state::search_state::helpers::build_replacement_targets;
use crate::app::app_state::{
    StatusDomain,
    search_state::{ReplacementPlan, ReplacementTargetPlan},
};
use crate::app::services::search::{SearchError, SearchProgram};

pub(super) fn replace_all_search_matches_in_scope(app: &mut ScratchpadApp) -> bool {
    if app.state.search_state.replace.pending.is_some() {
        return false;
    }
    let plan = match build_replace_all_plan(app) {
        Ok(Some(plan)) if plan.total_match_count > 0 => plan,
        Ok(_) => return false,
        Err(error) => {
            app.state
                .status
                .set_error_status_in_domain(StatusDomain::Search, error.message());
            return false;
        }
    };
    let settings = &app.state.app_settings.search;
    let review = settings.review_files_above.exceeded(plan.targets.len())
        || settings
            .review_matches_above
            .exceeded(plan.total_match_count);
    if review && !confirm_replace_all_plan(app, &plan) {
        return false;
    }
    super::super::recent::commit(app);
    super::transaction::start(app, plan)
}

fn confirm_replace_all_plan(app: &mut ScratchpadApp, plan: &ReplacementPlan) -> bool {
    let state = &mut app.state.search_state;
    if state
        .replace
        .pending_replace_all_confirmation
        .as_ref()
        .is_some_and(|c| {
            c.matches_plan(
                plan,
                &state.query.replacement,
                state.runtime.requested_generation,
            )
        })
    {
        state.replace.pending_replace_all_confirmation = None;
        return true;
    }
    state.replace.pending_replace_all_confirmation =
        Some(super::super::ReplaceAllConfirmation::from_plan(
            plan,
            &state.query.replacement,
            state.runtime.requested_generation,
        ));
    false
}

pub(super) fn validate_replacement_target(
    app: &ScratchpadApp,
    target: &ReplacementTargetPlan,
) -> bool {
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(target.buffer_id))
    else {
        return false;
    };
    buffer.document_snapshot().revision_key() == target.target_identity
        && !buffer.is_preparing_text()
        && !buffer.is_loading_preview
        && buffer
            .validate_char_replacements(&target.replacements)
            .is_ok()
}

fn build_replace_all_plan(app: &ScratchpadApp) -> Result<Option<ReplacementPlan>, SearchError> {
    let state = &app.state.search_state;
    if !state.replace_availability().allows_actions() {
        return Ok(None);
    }
    let included: Vec<_> = state
        .results
        .matches
        .iter()
        .enumerate()
        .filter(|(i, _)| !state.replace.excluded.contains(i))
        .map(|(_, m)| m.clone())
        .collect();
    let program = SearchProgram::compile(&state.query.query, state.search_options())?;
    let targets = build_replacement_targets(&included, |matched| {
        let buffer = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|t| t.buffer_by_id(matched.buffer_id))
            .ok_or_else(stale_replacement_match_error)?;
        let context = super::super::selection::resolve(app)
            .filter(|(_, id, _, _)| *id == matched.buffer_id)
            .map(|(_, _, _, r)| r);
        let replacement = super::expand_for_buffer_in_range(
            &program,
            buffer,
            &matched.range,
            &state.query.replacement,
            state.query.preserve_case,
            context,
        )?;
        Ok(replacement)
    })?;
    Ok(Some(ReplacementPlan {
        scope: state.query.scope,
        targets,
        total_match_count: included.len(),
    }))
}
