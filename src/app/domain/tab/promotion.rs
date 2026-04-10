use super::WorkspaceTab;
use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, ViewId, tab_support};
use std::collections::{HashMap, HashSet};

impl WorkspaceTab {
    pub fn promote_view_to_new_tab(&mut self, view_id: ViewId) -> Option<WorkspaceTab> {
        if !self.can_promote_view(view_id) {
            return None;
        }

        let plan = self.build_view_promotion_plan(view_id)?;
        let (remaining_views, promoted_views) =
            self.take_partitioned_views(&plan.promoted_view_ids);
        let promoted_buffer =
            self.take_buffer_by_id(plan.promoted_buffer_id, plan.replacement_buffer_id)?;

        self.views = remaining_views;
        self.active_view_id = plan.remaining_active_view_id;
        self.sync_active_buffer_to_active_view();
        self.prune_unused_buffers();

        Some(WorkspaceTab::restored(
            promoted_buffer,
            promoted_views,
            plan.promoted_root,
            plan.promoted_active_view_id,
        ))
    }

    pub fn into_tabs_per_file(self) -> Vec<WorkspaceTab> {
        let WorkspaceTab {
            buffer,
            extra_buffers,
            views,
            root_pane,
            active_view_id,
        } = self;

        let ordered_view_ids = Self::ordered_view_ids(&root_pane);
        let active_buffer_id = Self::active_buffer_id_for_view(&views, active_view_id);
        let mut ordered_buffer_ids = tab_support::ordered_buffer_ids(&views, &ordered_view_ids);

        let mut buffers = std::iter::once(buffer)
            .chain(extra_buffers)
            .map(|buffer| (buffer.id, buffer))
            .collect::<HashMap<_, _>>();
        tab_support::append_missing_buffer_ids(&mut ordered_buffer_ids, &views);

        let mut views_by_buffer = tab_support::group_views_by_buffer(views);
        let view_order = tab_support::view_order_lookup(&ordered_view_ids);

        ordered_buffer_ids
            .into_iter()
            .filter_map(|buffer_id| {
                tab_support::take_file_tab_parts(
                    buffer_id,
                    &root_pane,
                    active_view_id,
                    active_buffer_id,
                    &mut buffers,
                    &mut views_by_buffer,
                    &view_order,
                )
            })
            .map(|parts| {
                WorkspaceTab::restored(
                    parts.buffer,
                    parts.views,
                    parts.root_pane,
                    parts.active_view_id,
                )
            })
            .collect()
    }

    fn build_view_promotion_plan(
        &mut self,
        view_id: ViewId,
    ) -> Option<tab_support::ViewPromotionPlan> {
        let promoted_buffer_id = self.view(view_id)?.buffer_id;
        let promoted_view_ids = self.view_ids_for_buffer(promoted_buffer_id);
        let remaining_view_ids = self.view_ids_excluding_buffer(promoted_buffer_id);

        let promoted_root = self.prepare_view_partition(&promoted_view_ids, &remaining_view_ids)?;
        let promoted_active_view_id =
            Self::resolve_promoted_active_view_id(&promoted_view_ids, view_id, &promoted_root);
        let remaining_active_view_id =
            self.resolve_remaining_active_view_id(&remaining_view_ids)?;
        let replacement_buffer_id = self.view(remaining_active_view_id)?.buffer_id;

        Some(tab_support::ViewPromotionPlan {
            promoted_buffer_id,
            promoted_view_ids,
            promoted_root,
            promoted_active_view_id,
            remaining_active_view_id,
            replacement_buffer_id,
        })
    }

    fn prepare_view_partition(
        &mut self,
        promoted_view_ids: &HashSet<ViewId>,
        remaining_view_ids: &HashSet<ViewId>,
    ) -> Option<PaneNode> {
        if promoted_view_ids.is_empty() || remaining_view_ids.is_empty() {
            return None;
        }

        let promoted_root = self.retained_root_for_views(promoted_view_ids)?;
        self.root_pane
            .retain_views(remaining_view_ids)
            .then_some(promoted_root)
    }

    fn view_ids_for_buffer(&self, buffer_id: BufferId) -> HashSet<ViewId> {
        self.views
            .iter()
            .filter(|view| view.buffer_id == buffer_id)
            .map(|view| view.id)
            .collect()
    }

    fn view_ids_excluding_buffer(&self, buffer_id: BufferId) -> HashSet<ViewId> {
        self.views
            .iter()
            .filter(|view| view.buffer_id != buffer_id)
            .map(|view| view.id)
            .collect()
    }

    fn retained_root_for_views(&self, view_ids: &HashSet<ViewId>) -> Option<PaneNode> {
        let mut retained_root = self.root_pane.clone();
        retained_root
            .retain_views(view_ids)
            .then_some(retained_root)
    }

    fn resolve_promoted_active_view_id(
        promoted_view_ids: &HashSet<ViewId>,
        requested_view_id: ViewId,
        promoted_root: &PaneNode,
    ) -> ViewId {
        Self::resolved_active_view_id(promoted_view_ids, requested_view_id, promoted_root)
    }

    fn resolve_remaining_active_view_id(
        &self,
        remaining_view_ids: &HashSet<ViewId>,
    ) -> Option<ViewId> {
        Some(Self::resolved_active_view_id(
            remaining_view_ids,
            self.active_view_id,
            &self.root_pane,
        ))
    }

    fn resolved_active_view_id(
        available_view_ids: &HashSet<ViewId>,
        preferred_view_id: ViewId,
        root_pane: &PaneNode,
    ) -> ViewId {
        if available_view_ids.contains(&preferred_view_id) {
            preferred_view_id
        } else {
            root_pane.first_view_id()
        }
    }

    fn take_partitioned_views(
        &mut self,
        promoted_view_ids: &HashSet<ViewId>,
    ) -> (Vec<EditorViewState>, Vec<EditorViewState>) {
        let mut remaining_views = Vec::with_capacity(self.views.len() - promoted_view_ids.len());
        let mut promoted_views = Vec::with_capacity(promoted_view_ids.len());
        for view in std::mem::take(&mut self.views) {
            if promoted_view_ids.contains(&view.id) {
                promoted_views.push(view);
            } else {
                remaining_views.push(view);
            }
        }
        (remaining_views, promoted_views)
    }

    fn take_buffer_by_id(
        &mut self,
        buffer_id: BufferId,
        replacement_buffer_id: BufferId,
    ) -> Option<BufferState> {
        if self.buffer.id == buffer_id {
            let replacement_index = self
                .extra_buffers
                .iter()
                .position(|buffer| buffer.id == replacement_buffer_id)?;
            let replacement = self.extra_buffers.swap_remove(replacement_index);
            Some(std::mem::replace(&mut self.buffer, replacement))
        } else {
            let buffer_index = self
                .extra_buffers
                .iter()
                .position(|buffer| buffer.id == buffer_id)?;
            Some(self.extra_buffers.swap_remove(buffer_index))
        }
    }
}
