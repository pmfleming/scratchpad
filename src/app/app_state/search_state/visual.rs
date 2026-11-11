use super::helpers::{
    cursor_range_from_char_range, matches_buffer, search_highlight_state_for_view,
};
use super::runtime;
use super::{ScratchpadApp, SearchStatus};
use crate::app::domain::{
    BufferId, CursorRevealMode, EditorViewState, SearchHighlightState, SearchReplacementPreview,
    SearchReplacementPreviewEntry, ViewId,
};
use crate::app::services::search::SearchProgram;
use crate::app::ui::scrolling::{ScrollAlign, ScrollIntent};
use eframe::egui;
use std::ops::Range;
use std::sync::Arc;

pub(super) fn active_buffer_identity(app: &ScratchpadApp) -> Option<(usize, BufferId)> {
    let active_tab_index = app.tab_manager.active_tab_index;
    let active_buffer_id = app
        .tab_manager
        .active_tab()?
        .layout
        .active_view()?
        .buffer_id;
    Some((active_tab_index, active_buffer_id))
}

fn active_buffer_match_index_at_or_after(
    app: &ScratchpadApp,
    minimum_start: usize,
) -> Option<usize> {
    let (active_tab_index, active_buffer_id) = active_buffer_identity(app)?;
    app.state
        .search_state
        .results
        .matches
        .iter()
        .position(|search_match| {
            matches_buffer(search_match, active_tab_index, active_buffer_id)
                && search_match.range.start >= minimum_start
        })
        .or_else(|| {
            app.state
                .search_state
                .results
                .matches
                .iter()
                .position(|search_match| {
                    matches_buffer(search_match, active_tab_index, active_buffer_id)
                })
        })
}

fn active_search_match_range(app: &ScratchpadApp) -> Option<Range<usize>> {
    app.state
        .search_state
        .results
        .active_match_index
        .and_then(|index| app.state.search_state.results.matches.get(index))
        .map(|search_match| search_match.range.clone())
}

fn sync_active_search_cursor(app: &mut ScratchpadApp) {
    let Some(search_range) = active_search_match_range(app) else {
        return;
    };
    let cursor_range = cursor_range_from_char_range(search_range.clone());
    let active_tab_index = app.tab_manager.active_tab_index;
    let Some(view_id) = app
        .tab_manager
        .active_tab()
        .map(|tab| tab.layout.active_view_id())
    else {
        return;
    };
    if let Some((buffer, view)) =
        app.tab_manager.tabs.as_mut_slice()[active_tab_index].buffer_and_view_mut(view_id)
    {
        view.set_pending_cursor_range_anchored(buffer, cursor_range);
        if request_search_reveal_intent(view, &search_range) {
            view.request_cursor_reveal(CursorRevealMode::KeepHorizontalVisible);
        } else {
            view.request_cursor_reveal(CursorRevealMode::Center);
        }
    }
}

pub(super) fn refresh_search_visual_state(app: &mut ScratchpadApp) {
    sync_search_result_group_activity(app);
    apply_search_highlights(app);
}

pub(super) fn set_active_search_index(app: &mut ScratchpadApp, index: Option<usize>) {
    app.state.search_state.results.active_match_index = index;
    sync_active_search_cursor(app);
    refresh_search_visual_state(app);
}

pub(crate) fn select_next_active_buffer_match_from(app: &mut ScratchpadApp, minimum_start: usize) {
    set_active_search_index(
        app,
        active_buffer_match_index_at_or_after(app, minimum_start),
    );
}

pub(super) fn select_first_match_in_active_buffer(app: &mut ScratchpadApp) {
    set_active_search_index(app, active_buffer_match_index_at_or_after(app, 0));
}

fn sync_search_result_group_activity(app: &mut ScratchpadApp) {
    let active_match_index = app.state.search_state.results.active_match_index;
    for group in Arc::make_mut(&mut app.state.search_state.results.result_groups) {
        group.active = active_match_index.is_some_and(|index| {
            let start = group.first_match_index;
            let end = start.saturating_add(group.total_match_count);
            (start..end).contains(&index)
        });
    }
}

fn apply_search_highlights(app: &mut ScratchpadApp) {
    if !runtime::search_is_active(app) || app.state.search_state.runtime.searching {
        clear_search_highlights(app);
        return;
    }

    if !matches!(
        app.state.search_state.runtime.status,
        SearchStatus::Ready | SearchStatus::NoMatches
    ) {
        clear_search_highlights(app);
        return;
    }

    let active_tab_index = app.tab_manager.active_tab_index;
    let highlights = search_highlights_for_tab(app, active_tab_index);
    let replacement_previews = search_replacement_previews_for_tab(app, active_tab_index);

    clear_search_highlights_outside_tab(app, active_tab_index);
    let Some(tab) = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .get_mut(active_tab_index)
    else {
        return;
    };

    for (view_id, highlights) in highlights {
        if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
            view.set_search_highlights_anchored(buffer, highlights);
            let preview = replacement_previews
                .iter()
                .find(|(preview_view_id, _)| *preview_view_id == view_id)
                .map(|(_, preview)| preview.clone());
            view.set_search_replacement_preview(preview);
        }
    }
}

#[derive(Default)]
pub(super) struct ReplacementPreviewCache {
    key: Option<(
        u64,
        String,
        String,
        crate::app::services::search::SearchOptions,
    )>,
    program: Option<SearchProgram>,
    buffers: std::collections::HashMap<
        BufferId,
        (
            crate::app::domain::buffer::HistoryToken,
            SearchReplacementPreview,
        ),
    >,
}

fn search_replacement_previews_for_tab(
    app: &mut ScratchpadApp,
    tab_index: usize,
) -> Vec<(ViewId, SearchReplacementPreview)> {
    let state = &mut app.state.search_state;
    let options = state.search_options();
    let cache = &mut state.replacement_previews;
    if !state.panel.replace_open || state.runtime.status != SearchStatus::Ready {
        *cache = ReplacementPreviewCache::default();
        return Vec::new();
    }
    let current =
        cache
            .key
            .as_ref()
            .is_some_and(|(generation, query, replacement, cached_options)| {
                *generation == state.runtime.applied_generation
                    && *query == state.query.query
                    && *replacement == state.query.replacement
                    && *cached_options == options
            });
    if !current {
        cache.key = Some((
            state.runtime.applied_generation,
            state.query.query.clone(),
            state.query.replacement.clone(),
            options,
        ));
        cache.program = SearchProgram::compile(&state.query.query, options).ok();
        cache.buffers.clear();
    }
    let Some(program) = cache.program.as_ref() else {
        return Vec::new();
    };
    let Some(tab) = app.tab_manager.tabs.get(tab_index) else {
        return Vec::new();
    };
    let mut previews = Vec::new();
    for view in tab.layout.views() {
        let Some(buffer) = tab.buffer_by_id(view.buffer_id) else {
            continue;
        };
        let token = buffer.document().history_token();
        if !cache
            .buffers
            .get(&buffer.id)
            .is_some_and(|(cached, _)| *cached == token)
        {
            let entries = state
                .results
                .matches
                .iter()
                .filter(|matched| {
                    matches_buffer(matched, tab_index, buffer.id)
                        && matched.target_revision == buffer.document_revision()
                })
                .map(|matched| {
                    let replacement =
                        if options.mode == crate::app::services::search::SearchMode::PlainText {
                            state.query.replacement.clone()
                        } else {
                            let text = matched.matched_text.clone().unwrap_or_else(|| {
                                buffer
                                    .document()
                                    .piece_tree()
                                    .extract_range(matched.range.clone())
                            });
                            program.expand_replacement(&text, &state.query.replacement)?
                        };
                    Ok(SearchReplacementPreviewEntry {
                        range: matched.range.clone(),
                        replacement,
                    })
                })
                .collect::<Result<Vec<_>, crate::app::services::search::SearchError>>();
            let Ok(entries) = entries else {
                continue;
            };
            cache
                .buffers
                .insert(buffer.id, (token, SearchReplacementPreview::new(entries)));
        }
        let preview = &cache.buffers[&buffer.id].1;
        if !preview.entries().is_empty() {
            previews.push((view.id, preview.clone()));
        }
    }
    previews
}

fn search_highlights_for_tab(
    app: &ScratchpadApp,
    tab_index: usize,
) -> Vec<(ViewId, SearchHighlightState)> {
    app.tab_manager
        .tabs
        .as_slice()
        .get(tab_index)
        .map(|tab| {
            tab.layout
                .views()
                .iter()
                .map(|view| {
                    (
                        view.id,
                        search_highlight_state_for_view(
                            tab_index,
                            view.buffer_id,
                            &app.state.search_state.results.matches,
                            app.state.search_state.results.active_match_index,
                        ),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn clear_search_highlights(app: &mut ScratchpadApp) {
    app.state.search_state.replacement_previews = ReplacementPreviewCache::default();
    for tab_index in 0..app.tab_manager.tabs.as_slice().len() {
        clear_search_highlights_for_tab(app, tab_index);
    }
}

fn clear_search_highlights_outside_tab(app: &mut ScratchpadApp, active_tab_index: usize) {
    for tab_index in 0..app.tab_manager.tabs.as_slice().len() {
        if tab_index != active_tab_index {
            clear_search_highlights_for_tab(app, tab_index);
        }
    }
}

fn clear_search_highlights_for_tab(app: &mut ScratchpadApp, tab_index: usize) {
    let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(tab_index) else {
        return;
    };
    let mut anchors_to_release = std::collections::HashMap::<_, Vec<_>>::new();
    for view in tab.layout.views_mut() {
        for anchor in view.clear_search_highlights_for_release() {
            anchors_to_release
                .entry(view.buffer_id)
                .or_default()
                .push(anchor);
        }
    }
    for (buffer_id, anchors) in anchors_to_release {
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchors(&anchors);
        }
    }
}

fn request_search_reveal_intent(view: &mut EditorViewState, search_range: &Range<usize>) -> bool {
    let Some(snapshot) = view.latest_display_snapshot.as_ref() else {
        return false;
    };
    let Some(y) = snapshot.pixel_y_for_char_offset(search_range.start as u32) else {
        return false;
    };
    let row_height = snapshot
        .row_height()
        .max(view.scroll.metrics().row_height)
        .max(1.0);
    view.request_intent(ScrollIntent::Reveal {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, y), egui::vec2(1.0, row_height)),
        align_y: Some(ScrollAlign::Center),
        align_x: None,
    });
    true
}
