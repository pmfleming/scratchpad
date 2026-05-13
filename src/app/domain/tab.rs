use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, ViewId, tab_support};
use std::collections::HashSet;

mod layout;
mod promotion;
mod repair;
pub(crate) mod summary;

#[derive(Clone)]
pub struct WorkspaceTab {
    pub buffer: BufferState,
    pub extra_buffers: Vec<BufferState>,
    pub views: Vec<EditorViewState>,
    pub root_pane: PaneNode,
    pub active_view_id: ViewId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabAttentionState {
    AutoEdit,
    Dirty,
    DiskProblem,
}

impl WorkspaceTab {
    pub fn new(buffer: BufferState) -> Self {
        let initial_view = EditorViewState::new(buffer.id);
        let active_view_id = initial_view.id;
        Self {
            buffer,
            extra_buffers: Vec::new(),
            views: vec![initial_view],
            root_pane: PaneNode::leaf(active_view_id),
            active_view_id,
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
            buffer,
            extra_buffers,
            views,
            root_pane,
            active_view_id,
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
        if !self.root_pane.contains_view(view_id) {
            return false;
        }

        self.active_view_id = view_id;
        self.sync_active_buffer_to_active_view()
    }

    pub fn describe(&self) -> String {
        let path = self
            .buffer
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<unsaved>".to_owned());
        format!(
            "{} [path={}, dirty={}, views={}, active_view={}]",
            self.buffer.name,
            path,
            self.buffer.is_dirty,
            self.views.len(),
            self.active_view_id
        )
    }

    fn push_buffer_if_missing(&mut self, buffer: BufferState) {
        if self.buffer_by_id(buffer.id).is_some() {
            return;
        }

        self.extra_buffers.push(buffer);
    }

    pub(super) fn distinct_buffer_count(&self) -> usize {
        self.views
            .iter()
            .map(|view| view.buffer_id)
            .collect::<HashSet<_>>()
            .len()
    }

    pub(super) fn distinct_buffer_names_in_view_order(&self) -> Vec<String> {
        let ordered_view_ids = self.ordered_view_ids_in_layout_order();
        let mut names =
            tab_support::ordered_buffer_ids_with_fallback(&self.views, &ordered_view_ids)
                .into_iter()
                .filter_map(|buffer_id| {
                    self.buffer_by_id(buffer_id)
                        .map(|buffer| buffer.name.clone())
                })
                .collect::<Vec<_>>();

        if names.is_empty() {
            names.push(self.buffer.name.clone());
        }

        names
    }

    fn sync_active_buffer_to_active_view(&mut self) -> bool {
        let Some(active_buffer_id) = self.active_view().map(|view| view.buffer_id) else {
            return false;
        };

        if self.buffer.id == active_buffer_id {
            return true;
        }

        let Some(buffer_index) = Self::extra_buffer_index(&self.extra_buffers, active_buffer_id)
        else {
            return false;
        };

        std::mem::swap(&mut self.buffer, &mut self.extra_buffers[buffer_index]);
        true
    }

    fn prune_unused_buffers(&mut self) {
        let referenced_buffer_ids = self
            .views
            .iter()
            .map(|view| view.buffer_id)
            .collect::<HashSet<_>>();

        if !referenced_buffer_ids.contains(&self.buffer.id) {
            self.sync_active_buffer_to_active_view();
        }

        self.extra_buffers
            .retain(|buffer| referenced_buffer_ids.contains(&buffer.id));
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
