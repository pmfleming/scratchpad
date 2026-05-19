use super::WorkspaceTab;
use crate::app::domain::{BufferId, BufferState, EditorViewState, ViewId};
use std::collections::HashSet;

impl WorkspaceTab {
    #[must_use]
    pub fn active_buffer(&self) -> &BufferState {
        self.buffers.active()
    }

    pub fn active_buffer_mut(&mut self) -> &mut BufferState {
        self.buffers.active_mut()
    }

    pub fn buffers(&self) -> impl Iterator<Item = &BufferState> {
        self.buffers.all()
    }

    pub fn buffers_mut(&mut self) -> impl Iterator<Item = &mut BufferState> {
        self.buffers.all_mut()
    }

    #[must_use]
    pub fn buffer_by_id(&self, buffer_id: BufferId) -> Option<&BufferState> {
        self.buffers.by_id(buffer_id)
    }

    pub fn buffer_by_id_mut(&mut self, buffer_id: BufferId) -> Option<&mut BufferState> {
        self.buffers.by_id_mut(buffer_id)
    }

    #[must_use]
    pub fn buffer_for_view(&self, view_id: ViewId) -> Option<&BufferState> {
        let view = self.layout.view(view_id)?;
        self.buffer_by_id(view.buffer_id)
    }

    #[must_use]
    pub fn is_last_view_for_buffer(&self, view_id: ViewId) -> Option<bool> {
        let buffer_id = self.layout.view(view_id)?.buffer_id;
        Some(self.layout.view_count_for_buffer(buffer_id) <= 1)
    }

    pub fn buffer_and_view_mut(
        &mut self,
        view_id: ViewId,
    ) -> Option<(&mut BufferState, &mut EditorViewState)> {
        let Self { layout, buffers } = self;
        let view_index = layout.views.iter().position(|view| view.id == view_id)?;
        let buffer_id = layout.views[view_index].buffer_id;
        let view = &mut layout.views[view_index];

        if buffers.buffer.id == buffer_id {
            Some((&mut buffers.buffer, view))
        } else {
            let buffer_index =
                super::WorkspaceTabBuffers::extra_buffer_index(&buffers.extra_buffers, buffer_id)?;
            Some((&mut buffers.extra_buffers[buffer_index], view))
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
        self.layout
            .set_line_numbers_visible(self.layout.line_numbers_visible());
    }

    fn reset_to_single_view(&mut self) {
        self.buffers.extra_buffers.clear();
        self.layout.reset_to_single_view(self.buffers.active().id);
    }

    fn retain_views_for_known_buffers(&mut self) -> bool {
        let valid_buffer_ids = self
            .buffers()
            .map(|buffer| buffer.id)
            .collect::<HashSet<_>>();
        self.layout.retain_views_for_buffer_ids(&valid_buffer_ids)
    }

    fn repair_root_pane(&mut self) -> bool {
        self.layout.repair_root_pane_for_current_views()
    }

    fn ensure_active_view_is_present(&mut self) {
        self.layout.ensure_active_view_is_present();
    }
}
