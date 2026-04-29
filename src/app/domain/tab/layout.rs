use super::WorkspaceTab;
use crate::app::domain::{
    BufferId, BufferState, EditorViewState, PaneNode, SplitAxis, SplitPath, ViewId,
};
use std::collections::HashSet;

struct ViewPresentationState {
    show_line_numbers: bool,
    show_control_chars: bool,
}

impl WorkspaceTab {
    pub fn active_view(&self) -> Option<&EditorViewState> {
        self.view(self.active_view_id)
    }

    pub fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.view_mut(self.active_view_id)
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

    pub fn clear_transient_view_state(&mut self) {
        for view in &mut self.views {
            view.editor_has_focus = false;
            view.latest_display_snapshot = None;
            view.latest_display_snapshot_revision = None;
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

    pub(crate) fn ordered_view_ids_in_layout_order(&self) -> Vec<ViewId> {
        Self::ordered_view_ids(&self.root_pane)
    }

    pub(super) fn ordered_view_ids(root_pane: &PaneNode) -> Vec<ViewId> {
        let mut ordered = Vec::new();
        root_pane.collect_view_ids_in_order(&mut ordered);
        ordered
    }

    pub fn split_active_view(&mut self, axis: SplitAxis) -> Option<ViewId> {
        self.split_active_view_with_placement(axis, false, 0.5)
    }

    pub fn split_active_view_with_placement(
        &mut self,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let (active_buffer_id, has_control_chars) = {
            let active_buffer = self.active_buffer();
            (
                active_buffer.id,
                active_buffer.artifact_summary.has_control_chars(),
            )
        };
        self.split_view_for_buffer(
            self.active_view_id,
            active_buffer_id,
            has_control_chars,
            axis,
            new_view_first,
            ratio,
        )
    }

    pub fn open_buffer_as_split(
        &mut self,
        buffer: BufferState,
        axis: SplitAxis,
        place_after: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        self.open_buffer_in_view(self.active_view_id, buffer, axis, place_after, ratio)
    }

    pub fn open_buffer_with_balanced_layout(&mut self, buffer: BufferState) -> Option<ViewId> {
        let (target_view_id, target_depth) = self.root_pane.shallowest_leaf();
        let axis = if target_depth % 2 == 0 {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        };

        self.open_buffer_in_view(target_view_id, buffer, axis, false, 0.5)
    }

    fn open_buffer_in_view(
        &mut self,
        target_view_id: ViewId,
        buffer: BufferState,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let new_view_id = self.split_view_for_buffer(
            target_view_id,
            buffer.id,
            buffer.artifact_summary.has_control_chars(),
            axis,
            new_view_first,
            ratio,
        )?;
        self.extra_buffers.push(buffer);
        self.sync_active_buffer_to_active_view();
        Some(new_view_id)
    }

    pub fn combine_with_tab(
        &mut self,
        source: WorkspaceTab,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let target_view_id = self.active_view_id;
        let WorkspaceTab {
            buffer,
            extra_buffers,
            views,
            root_pane,
            active_view_id,
        } = source;

        if !self.root_pane.split_view_with_node(
            target_view_id,
            axis,
            root_pane,
            new_view_first,
            ratio,
        ) {
            return None;
        }

        self.push_buffer_if_missing(buffer);
        for extra_buffer in extra_buffers {
            self.push_buffer_if_missing(extra_buffer);
        }
        self.views.extend(views);
        self.active_view_id = active_view_id;
        self.sync_active_buffer_to_active_view();
        Some(active_view_id)
    }

    pub fn resize_split(&mut self, path: SplitPath, ratio: f32) -> bool {
        self.root_pane.resize_split(&path, ratio)
    }

    pub fn rebalance_views_equally(&mut self) -> bool {
        self.rebalance_views_equally_for_axis(SplitAxis::Vertical)
    }

    pub fn rebalance_views_equally_for_axis(&mut self, root_axis: SplitAxis) -> bool {
        if self.views.is_empty() {
            return false;
        }

        let ordered_view_ids = self.rebalanced_view_order();
        let Some(root_pane) = Self::balanced_root_from_view_ids(&ordered_view_ids, root_axis)
        else {
            return false;
        };

        self.root_pane = root_pane;
        self.sync_active_buffer_to_active_view()
    }

    fn view_presentation_state(&self, view_id: ViewId) -> Option<ViewPresentationState> {
        let source_view = self.view(view_id)?;
        Some(ViewPresentationState {
            show_line_numbers: source_view.show_line_numbers,
            show_control_chars: source_view.show_control_chars,
        })
    }

    fn build_split_view(
        buffer_id: BufferId,
        has_control_chars: bool,
        presentation: ViewPresentationState,
    ) -> EditorViewState {
        let mut new_view = EditorViewState::new(buffer_id, false);
        new_view.show_line_numbers = presentation.show_line_numbers;
        new_view.show_control_chars = presentation.show_control_chars && has_control_chars;
        new_view
    }

    fn split_view_for_buffer(
        &mut self,
        target_view_id: ViewId,
        buffer_id: BufferId,
        has_control_chars: bool,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let presentation = self.view_presentation_state(target_view_id)?;
        let new_view = Self::build_split_view(buffer_id, has_control_chars, presentation);
        self.insert_split_view(target_view_id, axis, new_view, new_view_first, ratio)
    }

    fn insert_split_view(
        &mut self,
        target_view_id: ViewId,
        axis: SplitAxis,
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

    fn rebalanced_view_order(&self) -> Vec<ViewId> {
        let mut ordered_view_ids = self.ordered_view_ids_from_layout();
        self.append_missing_view_ids(&mut ordered_view_ids);
        ordered_view_ids
    }

    fn ordered_view_ids_from_layout(&self) -> Vec<ViewId> {
        let mut ordered_view_ids = Vec::with_capacity(self.views.len());
        self.root_pane
            .collect_view_ids_in_order(&mut ordered_view_ids);
        ordered_view_ids
    }

    fn append_missing_view_ids(&self, ordered_view_ids: &mut Vec<ViewId>) {
        if ordered_view_ids.len() >= self.views.len() {
            return;
        }

        let mut seen_view_ids = ordered_view_ids.iter().copied().collect::<HashSet<_>>();
        for view in &self.views {
            if seen_view_ids.insert(view.id) {
                ordered_view_ids.push(view.id);
            }
        }
    }

    fn balanced_root_from_view_ids(
        ordered_view_ids: &[ViewId],
        root_axis: SplitAxis,
    ) -> Option<PaneNode> {
        PaneNode::balanced_from_view_ids(ordered_view_ids, root_axis)
    }
}
