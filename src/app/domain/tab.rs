use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, ViewId};
use std::collections::HashSet;

mod layout;
mod promotion;
mod repair;

#[derive(Clone)]
pub struct WorkspaceTab {
    pub buffer: BufferState,
    pub extra_buffers: Vec<BufferState>,
    pub views: Vec<EditorViewState>,
    pub root_pane: PaneNode,
    pub active_view_id: ViewId,
}

impl WorkspaceTab {
    pub fn new(buffer: BufferState) -> Self {
        let initial_view =
            EditorViewState::new(buffer.id, buffer.artifact_summary.has_control_chars());
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
        self.buffer.display_name()
    }

    pub fn full_display_name(&self, has_duplicate: bool) -> String {
        let name = self.display_name();
        if has_duplicate && let Some(context) = self.overflow_context_label() {
            return format!("{} ({})", name, context);
        }
        name
    }

    pub fn overflow_context_label(&self) -> Option<String> {
        self.buffer.overflow_context_label()
    }

    pub fn active_view(&self) -> Option<&EditorViewState> {
        self.view(self.active_view_id)
    }

    pub fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.view_mut(self.active_view_id)
    }

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

    pub fn can_promote_view(&self, view_id: ViewId) -> bool {
        self.view(view_id).is_some() && self.distinct_buffer_count() > 1
    }

    pub fn can_promote_all_files(&self) -> bool {
        self.distinct_buffer_count() >= 3
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

    pub fn line_numbers_visible(&self) -> bool {
        self.active_view()
            .map(|view| view.show_line_numbers)
            .unwrap_or(false)
    }

    pub fn set_line_numbers_visible(&mut self, visible: bool) {
        for view in &mut self.views {
            view.show_line_numbers = visible;
        }
    }

    pub fn view(&self, view_id: ViewId) -> Option<&EditorViewState> {
        self.views.iter().find(|view| view.id == view_id)
    }

    pub fn view_mut(&mut self, view_id: ViewId) -> Option<&mut EditorViewState> {
        self.views.iter_mut().find(|view| view.id == view_id)
    }

    pub fn close_view(&mut self, view_id: ViewId) -> bool {
        if self.root_pane.leaf_count() <= 1 {
            return false;
        }

        if !self.root_pane.contains_view(view_id) {
            return false;
        }

        if !self.root_pane.remove_view(view_id) {
            return false;
        }

        self.views.retain(|view| view.id != view_id);
        if self.active_view_id == view_id {
            self.active_view_id = self.root_pane.first_view_id();
        }
        self.sync_active_buffer_to_active_view();
        self.prune_unused_buffers();
        true
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

    fn distinct_buffer_count(&self) -> usize {
        self.views
            .iter()
            .map(|view| view.buffer_id)
            .collect::<HashSet<_>>()
            .len()
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

    fn ordered_view_ids(root_pane: &PaneNode) -> Vec<ViewId> {
        let mut ordered = Vec::new();
        root_pane.collect_view_ids_in_order(&mut ordered);
        ordered
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

    fn buffer_matches_id(&self, buffer_id: BufferId) -> bool {
        self.buffer.id == buffer_id
    }

    fn extra_buffer_index(extra_buffers: &[BufferState], buffer_id: BufferId) -> Option<usize> {
        extra_buffers
            .iter()
            .position(|buffer| buffer.id == buffer_id)
    }
}
