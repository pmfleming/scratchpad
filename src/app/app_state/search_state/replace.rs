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

impl ScratchpadApp {
    pub fn replace_current_search_match(&mut self) -> bool {
        if !self.search_replace_availability().allows_actions() {
            return false;
        }
        let Some(index) = self.search_state.active_match_index else {
            return false;
        };
        let Some(search_match) = self.search_state.matches.get(index).cloned() else {
            return false;
        };
        if !self.validate_search_match_for_replace(&search_match) {
            self.search_state.clear_replace_all_confirmation();
            self.report_search_results_stale_for_replace();
            self.mark_search_dirty();
            self.refresh_search_state();
            return false;
        }
        if !self.activate_search_match(index) {
            return false;
        }

        let replacement = match self.replacement_for_match(&search_match) {
            Ok(replacement) => replacement,
            Err(error) => {
                self.set_error_status_in_domain(StatusDomain::Search, error.message());
                return false;
            }
        };
        let replacement_char_count = replacement.chars().count();
        let previous_selection = self
            .active_tab()
            .and_then(|tab| tab.view(search_match.view_id))
            .and_then(|view| view.cursor_range)
            .unwrap_or_else(|| cursor_range_from_char_range(search_match.range.clone()));
        let replacement_range =
            search_match.range.start..search_match.range.start + replacement_char_count;
        let next_selection = cursor_range_from_char_range(replacement_range.clone());
        let replacements = vec![(search_match.range.clone(), replacement)];

        if self
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

        if let Err(error) = self.rebuild_active_buffer_search_matches() {
            self.set_error_status_in_domain(StatusDomain::Search, error.message());
            self.mark_search_dirty();
            self.refresh_search_state();
            return false;
        }
        self.select_next_active_buffer_match_from(replacement_range.end);
        self.mark_search_dirty();
        self.refresh_search_state();
        true
    }

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
            self.set_error_status_in_domain(StatusDomain::Search, error_message);
            return None;
        }

        self.finalize_tab_buffer_mutation(active_tab_index, buffer_id);
        Some(buffer_label)
    }

    pub(crate) fn replace_all_search_matches_in_scope(&mut self) -> bool {
        let plan = match self.build_replace_all_plan() {
            Ok(Some(plan)) => plan,
            Ok(None) => return false,
            Err(error) => {
                self.set_error_status_in_domain(StatusDomain::Search, error.message());
                return false;
            }
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
            self.set_info_status_in_domain(
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

    fn replace_all_in_active_buffer(&mut self, plan: &ReplacementPlan) -> bool {
        if !self.validate_replacement_plan(plan) {
            self.search_state.pending_replace_all_confirmation = None;
            self.report_search_results_stale_for_replace();
            self.mark_search_dirty();
            self.refresh_search_state();
            return false;
        }

        let target = &plan.targets[0];
        let previous_selection = self
            .active_tab()
            .and_then(|tab| tab.view(target.view_id))
            .and_then(|view| view.cursor_range);
        let (first_range, first_replacement) = first_document_order_replacement(target);
        let previous_selection =
            previous_selection.unwrap_or_else(|| cursor_range_from_char_range(first_range.clone()));
        let next_selection = cursor_range_from_char_range(
            first_range.start..first_range.start + first_replacement.chars().count(),
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
        if let Err(error) = self.rebuild_active_buffer_search_matches() {
            self.set_error_status_in_domain(StatusDomain::Search, error.message());
            self.mark_search_dirty();
            self.refresh_search_state();
            return false;
        }
        self.select_first_match_in_active_buffer();
        self.mark_search_dirty();
        self.refresh_search_state();
        self.set_info_status_in_domain(
            StatusDomain::Search,
            format!(
                "Replaced {} matches in {}.",
                plan.total_match_count, buffer_label
            ),
        );
        true
    }

    fn replace_all_in_multiple_buffers(&mut self, plan: &ReplacementPlan) -> bool {
        if !self.validate_replacement_plan(plan) {
            self.search_state.pending_replace_all_confirmation = None;
            self.report_search_results_stale_for_replace();
            return false;
        }

        for target in &plan.targets {
            if !self.validate_replacement_target(target) {
                self.search_state.pending_replace_all_confirmation = None;
                self.report_search_results_stale_for_replace();
                return false;
            }
            if !self.apply_replacement_target(target) {
                self.set_error_status_in_domain(
                    StatusDomain::Search,
                    "Search replace-all stopped after some targets may already have been updated.",
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
        self.set_info_status_in_domain(StatusDomain::Search, format!(
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

    fn validate_search_match_for_replace(&self, search_match: &super::SearchMatch) -> bool {
        let Some(tab) = self.tabs().get(search_match.tab_index) else {
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

    fn build_replace_all_plan(&self) -> Result<Option<ReplacementPlan>, SearchError> {
        if self.search_state.matches.is_empty() {
            return Ok(None);
        }
        let program =
            SearchProgram::compile(&self.search_state.query, self.search_state.search_options())?;
        let replacement = self.search_state.replacement.clone();

        Ok(Some(ReplacementPlan {
            scope: self.search_state.scope,
            total_match_count: self.search_state.matches.len(),
            targets: build_replacement_targets(&self.search_state.matches, |search_match| {
                program.expand_replacement(&search_match.matched_text, &replacement)
            })?,
        }))
    }

    fn replacement_for_match(
        &self,
        search_match: &super::SearchMatch,
    ) -> Result<String, SearchError> {
        let program =
            SearchProgram::compile(&self.search_state.query, self.search_state.search_options())?;
        program.expand_replacement(&search_match.matched_text, &self.search_state.replacement)
    }

    fn rebuild_active_buffer_search_matches(&mut self) -> Result<(), SearchError> {
        let active_tab_index = self.active_tab_index();
        let Some(tab) = self.tabs().get(active_tab_index) else {
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
        let program =
            SearchProgram::compile(&self.search_state.query, self.search_state.search_options())?;
        let ranges = search_program(&text, &program).matches;

        let insertion_index = self
            .search_state
            .matches
            .iter()
            .position(|search_match| {
                search_match.tab_index == active_tab_index && search_match.buffer_id == buffer_id
            })
            .unwrap_or(self.search_state.matches.len());
        self.search_state.matches.retain(|search_match| {
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
        self.search_state
            .matches
            .splice(insertion_index..insertion_index, new_matches);
        self.search_state.total_match_count = self.search_state.matches.len();
        self.search_state.displayed_match_count = self.search_state.matches.len();
        Ok(())
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
        if let Some(buffer) = tab.buffer_by_id_mut(buffer_id) {
            buffer.mark_dirty_after_local_edit();
        }
        let _ = tab;
        self.record_pending_text_history_event(tab_index, buffer_id);
        self.note_settings_toml_edit(tab_index);
    }
}
