use super::ScratchpadApp;
use super::api::activate_search_match;
use super::helpers::cursor_range_from_char_range;
use super::{runtime, visual};
use crate::app::app_state::StatusDomain;
use crate::app::app_state::workspace::mutation as workspace_mutation;
use crate::app::domain::{BufferId, CursorRevealMode, ViewId};
use crate::app::services::search::{SearchError, SearchProgram, search_program};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::ops::Range;

mod replace_all;

pub fn replace_current_search_match(app: &mut ScratchpadApp) -> bool {
    let Some((index, search_match)) = active_search_match_for_replace(app) else {
        return false;
    };
    if !validate_search_match_for_replace(app, &search_match) {
        invalidate_stale_replace_target(app);
        return false;
    }
    if !activate_search_match(app, index) {
        return false;
    }

    let replacement = match replacement_for_match(app, &search_match) {
        Ok(replacement) => replacement,
        Err(error) => {
            report_search_error(app, error);
            return false;
        }
    };
    let (previous_selection, next_selection, replacement_range) =
        active_match_replacement_selection(app, &search_match, replacement.chars().count());
    let replacements = vec![(search_match.range.clone(), replacement)];

    if replace_ranges_in_active_buffer(
        app,
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

    if !rebuild_active_buffer_search_matches_or_report(app) {
        return false;
    }
    visual::select_next_active_buffer_match_from(app, replacement_range.end);
    runtime::mark_search_dirty(app);
    runtime::refresh_search_state(app);
    true
}

fn active_search_match_for_replace(app: &ScratchpadApp) -> Option<(usize, super::SearchMatch)> {
    if !app
        .state
        .search_state
        .replace_availability()
        .allows_actions()
    {
        return None;
    }
    let index = app.state.search_state.results.active_match_index?;
    let search_match = app.state.search_state.results.matches.get(index)?.clone();
    Some((index, search_match))
}

fn invalidate_stale_replace_target(app: &mut ScratchpadApp) {
    app.state.search_state.clear_replace_all_confirmation();
    app.state.status.report_search_results_stale_for_replace();
    runtime::mark_search_dirty(app);
    runtime::refresh_search_state(app);
}

fn active_match_replacement_selection(
    app: &ScratchpadApp,
    search_match: &super::SearchMatch,
    replacement_char_count: usize,
) -> (CursorRange, CursorRange, Range<usize>) {
    let previous_selection = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.layout.view(search_match.view_id))
        .and_then(|view| view.cursor_range)
        .unwrap_or_else(|| cursor_range_from_char_range(search_match.range.clone()));
    let replacement_range =
        search_match.range.start..search_match.range.start + replacement_char_count;
    let next_selection = cursor_range_from_char_range(replacement_range.clone());
    (previous_selection, next_selection, replacement_range)
}

fn rebuild_active_buffer_search_matches_or_report(app: &mut ScratchpadApp) -> bool {
    if let Err(error) = rebuild_active_buffer_search_matches(app) {
        report_search_error(app, error);
        runtime::mark_search_dirty(app);
        runtime::refresh_search_state(app);
        return false;
    }
    true
}

fn report_search_error(app: &mut ScratchpadApp, error: SearchError) {
    app.state
        .status
        .set_error_status_in_domain(StatusDomain::Search, error.message());
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
                .map_or_else(|| buffer.name.clone(), |path| path.display().to_string())
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
    replace_all::replace_all_search_matches_in_scope(app)
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
        && search_match.matched_text.as_ref().is_none_or(|expected| {
            buffer
                .document()
                .piece_tree()
                .extract_range(search_match.range.clone())
                == *expected
        })
}

fn replacement_for_match(
    app: &ScratchpadApp,
    search_match: &super::SearchMatch,
) -> Result<String, SearchError> {
    let program = SearchProgram::compile(
        &app.state.search_state.query.query,
        app.state.search_state.search_options(),
    )?;
    let matched_text = matched_text_for_search_match(app, search_match)
        .ok_or_else(stale_replacement_match_error)?;
    program.expand_replacement(&matched_text, &app.state.search_state.query.replacement)
}

pub(super) fn matched_text_for_search_match(
    app: &ScratchpadApp,
    search_match: &super::SearchMatch,
) -> Option<String> {
    if let Some(matched_text) = &search_match.matched_text {
        return Some(matched_text.clone());
    }
    let tab = app
        .tab_manager
        .tabs
        .as_slice()
        .get(search_match.tab_index)?;
    let buffer = tab.buffer_by_id(search_match.buffer_id)?;
    (buffer.document_revision() == search_match.target_revision).then(|| {
        buffer
            .document()
            .piece_tree()
            .extract_range(search_match.range.clone())
    })
}

pub(super) fn stale_replacement_match_error() -> SearchError {
    SearchError::ReplacementMismatch(
        "Search replacement could not be expanded for stale search results.".to_owned(),
    )
}

pub(super) fn rebuild_active_buffer_search_matches(
    app: &mut ScratchpadApp,
) -> Result<(), SearchError> {
    let active_tab_index = app.tab_manager.active_tab_index;
    let Some(tab) = app.tab_manager.tabs.as_slice().get(active_tab_index) else {
        return Ok(());
    };
    let active_view_id = tab.layout.active_view_id;
    let Some(buffer) = tab
        .layout
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
        matched_text: None,
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

pub(super) fn finalize_tab_buffer_mutation(
    app: &mut ScratchpadApp,
    tab_index: usize,
    buffer_id: BufferId,
) {
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
    let mut text_metadata_refresh = None;
    if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
        buffer.mark_dirty_after_local_edit();
        text_metadata_refresh = buffer.text_metadata_refresh_needed().then(|| {
            (
                buffer.id,
                buffer.document_revision(),
                buffer.document_snapshot(),
                buffer.format.clone(),
            )
        });
    }
    let _ = tab;
    if let Some((buffer_id, revision, snapshot, format)) = text_metadata_refresh {
        app.queue_background_text_metadata_refresh(buffer_id, revision, snapshot, format);
    }
    workspace_mutation::record_pending_text_history_event(app, tab_index, buffer_id);
    app.note_settings_toml_edit(tab_index);
}
