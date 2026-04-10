use super::WorkspaceTab;
use crate::app::domain::{EditorViewState, PaneNode, ViewId};
use std::collections::HashSet;

impl WorkspaceTab {
    pub(super) fn repair_restored_state(&mut self) {
        if !self.retain_views_for_known_buffers() {
            self.reset_to_single_view();
            return;
        }

        if !self.repair_root_pane() {
            self.reset_to_single_view();
            return;
        }

        self.ensure_active_view_is_present();
        self.sync_active_buffer_to_active_view();
        self.prune_unused_buffers();
        self.set_line_numbers_visible(self.line_numbers_visible());
    }

    fn reset_to_single_view(&mut self) {
        let initial_view = EditorViewState::new(
            self.buffer.id,
            self.buffer.artifact_summary.has_control_chars(),
        );
        self.active_view_id = initial_view.id;
        self.root_pane = PaneNode::leaf(initial_view.id);
        self.extra_buffers.clear();
        self.views = vec![initial_view];
    }

    fn retain_views_for_known_buffers(&mut self) -> bool {
        let valid_buffer_ids = self
            .buffers()
            .map(|buffer| buffer.id)
            .collect::<HashSet<_>>();
        self.views
            .retain(|view| valid_buffer_ids.contains(&view.buffer_id));
        !self.views.is_empty()
    }

    fn repair_root_pane(&mut self) -> bool {
        let valid_view_ids = self
            .views
            .iter()
            .map(|view| view.id)
            .collect::<HashSet<_>>();
        if !self.root_pane.retain_views(&valid_view_ids) {
            return false;
        }

        let pane_view_ids = self.pane_view_ids();
        self.views.retain(|view| pane_view_ids.contains(&view.id));
        !self.views.is_empty()
    }

    fn pane_view_ids(&self) -> HashSet<ViewId> {
        let mut pane_view_ids = HashSet::new();
        self.root_pane.collect_view_ids(&mut pane_view_ids);
        pane_view_ids
    }

    fn ensure_active_view_is_present(&mut self) {
        if !self.root_pane.contains_view(self.active_view_id) {
            self.active_view_id = self.root_pane.first_view_id();
        }
    }
}
