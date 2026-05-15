use super::helpers::{
    build_search_target, collect_search_targets_for_views, first_match_index, matches_buffer,
};
use super::visual;
use super::worker::{
    SearchFileIdentity, SearchRequest, SearchResult, SearchTargetSnapshot, process_search_request,
};
use super::{
    ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchMatch, SearchScope, SearchStatus,
};
use crate::app::app_state::workspace::display_tabs;
use crate::app::domain::BufferId;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(crate) fn refresh_search_view_state(app: &mut ScratchpadApp) {
    if !search_is_active(app) {
        visual::clear_search_highlights(app);
        return;
    }
    visual::refresh_search_visual_state(app);
}

pub(crate) fn take_search_focus_target(app: &mut ScratchpadApp) -> Option<SearchFocusTarget> {
    app.state.search_state.panel.focus_target.take()
}

pub(crate) fn request_search_focus(app: &mut ScratchpadApp, target: SearchFocusTarget) {
    app.state.search_state.panel.focus_target = Some(target);
}

pub(crate) fn refresh_search_state(app: &mut ScratchpadApp) {
    poll_search_results(app);
    if !search_is_active(app) {
        clear_inactive_search_state(app);
        return;
    }
    if !app.state.search_state.runtime.dirty {
        return;
    }

    if app.state.search_state.query.scope == SearchScope::SelectionOnly
        && app.active_search_selection_range().is_none()
    {
        set_selection_only_search_error(app);
        return;
    }

    submit_search_request(app);
    app.state.search_state.runtime.dirty = false;
}

pub(crate) fn mark_search_dirty(app: &mut ScratchpadApp) {
    if app.state.search_state.panel.open {
        app.state.search_state.runtime.dirty = true;
        if !matches!(app.state.search_state.runtime.status, SearchStatus::Idle) {
            app.state.search_state.runtime.freshness = SearchFreshness::Stale;
        }
    }
}

fn submit_search_request(app: &mut ScratchpadApp) {
    let generation = app
        .state
        .search_state
        .runtime
        .requested_generation
        .saturating_add(1);
    crate::app::app_state::search_visual::clear_search_highlights(app);
    let targets = collect_search_targets(app, app.state.search_state.query.scope);
    let request = app.state.search_state.build_request(generation, targets);
    app.state.search_state.begin_request(generation);

    if let Err(error) = app.state.search_state.runtime.request_tx.send(request) {
        let latest_generation = AtomicU64::new(generation);
        if let Some(result) = process_search_request(error.0, &latest_generation) {
            apply_search_result(app, result);
        }
    }
}

fn poll_search_results(app: &mut ScratchpadApp) {
    let mut latest_result = None;
    while let Ok(result) = app.state.search_state.runtime.result_rx.try_recv() {
        if result.generation == app.state.search_state.runtime.requested_generation {
            latest_result = Some(result);
        }
    }
    if let Some(result) = latest_result {
        apply_search_result(app, result);
    }
}

fn apply_search_result(app: &mut ScratchpadApp, result: SearchResult) {
    let SearchResult {
        generation,
        matches,
        displayed_match_count,
        result_groups,
        status,
    } = result;
    let is_partial = matches!(status, SearchStatus::Searching { .. });
    app.state.search_state.results.active_match_index = preferred_active_match_index(
        app,
        &matches,
        app.state
            .search_state
            .results
            .previous_active_match
            .as_ref(),
    );
    app.state.search_state.results.matches = matches;
    app.state.search_state.results.total_match_count = app.state.search_state.results.matches.len();
    app.state.search_state.results.displayed_match_count = displayed_match_count;
    app.state.search_state.results.result_groups = Arc::from(result_groups);
    app.state.search_state.runtime.searching = is_partial;
    if !is_partial {
        app.state.search_state.results.previous_active_match = None;
    }
    app.state.search_state.runtime.applied_generation = generation;
    app.state.search_state.runtime.status = status;
    app.state.search_state.runtime.freshness = SearchFreshness::Fresh;
    visual::refresh_search_visual_state(app);
}

#[doc(hidden)]
pub fn profile_build_search_request(app: &ScratchpadApp, scope: SearchScope, query: &str) -> usize {
    let generation = app
        .state
        .search_state
        .runtime
        .requested_generation
        .saturating_add(1);
    let targets = collect_search_targets(app, scope);
    let request = SearchRequest {
        generation,
        query: query.to_owned(),
        options: app.state.search_state.search_options(),
        targets,
    };
    request
        .targets
        .iter()
        .map(|target| target.document_snapshot.document_length().chars)
        .sum::<usize>()
        + request.query.len()
}

fn collect_search_targets(app: &ScratchpadApp, scope: SearchScope) -> Vec<SearchTargetSnapshot> {
    match scope {
        SearchScope::SelectionOnly => {
            active_search_target(app, app.active_search_selection_range())
                .into_iter()
                .collect()
        }
        SearchScope::ActiveBuffer => active_search_target(app, None).into_iter().collect(),
        SearchScope::ActiveWorkspaceTab => collect_active_tab_search_targets(app),
        SearchScope::AllOpenTabs => {
            let active_tab_index = app.tab_manager.active_tab_index;
            let mut seen_files = HashSet::<SearchFileIdentity>::new();
            (0..app.tab_manager.tabs.as_slice().len())
                .map(|offset| {
                    (active_tab_index + offset) % app.tab_manager.tabs.as_slice().len().max(1)
                })
                .flat_map(|tab_index| {
                    let prioritized_buffer_id = (tab_index == active_tab_index)
                        .then(|| {
                            app.tab_manager
                                .active_tab()
                                .and_then(|tab| tab.active_view())
                                .map(|view| view.buffer_id)
                        })
                        .flatten();
                    collect_search_targets_for_tab(app, tab_index, prioritized_buffer_id, None)
                })
                .filter(|target| seen_files.insert(target.file_identity.clone()))
                .collect()
        }
    }
}

fn clear_inactive_search_state(app: &mut ScratchpadApp) {
    app.state.search_state.clear_inactive_results();
    app.state.search_state.runtime.status = SearchStatus::Idle;
    app.state.search_state.runtime.freshness = SearchFreshness::Fresh;
    crate::app::app_state::search_visual::clear_search_highlights(app);
}

fn set_selection_only_search_error(app: &mut ScratchpadApp) {
    app.state.search_state.runtime.searching = false;
    app.state.search_state.runtime.status =
        SearchStatus::Error("Selection-only search requires an active selection.".to_owned());
    app.state.search_state.runtime.freshness = SearchFreshness::Fresh;
    app.state.search_state.clear_match_results();
    app.state.search_state.runtime.dirty = false;
    crate::app::app_state::search_visual::clear_search_highlights(app);
}

fn collect_active_tab_search_targets(app: &ScratchpadApp) -> Vec<SearchTargetSnapshot> {
    collect_search_targets_for_tab(
        app,
        app.tab_manager.active_tab_index,
        app.tab_manager
            .active_tab()
            .and_then(|tab| tab.active_view())
            .map(|view| view.buffer_id),
        None,
    )
}

fn active_search_target(
    app: &ScratchpadApp,
    search_range: Option<Range<usize>>,
) -> Option<SearchTargetSnapshot> {
    let tab_index = app.tab_manager.active_tab_index;
    let tab_label = search_tab_label(app, tab_index);
    let tab = app.tab_manager.active_tab()?;
    build_search_target(
        tab_index,
        tab,
        tab.layout.active_view_id(),
        &tab_label,
        search_range,
    )
}

fn collect_search_targets_for_tab(
    app: &ScratchpadApp,
    tab_index: usize,
    prioritized_buffer_id: Option<BufferId>,
    search_range: Option<Range<usize>>,
) -> Vec<SearchTargetSnapshot> {
    let Some(tab) = app.tab_manager.tabs.as_slice().get(tab_index) else {
        return Vec::new();
    };
    let tab_label = search_tab_label(app, tab_index);
    collect_search_targets_for_views(
        tab_index,
        tab,
        &tab_label,
        search_range,
        prioritized_buffer_id,
        tab.ordered_view_ids_in_layout_order()
            .into_iter()
            .filter_map(|view_id| tab.view(view_id))
            .chain(tab.layout.views().iter()),
    )
}

fn search_tab_label(app: &ScratchpadApp, tab_index: usize) -> String {
    display_tabs::display_tab_name_at_slot(
        app,
        display_tabs::slot_for_workspace_index(app, tab_index),
    )
    .unwrap_or_else(|| format!("Tab {}", tab_index + 1))
}

fn preferred_active_match_index(
    app: &ScratchpadApp,
    matches: &[SearchMatch],
    previous_active: Option<&SearchMatch>,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    if let Some(previous_active) = previous_active
        && let Some(index) =
            first_match_index(matches, |search_match| search_match == previous_active)
    {
        return Some(index);
    }

    if let Some((active_tab_index, active_buffer_id)) = visual::active_buffer_identity(app)
        && let Some(index) = first_match_index(matches, |search_match| {
            matches_buffer(search_match, active_tab_index, active_buffer_id)
        })
    {
        return Some(index);
    }

    first_match_index(matches, |search_match| {
        search_match.tab_index == app.tab_manager.active_tab_index
    })
    .or(Some(0))
}

pub(super) fn search_is_active(app: &ScratchpadApp) -> bool {
    app.state.search_state.panel.open && !app.state.search_state.query.query.is_empty()
}
