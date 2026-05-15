use crate::app::domain::{BufferId, EditorViewState, PaneNode, ViewId};
use std::collections::HashSet;

#[derive(Clone)]
pub struct WorkspaceTabLayout {
    pub views: Vec<EditorViewState>,
    pub root_pane: PaneNode,
    pub active_view_id: ViewId,
}

impl WorkspaceTabLayout {
    pub fn new(buffer_id: BufferId) -> Self {
        let initial_view = EditorViewState::new(buffer_id);
        let active_view_id = initial_view.id;
        Self {
            views: vec![initial_view],
            root_pane: PaneNode::leaf(active_view_id),
            active_view_id,
        }
    }

    pub fn restored(
        views: Vec<EditorViewState>,
        root_pane: PaneNode,
        active_view_id: ViewId,
    ) -> Self {
        Self {
            views,
            root_pane,
            active_view_id,
        }
    }

    pub fn active_view_id(&self) -> ViewId {
        self.active_view_id
    }

    pub fn set_active_view_id(&mut self, view_id: ViewId) {
        self.active_view_id = view_id;
    }

    pub fn views(&self) -> &[EditorViewState] {
        &self.views
    }

    pub fn views_mut(&mut self) -> &mut [EditorViewState] {
        &mut self.views
    }

    pub fn view_count(&self) -> usize {
        self.views.len()
    }

    pub fn root_pane(&self) -> &PaneNode {
        &self.root_pane
    }

    pub fn root_pane_mut(&mut self) -> &mut PaneNode {
        &mut self.root_pane
    }

    pub fn contains_view(&self, view_id: ViewId) -> bool {
        self.root_pane.contains_view(view_id)
    }

    pub fn leaf_count(&self) -> usize {
        self.root_pane.leaf_count()
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

    pub fn view_count_for_buffer(&self, buffer_id: BufferId) -> usize {
        self.views
            .iter()
            .filter(|view| view.buffer_id == buffer_id)
            .count()
    }

    pub fn active_view(&self) -> Option<&EditorViewState> {
        self.view(self.active_view_id)
    }

    pub fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.view_mut(self.active_view_id)
    }

    pub fn view(&self, view_id: ViewId) -> Option<&EditorViewState> {
        self.views.iter().find(|view| view.id == view_id)
    }

    pub fn view_mut(&mut self, view_id: ViewId) -> Option<&mut EditorViewState> {
        self.views.iter_mut().find(|view| view.id == view_id)
    }

    pub fn ordered_view_ids(&self) -> Vec<ViewId> {
        let mut ordered = Vec::new();
        self.root_pane.collect_view_ids_in_order(&mut ordered);
        ordered
    }

    pub fn reset_to_single_view(&mut self, buffer_id: BufferId) {
        let initial_view = EditorViewState::new(buffer_id);
        self.active_view_id = initial_view.id;
        self.root_pane = PaneNode::leaf(initial_view.id);
        self.views = vec![initial_view];
    }

    pub fn retain_views_for_buffer_ids(&mut self, valid_buffer_ids: &HashSet<BufferId>) -> bool {
        self.views
            .retain(|view| valid_buffer_ids.contains(&view.buffer_id));
        !self.views.is_empty()
    }

    pub fn repair_root_pane_for_current_views(&mut self) -> bool {
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

    pub fn ensure_active_view_is_present(&mut self) {
        if !self.root_pane.contains_view(self.active_view_id) {
            self.active_view_id = self.root_pane.first_view_id();
        }
    }

    pub fn remove_view(&mut self, view_id: ViewId) -> bool {
        if !self.root_pane.remove_view(view_id) {
            return false;
        }

        self.views.retain(|view| view.id != view_id);
        if self.active_view_id == view_id {
            self.active_view_id = self.root_pane.first_view_id();
        }
        true
    }

    pub fn insert_split_view(
        &mut self,
        target_view_id: ViewId,
        axis: crate::app::domain::SplitAxis,
        new_view: EditorViewState,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let new_view_id = new_view.id;
        if !self
            .root_pane
            .split_view(target_view_id, axis, new_view_id, new_view_first, ratio)
        {
            return None;
        }

        self.views.push(new_view);
        self.active_view_id = new_view_id;
        Some(new_view_id)
    }

    fn pane_view_ids(&self) -> HashSet<ViewId> {
        let mut pane_view_ids = HashSet::new();
        self.root_pane.collect_view_ids(&mut pane_view_ids);
        pane_view_ids
    }

    pub fn into_parts(self) -> (Vec<EditorViewState>, PaneNode, ViewId) {
        (self.views, self.root_pane, self.active_view_id)
    }
}
