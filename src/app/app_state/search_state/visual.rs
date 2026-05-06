use super::helpers::{
    cursor_range_from_char_range, matches_buffer, search_highlight_state_for_view,
};
use super::{ScratchpadApp, SearchStatus};
use crate::app::domain::{
    BufferId, CursorRevealMode, EditorViewState, SearchHighlightState, ViewId,
};
use crate::app::ui::scrolling::{ScrollAlign, ScrollIntent};
use eframe::egui;
use std::ops::Range;
use std::sync::Arc;

impl ScratchpadApp {
    pub(super) fn active_buffer_identity(&self) -> Option<(usize, BufferId)> {
        let active_tab_index = self.active_tab_index();
        let active_buffer_id = self.active_tab()?.active_view()?.buffer_id;
        Some((active_tab_index, active_buffer_id))
    }

    fn active_buffer_match_index_at_or_after(&self, minimum_start: usize) -> Option<usize> {
        let (active_tab_index, active_buffer_id) = self.active_buffer_identity()?;
        self.search_state
            .matches
            .iter()
            .position(|search_match| {
                matches_buffer(search_match, active_tab_index, active_buffer_id)
                    && search_match.range.start >= minimum_start
            })
            .or_else(|| {
                self.search_state.matches.iter().position(|search_match| {
                    matches_buffer(search_match, active_tab_index, active_buffer_id)
                })
            })
    }

    fn active_search_match_range(&self) -> Option<Range<usize>> {
        self.search_state
            .active_match_index
            .and_then(|index| self.search_state.matches.get(index))
            .map(|search_match| search_match.range.clone())
    }

    fn sync_active_search_cursor(&mut self) {
        let Some(search_range) = self.active_search_match_range() else {
            return;
        };
        let cursor_range = cursor_range_from_char_range(search_range.clone());
        let active_tab_index = self.active_tab_index();
        let Some(view_id) = self.active_tab().map(|tab| tab.active_view_id) else {
            return;
        };
        if let Some((buffer, view)) = self.tabs_mut()[active_tab_index].buffer_and_view_mut(view_id)
        {
            view.set_pending_cursor_range_anchored(buffer, cursor_range);
            if request_search_reveal_intent(view, &search_range) {
                view.request_cursor_reveal(CursorRevealMode::KeepHorizontalVisible);
            } else {
                view.request_cursor_reveal(CursorRevealMode::Center);
            }
        }
    }

    pub(super) fn refresh_search_visual_state(&mut self) {
        self.sync_search_result_group_activity();
        self.apply_search_highlights();
    }

    pub(super) fn set_active_search_index(&mut self, index: Option<usize>) {
        self.search_state.active_match_index = index;
        self.sync_active_search_cursor();
        self.refresh_search_visual_state();
    }

    pub(crate) fn select_next_active_buffer_match_from(&mut self, minimum_start: usize) {
        self.set_active_search_index(self.active_buffer_match_index_at_or_after(minimum_start));
    }

    pub(super) fn select_first_match_in_active_buffer(&mut self) {
        self.set_active_search_index(self.active_buffer_match_index_at_or_after(0));
    }

    fn sync_search_result_group_activity(&mut self) {
        let active_match_index = self.search_state.active_match_index;
        for group in Arc::make_mut(&mut self.search_state.result_groups) {
            group.active = false;
            for entry in &mut group.entries {
                entry.active = Some(entry.match_index) == active_match_index;
                group.active |= entry.active;
            }
        }
    }

    fn apply_search_highlights(&mut self) {
        if !self.search_is_active() || self.search_state.searching {
            self.clear_search_highlights();
            return;
        }

        if !matches!(
            self.search_state.status,
            SearchStatus::Ready | SearchStatus::NoMatches
        ) {
            self.clear_search_highlights();
            return;
        }

        let active_tab_index = self.active_tab_index();
        let highlights = self.search_highlights_for_tab(active_tab_index);
        let replacement_preview = self.search_replacement_preview();

        self.clear_search_highlights_outside_tab(active_tab_index);
        let Some(tab) = self.tabs_mut().get_mut(active_tab_index) else {
            return;
        };

        for (view_id, highlights) in highlights {
            if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
                view.set_search_highlights_anchored(buffer, highlights);
                view.set_search_replacement_preview(replacement_preview.clone());
            }
        }
    }

    fn search_replacement_preview(&self) -> Option<String> {
        (self.search_state.replace_open && self.search_state.status == SearchStatus::Ready)
            .then(|| self.search_state.replacement.clone())
    }

    fn search_highlights_for_tab(&self, tab_index: usize) -> Vec<(ViewId, SearchHighlightState)> {
        self.tabs()
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
                                &self.search_state.matches,
                                self.search_state.active_match_index,
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn clear_search_highlights(&mut self) {
        for tab_index in 0..self.tabs().len() {
            self.clear_search_highlights_for_tab(tab_index);
        }
    }

    fn clear_search_highlights_outside_tab(&mut self, active_tab_index: usize) {
        for tab_index in 0..self.tabs().len() {
            if tab_index != active_tab_index {
                self.clear_search_highlights_for_tab(tab_index);
            }
        }
    }

    fn clear_search_highlights_for_tab(&mut self, tab_index: usize) {
        let Some(tab) = self.tabs_mut().get_mut(tab_index) else {
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
