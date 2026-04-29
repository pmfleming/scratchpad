use super::WorkspaceTab;
use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, ViewId};
use std::collections::HashSet;

impl WorkspaceTab {
    pub fn active_buffer(&self) -> &BufferState {
        &self.buffer
    }

    pub fn active_buffer_mut(&mut self) -> &mut BufferState {
        &mut self.buffer
    }

    pub fn buffers(&self) -> impl Iterator<Item = &BufferState> {
        std::iter::once(&self.buffer).chain(self.extra_buffers.iter())
    }

    pub fn buffers_mut(&mut self) -> impl Iterator<Item = &mut BufferState> {
        std::iter::once(&mut self.buffer).chain(self.extra_buffers.iter_mut())
    }

    pub fn buffer_by_id(&self, buffer_id: BufferId) -> Option<&BufferState> {
        self.buffer_matches_id(buffer_id)
            .then_some(&self.buffer)
            .or_else(|| {
                self.extra_buffers
                    .iter()
                    .find(|buffer| buffer.id == buffer_id)
            })
    }

    pub fn buffer_by_id_mut(&mut self, buffer_id: BufferId) -> Option<&mut BufferState> {
        if self.buffer_matches_id(buffer_id) {
            Some(&mut self.buffer)
        } else {
            self.extra_buffers
                .iter_mut()
                .find(|buffer| buffer.id == buffer_id)
        }
    }

    pub fn buffer_for_view(&self, view_id: ViewId) -> Option<&BufferState> {
        let view = self.view(view_id)?;
        self.buffer_by_id(view.buffer_id)
    }

    pub fn is_last_view_for_buffer(&self, view_id: ViewId) -> Option<bool> {
        let buffer_id = self.view(view_id)?.buffer_id;
        Some(
            self.views
                .iter()
                .filter(|view| view.buffer_id == buffer_id)
                .count()
                <= 1,
        )
    }

    pub fn buffer_and_view_mut(
        &mut self,
        view_id: ViewId,
    ) -> Option<(&mut BufferState, &mut EditorViewState)> {
        let Self {
            buffer,
            extra_buffers,
            views,
            ..
        } = self;
        let view_index = views.iter().position(|view| view.id == view_id)?;
        let buffer_id = views[view_index].buffer_id;
        let view = &mut views[view_index];

        if buffer.id == buffer_id {
            Some((buffer, view))
        } else {
            let buffer_index = Self::extra_buffer_index(extra_buffers, buffer_id)?;
            Some((&mut extra_buffers[buffer_index], view))
        }
    }

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
        let initial_view = EditorViewState::new(self.buffer.id, false);
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

    fn buffer_matches_id(&self, buffer_id: BufferId) -> bool {
        self.buffer.id == buffer_id
    }

    pub(super) fn extra_buffer_index(
        extra_buffers: &[BufferState],
        buffer_id: BufferId,
    ) -> Option<usize> {
        extra_buffers
            .iter()
            .position(|buffer| buffer.id == buffer_id)
    }
}
