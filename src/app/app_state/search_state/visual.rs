use super::helpers::{
    cursor_range_from_char_range, matches_buffer, search_highlight_state_for_view,
};
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
    let active_buffer_id = app.tab_manager.active_tab()?.active_view()?.buffer_id;
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
    let Some(view_id) = app.tab_manager.active_tab().map(|tab| tab.active_view_id) else {
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
    if !app.search_is_active() || app.state.search_state.runtime.searching {
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

fn search_replacement_previews_for_tab(
    app: &ScratchpadApp,
    tab_index: usize,
) -> Vec<(ViewId, SearchReplacementPreview)> {
    if !app.state.search_state.panel.replace_open
        || app.state.search_state.runtime.status != SearchStatus::Ready
    {
        return Vec::new();
    }
    let Ok(program) = SearchProgram::compile(
        &app.state.search_state.query.query,
        app.state.search_state.search_options(),
    ) else {
        return Vec::new();
    };

    app.tab_manager
        .tabs
        .as_slice()
        .get(tab_index)
        .map(|tab| {
            tab.views
                .iter()
                .filter_map(|view| {
                    let buffer = tab.buffer_by_id(view.buffer_id)?;
                    let entries = app
                        .state
                        .search_state
                        .results
                        .matches
                        .iter()
                        .filter(|search_match| {
                            matches_buffer(search_match, tab_index, view.buffer_id)
                                && search_match.target_revision == buffer.document_revision()
                        })
                        .map(|search_match| {
                            program
                                .expand_replacement(
                                    &search_match.matched_text,
                                    &app.state.search_state.query.replacement,
                                )
                                .map(|replacement| SearchReplacementPreviewEntry {
                                    range: search_match.range.clone(),
                                    replacement,
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .ok()?;
                    (!entries.is_empty()).then_some((view.id, SearchReplacementPreview { entries }))
                })
                .collect()
        })
        .unwrap_or_default()
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
            tab.views
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
    let mut anchors_to_release = Vec::new();
    for view in &mut tab.views {
        for anchor in view.clear_search_highlights_for_release() {
            anchors_to_release.push((view.buffer_id, anchor));
        }
    }
    for (buffer_id, anchor) in anchors_to_release {
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchor(anchor);
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

macro_rules! compat_scratchpad_app_methods {
    ($type:ty { $($item:item)* }) => {
        #[allow(dead_code)]
        impl $type {
            $($item)*
        }
    };
}

compat_scratchpad_app_methods!(ScratchpadApp {
    pub(super) fn active_buffer_identity(&self) -> Option<(usize, BufferId)> {
        active_buffer_identity(self)
    }

    pub(super) fn refresh_search_visual_state(&mut self) {
        refresh_search_visual_state(self)
    }

    pub(super) fn set_active_search_index(&mut self, index: Option<usize>) {
        set_active_search_index(self, index)
    }

    pub(crate) fn select_next_active_buffer_match_from(&mut self, minimum_start: usize) {
        select_next_active_buffer_match_from(self, minimum_start)
    }

    pub(super) fn select_first_match_in_active_buffer(&mut self) {
        select_first_match_in_active_buffer(self)
    }

    pub(super) fn clear_search_highlights(&mut self) {
        clear_search_highlights(self)
    }
});
