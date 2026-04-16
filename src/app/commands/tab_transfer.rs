use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{SplitAxis, ViewId, WorkspaceTab};

struct TabCombineContext {
    adjusted_target_index: usize,
}

impl ScratchpadApp {
    pub(super) fn combine_tab_into_tab_command(
        &mut self,
        source_index: usize,
        target_index: usize,
    ) {
        if !Self::can_combine_tabs(self.tabs().len(), source_index, target_index) {
            return;
        }

        let snapshot = self.capture_transaction_snapshot();

        if source_index == self.active_tab_index() || target_index == self.active_tab_index() {
            self.reload_settings_before_workspace_change();
        }

        let (context, source_tab) = self.remove_source_tab_for_combine(source_index, target_index);
        let mut source_tab = Some(source_tab);
        if !self.try_combine_tabs(context.adjusted_target_index, &mut source_tab) {
            self.rollback_combined_tab(
                source_index,
                source_tab.expect("source tab should remain available on combine failure"),
            );
            return;
        }

        self.rebalance_combined_workspace_layout(context.adjusted_target_index, target_index);
        self.finish_combined_tab(source_index, target_index, context);
        self.record_transaction(
            "Combine tab",
            vec![
                format!("source {}", source_index + 1),
                format!("target {}", target_index + 1),
            ],
            None,
            snapshot,
        );
    }

    pub(super) fn promote_view_to_tab_command(&mut self, view_id: ViewId) {
        self.reload_settings_before_workspace_change();
        let snapshot = self.capture_transaction_snapshot();

        let source_index = self.active_tab_index();
        let promoted_tab = self
            .tabs_mut()
            .get_mut(source_index)
            .and_then(|tab| tab.promote_view_to_new_tab(view_id));
        let Some(promoted_tab) = promoted_tab else {
            return;
        };

        let promoted_description = promoted_tab.describe();
        self.append_tab(promoted_tab);
        self.record_transaction(
            "Promote view to tab",
            vec![promoted_description.clone()],
            None,
            snapshot,
        );
        let _ = self.persist_session_now();
    }

    pub(super) fn promote_tab_files_to_tabs_command(&mut self, index: usize) {
        if index >= self.tabs().len() {
            return;
        }

        let snapshot = self.capture_transaction_snapshot();

        if index == self.active_tab_index() {
            self.reload_settings_before_workspace_change();
        }

        let source_description = self.describe_tab_at(index);
        let source_tab = self.tab_manager_mut().tabs.remove(index);
        if !source_tab.can_promote_all_files() {
            self.tab_manager_mut().tabs.insert(index, source_tab);
            return;
        }

        let active_buffer_id = source_tab.active_buffer().id;
        let promoted_tabs = source_tab.into_tabs_per_file();
        if promoted_tabs.len() <= 1 {
            self.tab_manager_mut().tabs.insert(
                index,
                promoted_tabs
                    .into_iter()
                    .next()
                    .unwrap_or_else(WorkspaceTab::untitled),
            );
            return;
        }

        let active_tab_offset = promoted_tabs
            .iter()
            .position(|tab| tab.active_buffer().id == active_buffer_id)
            .unwrap_or(0);
        for (offset, tab) in promoted_tabs.into_iter().enumerate() {
            self.tab_manager_mut().tabs.insert(index + offset, tab);
        }
        self.tab_manager_mut().active_tab_index = index + active_tab_offset;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
        self.record_transaction(
            "Promote files to tabs",
            vec![source_description.clone()],
            None,
            snapshot,
        );
        let _ = self.persist_session_now();
    }

    fn can_combine_tabs(tab_count: usize, source_index: usize, target_index: usize) -> bool {
        source_index != target_index && source_index < tab_count && target_index < tab_count
    }

    pub(super) fn combine_tabs_into_tab_command(
        &mut self,
        mut source_indices: Vec<usize>,
        target_index: usize,
    ) {
        source_indices.sort_unstable();
        source_indices.dedup();
        source_indices.retain(|index| *index != target_index);
        if source_indices.is_empty()
            || source_indices
                .iter()
                .any(|index| *index >= self.tabs().len())
            || target_index >= self.tabs().len()
        {
            return;
        }

        let snapshot = self.capture_transaction_snapshot();
        if source_indices.contains(&self.active_tab_index())
            || target_index == self.active_tab_index()
        {
            self.reload_settings_before_workspace_change();
        }

        let source_descriptions = source_indices
            .iter()
            .map(|index| self.describe_tab_at(*index))
            .collect::<Vec<_>>();

        if self.tab_manager().tabs.get(target_index).is_none() {
            return;
        }

        let mut moved_tabs = Vec::with_capacity(source_indices.len());
        let mut adjusted_target_index = target_index;
        for source_index in source_indices.iter().rev().copied() {
            let removed = self.tab_manager_mut().tabs.remove(source_index);
            if source_index < adjusted_target_index {
                adjusted_target_index = adjusted_target_index.saturating_sub(1);
            }
            moved_tabs.push(removed);
        }
        moved_tabs.reverse();

        {
            let Some(target_tab) = self.tab_manager_mut().tabs.get_mut(adjusted_target_index)
            else {
                return;
            };

            for source_tab in moved_tabs {
                let _ = target_tab.combine_with_tab(source_tab, SplitAxis::Vertical, false, 0.5);
            }
        }

        self.tab_manager_mut().active_tab_index = adjusted_target_index;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
        self.rebalance_combined_workspace_layout(adjusted_target_index, target_index);
        self.record_transaction(
            "Combine tabs",
            vec![
                format!("{} tabs", source_descriptions.len()),
                format!("target {}", target_index + 1),
            ],
            None,
            snapshot,
        );
        let _ = self.persist_session_now();
    }

    fn remove_source_tab_for_combine(
        &mut self,
        source_index: usize,
        target_index: usize,
    ) -> (TabCombineContext, WorkspaceTab) {
        let adjusted_target_index = Self::adjusted_target_index(source_index, target_index);
        let source_tab = self.tab_manager_mut().tabs.remove(source_index);
        (
            TabCombineContext {
                adjusted_target_index,
            },
            source_tab,
        )
    }

    fn adjusted_target_index(source_index: usize, target_index: usize) -> usize {
        if source_index < target_index {
            target_index.saturating_sub(1)
        } else {
            target_index
        }
    }

    fn try_combine_tabs(
        &mut self,
        adjusted_target_index: usize,
        source_tab: &mut Option<WorkspaceTab>,
    ) -> bool {
        self.tab_manager_mut()
            .tabs
            .get_mut(adjusted_target_index)
            .is_some_and(|target_tab| {
                target_tab
                    .combine_with_tab(
                        source_tab
                            .take()
                            .expect("source tab removed before combine"),
                        SplitAxis::Vertical,
                        false,
                        0.5,
                    )
                    .is_some()
            })
    }

    fn rollback_combined_tab(&mut self, source_index: usize, source_tab: WorkspaceTab) {
        let reinsertion_index = source_index.min(self.tabs().len());
        self.tab_manager_mut()
            .tabs
            .insert(reinsertion_index, source_tab);
    }

    fn rebalance_combined_workspace_layout(
        &mut self,
        adjusted_target_index: usize,
        _target_index: usize,
    ) {
        let reflow_axis = self.workspace_reflow_axis;
        let Some(target_tab) = self.tab_manager_mut().tabs.get_mut(adjusted_target_index) else {
            return;
        };
        let _ = target_tab.rebalance_views_equally_for_axis(reflow_axis);
    }

    fn finish_combined_tab(
        &mut self,
        _source_index: usize,
        _target_index: usize,
        context: TabCombineContext,
    ) {
        self.tab_manager_mut().active_tab_index = context.adjusted_target_index;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
    }
}
