use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, ViewId, tab_support};
use std::collections::HashSet;

mod buffers;
mod layout;
mod layout_state;
mod promotion;
mod repair;
pub(crate) mod summary;

pub use buffers::WorkspaceTabBuffers;
pub use layout_state::WorkspaceTabLayout;

#[derive(Clone)]
pub struct WorkspaceTab {
    pub buffers: WorkspaceTabBuffers,
    pub layout: WorkspaceTabLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabAttentionState {
    AutoEdit,
    Dirty,
    DiskProblem,
}

impl WorkspaceTab {
    pub fn new(buffer: BufferState) -> Self {
        let layout = WorkspaceTabLayout::new(buffer.id);
        Self {
            buffers: WorkspaceTabBuffers::new(buffer),
            layout,
        }
    }

    pub fn restored(
        buffer: BufferState,
        views: Vec<EditorViewState>,
        root_pane: PaneNode,
        active_view_id: ViewId,
    ) -> Self {
        Self::restored_with_buffers(buffer, Vec::new(), views, root_pane, active_view_id)
    }

    pub fn restored_with_buffers(
        buffer: BufferState,
        extra_buffers: Vec<BufferState>,
        views: Vec<EditorViewState>,
        root_pane: PaneNode,
        active_view_id: ViewId,
    ) -> Self {
        let mut tab = Self {
            buffers: WorkspaceTabBuffers::restored(buffer, extra_buffers),
            layout: WorkspaceTabLayout::restored(views, root_pane, active_view_id),
        };
        tab.repair_restored_state();
        tab
    }

    pub fn untitled() -> Self {
        Self::new(BufferState::new("Untitled".to_owned(), String::new(), None))
    }

    pub fn display_name(&self) -> String {
        summary::display_name(self)
    }

    pub fn file_group_count(&self) -> usize {
        self.distinct_buffer_count()
    }

    pub fn activate_view(&mut self, view_id: ViewId) -> bool {
        if !self.layout.contains_view(view_id) {
            return false;
        }

        self.layout.set_active_view_id(view_id);
        self.sync_active_buffer_to_active_view()
    }

    pub fn describe(&self) -> String {
        let path = self
            .buffers
            .active()
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<unsaved>".to_owned());
        let active_buffer = self.buffers.active();
        format!(
            "{} [path={}, dirty={}, views={}, active_view={}]",
            active_buffer.name,
            path,
            active_buffer.is_dirty,
            self.layout.view_count(),
            self.layout.active_view_id()
        )
    }

    fn push_buffer_if_missing(&mut self, buffer: BufferState) {
        self.buffers.push_if_missing(buffer);
    }

    pub(super) fn distinct_buffer_count(&self) -> usize {
        self.layout
            .views()
            .iter()
            .map(|view| view.buffer_id)
            .collect::<HashSet<_>>()
            .len()
    }

    pub(super) fn distinct_buffer_names_in_view_order(&self) -> Vec<String> {
        let ordered_view_ids = self.ordered_view_ids_in_layout_order();
        let mut names =
            tab_support::ordered_buffer_ids_with_fallback(self.layout.views(), &ordered_view_ids)
                .into_iter()
                .filter_map(|buffer_id| {
                    self.buffer_by_id(buffer_id)
                        .map(|buffer| buffer.name.clone())
                })
                .collect::<Vec<_>>();

        if names.is_empty() {
            names.push(self.buffers.active().name.clone());
        }

        names
    }

    fn sync_active_buffer_to_active_view(&mut self) -> bool {
        let Some(active_buffer_id) = self.active_view().map(|view| view.buffer_id) else {
            return false;
        };

        self.buffers.sync_active_buffer_to(active_buffer_id)
    }

    fn prune_unused_buffers(&mut self) {
        let referenced_buffer_ids = self
            .layout
            .views()
            .iter()
            .map(|view| view.buffer_id)
            .collect::<HashSet<_>>();

        if !referenced_buffer_ids.contains(&self.buffers.active().id) {
            self.sync_active_buffer_to_active_view();
        }

        self.buffers.prune_to_buffer_ids(&referenced_buffer_ids);
    }

    fn active_buffer_id_for_view(
        views: &[EditorViewState],
        active_view_id: ViewId,
    ) -> Option<BufferId> {
        views
            .iter()
            .find(|view| view.id == active_view_id)
            .map(|view| view.buffer_id)
    }
}
