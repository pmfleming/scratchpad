use super::api::activate_search_match;
use super::helpers::{
    build_replacement_targets, cursor_range_from_char_range, fallback_selection_for_target,
    first_document_order_replacement, next_selection_for_target,
};
use super::{ReplacementPlan, ReplacementTargetPlan, ScratchpadApp, SearchScope};
use crate::app::app_state::StatusDomain;
use crate::app::domain::{BufferId, CursorRevealMode, ViewId};
use crate::app::services::search::{SearchError, SearchProgram, search_program};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::ops::Range;

pub fn replace_current_search_match(app: &mut ScratchpadApp) -> bool {
    if !app
        .state
        .search_state
        .replace_availability()
        .allows_actions()
    {
        return false;
    }
    let Some(index) = app.state.search_state.results.active_match_index else {
        return false;
    };
    let Some(search_match) = app.state.search_state.results.matches.get(index).cloned() else {
        return false;
    };
    if !validate_search_match_for_replace(app, &search_match) {
        app.state.search_state.clear_replace_all_confirmation();
        app.state.status.report_search_results_stale_for_replace();
        app.mark_search_dirty();
        app.refresh_search_state();
        return false;
    }
    if !activate_search_match(app, index) {
        return false;
    }

    let replacement = match replacement_for_match(app, &search_match) {
        Ok(replacement) => replacement,
        Err(error) => {
            app.state
                .status
                .set_error_status_in_domain(StatusDomain::Search, error.message());
            return false;
        }
    };
    let replacement_char_count = replacement.chars().count();
    let previous_selection = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.view(search_match.view_id))
        .and_then(|view| view.cursor_range)
        .unwrap_or_else(|| cursor_range_from_char_range(search_match.range.clone()));
    let replacement_range =
        search_match.range.start..search_match.range.start + replacement_char_count;
    let next_selection = cursor_range_from_char_range(replacement_range.clone());
    let replacements = vec![(search_match.range.clone(), replacement)];

    if app
        .replace_ranges_in_active_buffer(
            search_match.view_id,
            search_match.buffer_id,
            &replacements,
            previous_selection,
            next_selection,
            "Search replace failed for the active match.",
        )
        .is_none()
    {
        return false;
    }

    if let Err(error) = rebuild_active_buffer_search_matches(app) {
        app.state
            .status
            .set_error_status_in_domain(StatusDomain::Search, error.message());
        app.mark_search_dirty();
        app.refresh_search_state();
        return false;
    }
    app.select_next_active_buffer_match_from(replacement_range.end);
    app.mark_search_dirty();
    app.refresh_search_state();
    true
}

pub(super) fn replace_ranges_in_active_buffer(
    app: &mut ScratchpadApp,
    view_id: ViewId,
    buffer_id: BufferId,
    replacements: &[(Range<usize>, String)],
    previous_selection: CursorRange,
    next_selection: CursorRange,
    error_message: &str,
) -> Option<String> {
    let active_tab_index = app.tab_manager.active_tab_index;
    let buffer_label = app
        .tab_manager
        .tabs
        .as_slice()
        .get(active_tab_index)
        .and_then(|tab| tab.buffer_by_id(buffer_id))
        .map(|buffer| {
            buffer
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| buffer.name.clone())
        })?;

    let replaced = {
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
        let buffer = tab.buffer_by_id_mut(buffer_id)?;
        if buffer
            .replace_char_ranges_with_undo(replacements, previous_selection, next_selection)
            .is_err()
        {
            false
        } else {
            if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
                view.set_cursor_range_anchored(buffer, next_selection);
                view.set_pending_cursor_range_anchored(buffer, next_selection);
                view.request_cursor_reveal(CursorRevealMode::Center);
            }
            true
        }
    };
    if !replaced {
        app.state
            .status
            .set_error_status_in_domain(StatusDomain::Search, error_message);
        return None;
    }

    finalize_tab_buffer_mutation(app, active_tab_index, buffer_id);
    Some(buffer_label)
}

pub(crate) fn replace_all_search_matches_in_scope(app: &mut ScratchpadApp) -> bool {
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
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
        app.state.status.report_search_results_stale_for_replace();
        app.mark_search_dirty();
        app.refresh_search_state();
        return false;
    }

    let target = &plan.targets[0];
    let previous_selection = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.view(target.view_id))
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
        app.mark_search_dirty();
        app.refresh_search_state();
        return false;
    }
    app.select_first_match_in_active_buffer();
    app.mark_search_dirty();
    app.refresh_search_state();
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
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
        app.state.status.report_search_results_stale_for_replace();
        return false;
    }

    for target in &plan.targets {
        if !validate_replacement_target(app, target) {
            app.state
                .search_state
                .replace
                .pending_replace_all_confirmation = None;
            app.state.status.report_search_results_stale_for_replace();
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
    app.mark_search_dirty();
    app.tab_manager.mark_session_dirty();
    app.state
        .search_state
        .replace
        .pending_replace_all_confirmation = None;
    app.refresh_search_state();
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
        super::ReplaceAllConfirmation::from_plan(plan, &replacement, requested_generation);
    let replacement_preview = if replacement.is_empty() {
        "empty text".to_owned()
    } else {
        format!("\"{}\"", replacement)
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

fn validate_search_match_for_replace(
    app: &ScratchpadApp,
    search_match: &super::SearchMatch,
) -> bool {
    let Some(tab) = app.tab_manager.tabs.as_slice().get(search_match.tab_index) else {
        return false;
    };
    let Some(buffer) = tab.buffer_by_id(search_match.buffer_id) else {
        return false;
    };
    buffer.document_revision() == search_match.target_revision
        && buffer
            .validate_char_replacements(&[(search_match.range.clone(), String::new())])
            .is_ok()
        && buffer
            .document()
            .piece_tree()
            .extract_range(search_match.range.clone())
            == search_match.matched_text
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

fn replacement_for_match(
    app: &ScratchpadApp,
    search_match: &super::SearchMatch,
) -> Result<String, SearchError> {
    let program = SearchProgram::compile(
        &app.state.search_state.query.query,
        app.state.search_state.search_options(),
    )?;
    program.expand_replacement(
        &search_match.matched_text,
        &app.state.search_state.query.replacement,
    )
}

fn rebuild_active_buffer_search_matches(app: &mut ScratchpadApp) -> Result<(), SearchError> {
    let active_tab_index = app.tab_manager.active_tab_index;
    let Some(tab) = app.tab_manager.tabs.as_slice().get(active_tab_index) else {
        return Ok(());
    };
    let active_view_id = tab.active_view_id;
    let Some(buffer) = tab
        .active_view()
        .and_then(|view| tab.buffer_by_id(view.buffer_id))
    else {
        return Ok(());
    };
    let buffer_id = buffer.id;
    let buffer_label = buffer.display_name();
    let target_revision = buffer.document_revision();
    let text = buffer.text();
    let program = SearchProgram::compile(
        &app.state.search_state.query.query,
        app.state.search_state.search_options(),
    )?;
    let ranges = search_program(&text, &program).matches;

    let insertion_index = app
        .state
        .search_state
        .results
        .matches
        .iter()
        .position(|search_match| {
            search_match.tab_index == active_tab_index && search_match.buffer_id == buffer_id
        })
        .unwrap_or(app.state.search_state.results.matches.len());
    app.state
        .search_state
        .results
        .matches
        .retain(|search_match| {
            search_match.tab_index != active_tab_index || search_match.buffer_id != buffer_id
        });

    let new_matches = ranges.into_iter().map(|range| super::SearchMatch {
        tab_index: active_tab_index,
        view_id: active_view_id,
        buffer_id,
        buffer_label: buffer_label.clone(),
        target_revision,
        matched_text: text
            .chars()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .collect(),
        range,
    });
    app.state
        .search_state
        .results
        .matches
        .splice(insertion_index..insertion_index, new_matches);
    app.state.search_state.results.total_match_count = app.state.search_state.results.matches.len();
    app.state.search_state.results.displayed_match_count =
        app.state.search_state.results.matches.len();
    Ok(())
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

fn finalize_tab_buffer_mutation(app: &mut ScratchpadApp, tab_index: usize, buffer_id: BufferId) {
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
    if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
        buffer.mark_dirty_after_local_edit();
    }
    let _ = tab;
    app.record_pending_text_history_event(tab_index, buffer_id);
    app.note_settings_toml_edit(tab_index);
}

macro_rules! compat_scratchpad_app_methods {
    ($type:ty { $($item:item)* }) => {
        #[allow(dead_code)]
        impl $type {
            $($item)*
        }
    };
}

compat_scratchpad_app_methods!(ScratchpadApp {
    pub fn replace_current_search_match(&mut self) -> bool {
        replace_current_search_match(self)
    }

    pub(super) fn replace_ranges_in_active_buffer(&mut self, view_id: ViewId, buffer_id: BufferId, replacements: &[(Range<usize>, String)], previous_selection: CursorRange, next_selection: CursorRange, error_message: &str) -> Option<String> {
        replace_ranges_in_active_buffer(self, view_id, buffer_id, replacements, previous_selection, next_selection, error_message)
    }

    pub(crate) fn replace_all_search_matches_in_scope(&mut self) -> bool {
        replace_all_search_matches_in_scope(self)
    }
});
