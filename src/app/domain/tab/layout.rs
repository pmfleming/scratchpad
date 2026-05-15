use super::WorkspaceTab;
use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, SplitAxis, ViewId};
use std::collections::HashSet;

struct ViewPresentationState {
    show_line_numbers: bool,
}

impl WorkspaceTab {
    pub fn clear_transient_view_state(&mut self) {
        let mut anchors_to_release = Vec::new();
        for view in self.layout.views_mut() {
            view.editor_has_focus = false;
            view.latest_display_snapshot = None;
            view.latest_display_snapshot_revision = None;
            for anchor in view.take_runtime_anchors_for_release() {
                anchors_to_release.push((view.buffer_id, anchor));
            }
        }

        for (buffer_id, anchor) in anchors_to_release {
            if let Some(buffer) = self.buffer_by_id_mut(buffer_id) {
                buffer
                    .document_mut()
                    .piece_tree_mut()
                    .release_anchor(anchor);
            }
        }
    }

    pub fn clear_view_state_for_buffer_replacement(&mut self, buffer_id: BufferId) {
        let mut anchors_to_release = Vec::new();
        for view in self.layout.views_mut() {
            if view.buffer_id != buffer_id {
                continue;
            }
            view.latest_display_snapshot = None;
            view.latest_display_snapshot_revision = None;
            for anchor in view.take_runtime_anchors_for_release() {
                anchors_to_release.push(anchor);
            }
        }

        if let Some(buffer) = self.buffer_by_id_mut(buffer_id) {
            for anchor in anchors_to_release {
                buffer
                    .document_mut()
                    .piece_tree_mut()
                    .release_anchor(anchor);
            }
        }
    }

    pub fn close_view(&mut self, view_id: ViewId) -> bool {
        if self.layout.leaf_count() <= 1 {
            return false;
        }

        if !self.layout.contains_view(view_id) {
            return false;
        }

        if let Some((buffer, view)) = self.buffer_and_view_mut(view_id) {
            for anchor in view.take_runtime_anchors_for_release() {
                buffer
                    .document_mut()
                    .piece_tree_mut()
                    .release_anchor(anchor);
            }
        }

        if !self.layout.remove_view(view_id) {
            return false;
        }

        self.sync_active_buffer_to_active_view();
        self.prune_unused_buffers();
        true
    }

    pub(crate) fn ordered_view_ids_in_layout_order(&self) -> Vec<ViewId> {
        self.layout.ordered_view_ids()
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
        let active_buffer_id = self.active_buffer().id;
        self.split_view_for_buffer(
            self.layout.active_view_id(),
            active_buffer_id,
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
        self.open_buffer_in_view(
            self.layout.active_view_id(),
            buffer,
            axis,
            place_after,
            ratio,
        )
    }

    pub fn open_buffer_with_balanced_layout(&mut self, buffer: BufferState) -> Option<ViewId> {
        let (target_view_id, target_depth) = self.layout.root_pane.shallowest_leaf();
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
        let new_view_id =
            self.split_view_for_buffer(target_view_id, buffer.id, axis, new_view_first, ratio)?;
        self.buffers.push_extra(buffer);
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
        let target_view_id = self.layout.active_view_id();
        let WorkspaceTab { buffers, layout } = source;
        let (buffer, extra_buffers) = buffers.into_parts();
        let (views, root_pane, active_view_id) = layout.into_parts();

        if !self.layout.root_pane.split_view_with_node(
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
        self.layout.views.extend(views);
        self.layout.active_view_id = active_view_id;
        self.sync_active_buffer_to_active_view();
        Some(active_view_id)
    }

    pub fn rebalance_views_equally(&mut self) -> bool {
        self.rebalance_views_equally_for_axis(SplitAxis::Vertical)
    }

    pub fn rebalance_views_equally_for_axis(&mut self, root_axis: SplitAxis) -> bool {
        if self.layout.views.is_empty() {
            return false;
        }

        let ordered_view_ids = self.rebalanced_view_order();
        let Some(root_pane) = Self::balanced_root_from_view_ids(&ordered_view_ids, root_axis)
        else {
            return false;
        };

        self.layout.root_pane = root_pane;
        self.sync_active_buffer_to_active_view()
    }

    fn view_presentation_state(&self, view_id: ViewId) -> Option<ViewPresentationState> {
        let source_view = self.layout.view(view_id)?;
        Some(ViewPresentationState {
            show_line_numbers: source_view.show_line_numbers,
        })
    }

    fn build_split_view(
        buffer_id: BufferId,
        source_view: &EditorViewState,
        presentation: ViewPresentationState,
    ) -> EditorViewState {
        let mut new_view = EditorViewState::new(buffer_id);
        new_view.show_line_numbers = presentation.show_line_numbers;
        new_view.cursor_range = source_view.cursor_range;
        new_view.pending_cursor_range = source_view.pending_cursor_range;
        new_view.scroll = source_view.scroll.clone();
        new_view
            .latest_display_snapshot
            .clone_from(&source_view.latest_display_snapshot);
        new_view.latest_display_snapshot_revision = source_view.latest_display_snapshot_revision;
        new_view.layout_cache = source_view.layout_cache.clone();
        new_view.search_highlights = source_view.search_highlights.clone();
        new_view
            .search_replacement_preview
            .clone_from(&source_view.search_replacement_preview);
        new_view
    }

    fn split_view_for_buffer(
        &mut self,
        target_view_id: ViewId,
        buffer_id: BufferId,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let presentation = self.view_presentation_state(target_view_id)?;
        let source_view = self.layout.view(target_view_id)?;
        let new_view = Self::build_split_view(buffer_id, source_view, presentation);
        self.layout
            .insert_split_view(target_view_id, axis, new_view, new_view_first, ratio)
    }

    fn rebalanced_view_order(&self) -> Vec<ViewId> {
        let mut ordered_view_ids = self.ordered_view_ids_from_layout();
        self.append_missing_view_ids(&mut ordered_view_ids);
        ordered_view_ids
    }

    fn ordered_view_ids_from_layout(&self) -> Vec<ViewId> {
        let mut ordered_view_ids = Vec::with_capacity(self.layout.views.len());
        self.layout
            .root_pane
            .collect_view_ids_in_order(&mut ordered_view_ids);
        ordered_view_ids
    }

    fn append_missing_view_ids(&self, ordered_view_ids: &mut Vec<ViewId>) {
        if ordered_view_ids.len() >= self.layout.views.len() {
            return;
        }

        let mut seen_view_ids = ordered_view_ids.iter().copied().collect::<HashSet<_>>();
        for view in &self.layout.views {
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
