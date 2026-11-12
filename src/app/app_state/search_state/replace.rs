use super::{ReplacementPlan, ReplacementTargetPlan, ScratchpadApp, runtime};
use crate::app::app_state::StatusDomain;
use crate::app::domain::BufferId;
use crate::app::services::search::{SearchError, SearchProgram};
use std::ops::Range;
pub(crate) mod limited;
mod replace_all;
pub(crate) mod transaction;

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
    let Some(matched) = app.state.search_state.results.matches.get(index).cloned() else {
        return false;
    };
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|t| t.buffer_by_id(matched.buffer_id))
    else {
        return false;
    };
    if buffer.document_snapshot().revision_key() != matched.target_identity {
        runtime::mark_search_dirty(app);
        return false;
    }
    let replacement = match replacement_for_match(app, &matched) {
        Ok(text) => text,
        Err(error) => {
            app.state
                .status
                .set_error_status_in_domain(StatusDomain::Search, error.message());
            return false;
        }
    };
    let plan = ReplacementPlan {
        scope: app.state.search_state.query.scope,
        total_match_count: 1,
        targets: vec![ReplacementTargetPlan {
            tab_index: matched.tab_index,
            view_id: matched.view_id,
            buffer_id: matched.buffer_id,
            buffer_label: matched.buffer_label.to_string(),
            target_revision: matched.target_revision,
            target_identity: matched.target_identity,
            replacements: vec![(matched.range, replacement)],
        }],
    };
    if !super::api::activate_search_match(app, index) {
        return false;
    }
    transaction::start_single(app, plan)
}
pub(crate) fn replace_all_search_matches_in_scope(app: &mut ScratchpadApp) -> bool {
    replace_all::replace_all_search_matches_in_scope(app)
}
pub(crate) fn replacement_for_match(
    app: &ScratchpadApp,
    matched: &super::SearchMatch,
) -> Result<String, SearchError> {
    let program = SearchProgram::compile(
        &app.state.search_state.query.query,
        app.state.search_state.search_options(),
    )?;
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|t| t.buffer_by_id(matched.buffer_id))
        .ok_or_else(stale_replacement_match_error)?;
    if buffer.document_snapshot().revision_key() != matched.target_identity {
        return Err(stale_replacement_match_error());
    }
    let context = super::selection::resolve(app)
        .filter(|(_, id, _, _)| *id == matched.buffer_id)
        .map(|(_, _, _, r)| r);
    expand_for_buffer_in_range(
        &program,
        buffer,
        &matched.range,
        &app.state.search_state.query.replacement,
        app.state.search_state.query.preserve_case,
        context,
    )
}

pub(crate) fn expand_for_buffer_in_range(
    program: &SearchProgram,
    buffer: &crate::app::domain::BufferState,
    range: &Range<usize>,
    replacement: &str,
    preserve: bool,
    context_range: Option<Range<usize>>,
) -> Result<String, SearchError> {
    let tree = buffer.document().piece_tree();
    let domain = context_range.unwrap_or(0..tree.len_chars());
    if range.start > range.end || range.start < domain.start || range.end > domain.end {
        return Err(stale_replacement_match_error());
    }
    if program.options().mode == crate::app::services::search::SearchMode::PlainText && !preserve {
        return Ok(
            crate::app::domain::buffer::normalize_inserted_text_line_endings(
                replacement,
                buffer.format.preferred_line_ending_style(),
            )
            .into_owned(),
        );
    }
    // Bounded patterns inspect at most their match plus one context character
    // on either side. That character preserves word and line assertions.
    let start = range.start.saturating_sub(1).max(domain.start);
    let end = range.end.saturating_add(2).min(domain.end);
    let context = tree.extract_range(start..end);
    let byte_start = context
        .chars()
        .take(range.start - start)
        .map(char::len_utf8)
        .sum();
    let byte_len: usize = context
        .chars()
        .skip(range.start - start)
        .take(range.len())
        .map(char::len_utf8)
        .sum();
    let expanded =
        program.expand_replacement_at(&context, byte_start..byte_start + byte_len, replacement)?;
    let expanded = if preserve {
        crate::app::services::search::preserve_replacement_case(
            &tree.extract_range(range.clone()),
            &expanded,
        )
    } else {
        expanded
    };
    Ok(
        crate::app::domain::buffer::normalize_inserted_text_line_endings(
            &expanded,
            buffer.format.preferred_line_ending_style(),
        )
        .into_owned(),
    )
}

pub(crate) fn matched_text_for_search_match(
    app: &ScratchpadApp,
    search_match: &super::SearchMatch,
) -> Option<String> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|t| t.buffer_by_id(search_match.buffer_id))?;
    (buffer.document_snapshot().revision_key() == search_match.target_identity).then(|| {
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
    app.note_settings_toml_edit(tab_index);
}
