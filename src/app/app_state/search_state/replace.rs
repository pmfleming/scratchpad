use super::helpers::{
    build_replacement_targets, cursor_range_from_char_range, fallback_selection_for_target,
    next_selection_for_target,
};
use super::{ReplacementPlan, ReplacementTargetPlan, ScratchpadApp, SearchScope};
use crate::app::domain::{BufferId, CursorRevealMode, ViewId};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::ops::Range;

impl ScratchpadApp {
    pub(super) fn replace_ranges_in_active_buffer(
        &mut self,
        view_id: ViewId,
        buffer_id: BufferId,
        replacements: &[(Range<usize>, String)],
        previous_selection: CursorRange,
        next_selection: CursorRange,
        error_message: &str,
    ) -> Option<String> {
        let active_tab_index = self.active_tab_index();
        let buffer_label = self
            .tabs()
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
            let tab = &mut self.tabs_mut()[active_tab_index];
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
            self.set_error_status(error_message);
            return None;
        }

        self.finalize_tab_buffer_mutation(active_tab_index, buffer_id);
        Some(buffer_label)
    }

    pub(crate) fn replace_all_search_matches_in_scope(&mut self) -> bool {
        let Some(plan) = self.build_replace_all_plan() else {
            return false;
        };
        if plan.total_match_count == 0 {
            return false;
        }

        if plan.scope == SearchScope::ActiveBuffer && plan.targets.len() == 1 {
            return self.replace_all_in_active_buffer(&plan);
        }

        if plan.requires_confirmation() && !self.confirm_replace_all_plan(&plan) {
            return false;
        }

        let replaced = self.replace_all_in_multiple_buffers(&plan);
        if replaced {
            self.set_info_status(format!(
                "Replaced {} matches across {} buffers.",
                plan.total_match_count,
                plan.affected_buffer_count()
            ));
        }
        replaced
    }

    fn replace_all_in_active_buffer(&mut self, plan: &ReplacementPlan) -> bool {
        if !self.validate_replacement_plan(plan) {
            self.search_state.pending_replace_all_confirmation = None;
            self.set_error_status("Search replace-all was blocked because results are stale.");
            self.mark_search_dirty();
            self.refresh_search_state();
            return false;
        }

        let target = &plan.targets[0];
        let previous_selection = self
            .active_tab()
            .and_then(|tab| tab.view(target.view_id))
            .and_then(|view| view.cursor_range)
            .unwrap_or_else(|| cursor_range_from_char_range(target.replacements[0].0.clone()));
        let next_selection = cursor_range_from_char_range(
            target.replacements[0].0.start
                ..target.replacements[0].0.start + target.replacements[0].1.chars().count(),
        );
        let Some(buffer_label) = self.replace_ranges_in_active_buffer(
            target.view_id,
            target.buffer_id,
            &target.replacements,
            previous_selection,
            next_selection,
            "Search replace-all failed for the active buffer.",
        ) else {
            return false;
        };
        self.refresh_search_state();
        self.select_first_match_in_active_buffer();
        self.set_info_status(format!(
            "Replaced {} matches in {}.",
            plan.total_match_count, buffer_label
        ));
        true
    }

    fn replace_all_in_multiple_buffers(&mut self, plan: &ReplacementPlan) -> bool {
        if !self.validate_replacement_plan(plan) {
            self.search_state.pending_replace_all_confirmation = None;
            self.set_error_status("Search replace-all was blocked because results are stale.");
            return false;
        }

        for target in &plan.targets {
            if !self.apply_replacement_target(target) {
                self.set_error_status(
                    "Search replace-all failed before all targets could be updated.",
                );
                return false;
            }
        }
        self.mark_search_dirty();
        self.mark_session_dirty();
        self.search_state.pending_replace_all_confirmation = None;
        self.refresh_search_state();
        true
    }

    fn confirm_replace_all_plan(&mut self, plan: &ReplacementPlan) -> bool {
        let replacement = self.search_state.replacement.clone();
        let requested_generation = self.search_state.requested_generation;
        if self
            .search_state
            .pending_replace_all_confirmation
            .as_ref()
            .is_some_and(|confirmation| {
                confirmation.matches_plan(plan, &replacement, requested_generation)
            })
        {
            self.search_state.pending_replace_all_confirmation = None;
            return true;
        }

        let confirmation =
            super::ReplaceAllConfirmation::from_plan(plan, &replacement, requested_generation);
        let replacement_preview = if replacement.is_empty() {
            "empty text".to_owned()
        } else {
            format!("\"{}\"", replacement)
        };
        self.search_state.pending_replace_all_confirmation = Some(confirmation);
        self.set_info_status(format!(
            "Replace all will change {} matches across {} buffers with {replacement_preview}. Run Replace All again to confirm.",
            plan.total_match_count,
            plan.affected_buffer_count()
        ));
        false
    }

    fn validate_replacement_plan(&self, plan: &ReplacementPlan) -> bool {
        plan.targets
            .iter()
            .all(|target| self.validate_replacement_target(target))
    }

    fn validate_replacement_target(&self, target: &ReplacementTargetPlan) -> bool {
        let Some(tab) = self.tabs().get(target.tab_index) else {
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

    fn build_replace_all_plan(&self) -> Option<ReplacementPlan> {
        if self.search_state.matches.is_empty() {
            return None;
        }

        Some(ReplacementPlan {
            scope: self.search_state.scope,
            total_match_count: self.search_state.matches.len(),
            targets: build_replacement_targets(
                &self.search_state.matches,
                &self.search_state.replacement,
            ),
        })
    }

    fn apply_replacement_target(&mut self, target: &ReplacementTargetPlan) -> bool {
        let Some(tab) = self.tabs_mut().get_mut(target.tab_index) else {
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
        self.finalize_tab_buffer_mutation(target.tab_index, target.buffer_id);
        true
    }

    fn finalize_tab_buffer_mutation(&mut self, tab_index: usize, buffer_id: BufferId) {
        let tab = &mut self.tabs_mut()[tab_index];
        let has_control_chars = tab
            .buffer_by_id(buffer_id)
            .map(|buffer| buffer.artifact_summary.has_control_chars())
            .unwrap_or(false);
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer.is_dirty = true;
        }
        for view in &mut tab.views {
            if view.buffer_id == buffer_id && !has_control_chars {
                view.show_control_chars = false;
            }
        }
        let _ = tab;
        self.record_pending_text_history_event(tab_index, buffer_id);
        self.note_settings_toml_edit(tab_index);
    }
}
