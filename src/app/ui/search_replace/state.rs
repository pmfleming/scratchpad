use crate::app::app_state::{
    ScratchpadApp, SearchFocusTarget, SearchFreshness, SearchProgress, SearchReplaceAvailability,
    SearchResultGroup, SearchScope, SearchScopeOrigin, SearchStatus,
};
use crate::app::services::search::SearchMode;
use crate::app::utils::pluralize;
use eframe::egui;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct SearchStripActions {
    pub(super) close_requested: bool,
    pub(super) next_requested: bool,
    pub(super) previous_requested: bool,
    pub(super) undo_requested: bool,
    pub(super) redo_requested: bool,
    pub(super) replace_current_requested: bool,
    pub(super) replace_all_requested: bool,
    pub(super) focused_file_match_index: Option<usize>,
    pub(super) selected_match_index: Option<usize>,
}

pub(super) struct SearchStripState {
    pub(super) query: String,
    pub(super) replacement: String,
    pub(super) replace_open: bool,
    pub(super) scope: SearchScope,
    pub(super) scope_origin: SearchScopeOrigin,
    pub(super) mode: SearchMode,
    pub(super) match_case: bool,
    pub(super) whole_word: bool,
    pub(super) match_count: usize,
    pub(super) active_match_index: Option<usize>,
    pub(super) progress: SearchProgressSnapshot,
    pub(super) result_groups: Arc<[SearchResultGroup]>,
    pub(super) replace_availability: SearchReplaceAvailability,
    pub(super) can_undo_text_operation: bool,
    pub(super) can_redo_text_operation: bool,
    requested_focus: Option<SearchFocusTarget>,
    retained_focus: Option<SearchFocusTarget>,
}

pub(super) struct SearchProgressSnapshot {
    pub(super) searching: bool,
    pub(super) scanned_targets: usize,
    pub(super) target_count: usize,
    pub(super) displayed_match_count: usize,
    pub(super) total_match_count: usize,
    pub(super) status: SearchStatus,
    pub(super) freshness: SearchFreshness,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VirtualRows {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) leading_space: f32,
    pub(super) trailing_space: f32,
}

impl SearchStripState {
    pub(super) fn from_app(app: &mut ScratchpadApp) -> Self {
        let match_count = app.search_match_count();
        let progress = app.search_progress();
        let requested_focus = app.take_search_focus_target();

        Self {
            query: app.search_query().to_owned(),
            replacement: app.search_replacement().to_owned(),
            replace_open: app.search_replace_open(),
            scope: app.search_scope(),
            scope_origin: app.search_scope_origin(),
            mode: app.search_mode(),
            match_case: app.search_match_case(),
            whole_word: app.search_whole_word(),
            match_count,
            active_match_index: app.search_active_match_index(),
            progress: SearchProgressSnapshot::from_progress(progress),
            result_groups: app.search_result_groups_snapshot(),
            replace_availability: app.search_replace_availability(),
            can_undo_text_operation: app.active_buffer_can_undo_text_operation(),
            can_redo_text_operation: app.active_buffer_can_redo_text_operation(),
            requested_focus,
            retained_focus: requested_focus,
        }
    }

    pub(super) fn sync_focus(
        &mut self,
        response: &egui::Response,
        focus_target: SearchFocusTarget,
    ) {
        if self.requested_focus == Some(focus_target) {
            response.request_focus();
            self.retained_focus = Some(focus_target);
        } else if response.has_focus() {
            self.retained_focus = Some(focus_target);
        }
    }

    pub(super) fn target_focus(&self) -> SearchFocusTarget {
        self.retained_focus.unwrap_or(SearchFocusTarget::FindInput)
    }

    pub(super) fn results_summary(&self) -> String {
        if self.progress.searching || self.progress.freshness == SearchFreshness::Stale {
            return searching_summary(&self.progress);
        }

        if self.query.is_empty() {
            return String::new();
        }

        if let Some(summary) = status_summary(&self.progress.status) {
            return summary;
        }

        if self.progress.displayed_match_count < self.progress.total_match_count {
            return format!(
                "{} previews of {} matches",
                self.progress.displayed_match_count, self.progress.total_match_count
            );
        }

        format!(
            "{} matches in {}",
            self.match_count,
            pluralize(self.result_groups.len(), "file")
        )
    }
}

impl SearchProgressSnapshot {
    fn from_progress(progress: SearchProgress) -> Self {
        Self {
            searching: matches!(progress.status, SearchStatus::Searching { .. }),
            scanned_targets: progress.scanned_targets,
            target_count: progress.target_count,
            displayed_match_count: progress.displayed_match_count,
            total_match_count: progress.total_match_count,
            status: progress.status,
            freshness: progress.freshness,
        }
    }
}

fn searching_summary(progress: &SearchProgressSnapshot) -> String {
    if progress.target_count > 0 {
        format!(
            "Searching {} of {}...",
            progress.scanned_targets.min(progress.target_count),
            pluralize(progress.target_count, "file")
        )
    } else {
        "Searching\u{2026}".to_owned()
    }
}

fn status_summary(status: &SearchStatus) -> Option<String> {
    match status {
        SearchStatus::InvalidQuery(_) => Some("Invalid query".to_owned()),
        SearchStatus::Error(message) => Some(message.clone()),
        SearchStatus::Idle
        | SearchStatus::Searching { .. }
        | SearchStatus::Ready
        | SearchStatus::NoMatches => None,
    }
}

pub(super) fn virtual_rows_for_clip(
    row_count: usize,
    start_y: f32,
    clip: egui::Rect,
    row_height: f32,
    row_spacing: f32,
    overscan: usize,
) -> VirtualRows {
    if row_count == 0 {
        return VirtualRows {
            first: 0,
            last: 0,
            leading_space: 0.0,
            trailing_space: 0.0,
        };
    }

    let row_step = virtual_row_step(row_height, row_spacing);
    let first_visible = ((clip.top() - start_y) / row_step).floor().max(0.0) as usize;
    let last_visible = ((clip.bottom() - start_y) / row_step).ceil().max(0.0) as usize + 1;
    let first = first_visible.saturating_sub(overscan).min(row_count);
    let last = last_visible.saturating_add(overscan).min(row_count);
    let rendered_end = if first >= last {
        row_top(first, row_height, row_spacing)
    } else {
        row_top(last - 1, row_height, row_spacing) + row_height
    };

    let total_height = virtual_total_height(row_count, row_height, row_spacing);
    VirtualRows {
        first,
        last,
        leading_space: row_top(first, row_height, row_spacing).min(total_height),
        trailing_space: (total_height - rendered_end).max(0.0),
    }
}

pub(super) fn virtual_total_height(row_count: usize, row_height: f32, row_spacing: f32) -> f32 {
    if row_count == 0 {
        0.0
    } else {
        row_count as f32 * row_height + row_count.saturating_sub(1) as f32 * row_spacing
    }
}

pub(super) fn row_top(row_index: usize, row_height: f32, row_spacing: f32) -> f32 {
    row_index as f32 * virtual_row_step(row_height, row_spacing)
}

fn virtual_row_step(row_height: f32, row_spacing: f32) -> f32 {
    row_height + row_spacing
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW_HEIGHT: f32 = 34.0;
    const ROW_SPACING: f32 = 2.0;

    #[test]
    fn virtual_total_height_counts_rows_without_trailing_gap() {
        assert_eq!(virtual_total_height(0, ROW_HEIGHT, ROW_SPACING), 0.0);
        assert_eq!(virtual_total_height(1, ROW_HEIGHT, ROW_SPACING), ROW_HEIGHT);
        assert_eq!(
            virtual_total_height(3, ROW_HEIGHT, ROW_SPACING),
            ROW_HEIGHT * 3.0 + ROW_SPACING * 2.0
        );
    }

    #[test]
    fn virtual_rows_for_clip_adds_bounded_overscan() {
        let clip = egui::Rect::from_min_max(egui::pos2(0.0, 92.0), egui::pos2(100.0, 184.0));
        let rows = virtual_rows_for_clip(20, 0.0, clip, ROW_HEIGHT, ROW_SPACING, 1);

        assert_eq!(rows.first, 1);
        assert_eq!(rows.last, 8);
        assert_eq!(rows.leading_space, row_top(1, ROW_HEIGHT, ROW_SPACING));
        assert_eq!(
            rows.trailing_space,
            virtual_total_height(20, ROW_HEIGHT, ROW_SPACING)
                - (row_top(7, ROW_HEIGHT, ROW_SPACING) + ROW_HEIGHT)
        );
    }

    #[test]
    fn virtual_rows_for_clip_clamps_outside_content() {
        let clip = egui::Rect::from_min_max(egui::pos2(0.0, 10_000.0), egui::pos2(100.0, 10_100.0));
        let rows = virtual_rows_for_clip(5, 0.0, clip, ROW_HEIGHT, ROW_SPACING, 2);

        assert_eq!(rows.first, 5);
        assert_eq!(rows.last, 5);
        assert_eq!(
            rows.leading_space,
            virtual_total_height(5, ROW_HEIGHT, ROW_SPACING)
        );
        assert_eq!(rows.trailing_space, 0.0);
    }
}
