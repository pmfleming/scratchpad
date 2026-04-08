use crate::app::domain::{
    BufferId, BufferState, EditorViewState, PaneNode, SplitAxis, SplitPath, ViewId,
};
use std::collections::{HashMap, HashSet};

struct ViewPromotionPlan {
    promoted_buffer_id: BufferId,
    promoted_view_ids: HashSet<ViewId>,
    promoted_root: PaneNode,
    promoted_active_view_id: ViewId,
    remaining_active_view_id: ViewId,
    replacement_buffer_id: BufferId,
}

struct FileTabParts {
    buffer: BufferState,
    views: Vec<EditorViewState>,
    root_pane: PaneNode,
    active_view_id: ViewId,
}

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

    pub fn buffer_by_id(&self, buffer_id: BufferId) -> Option<&BufferState> {
        if self.buffer.id == buffer_id {
            Some(&self.buffer)
        } else {
            self.extra_buffers
                .iter()
                .find(|buffer| buffer.id == buffer_id)
        }
    }

    pub fn buffer_by_id_mut(&mut self, buffer_id: BufferId) -> Option<&mut BufferState> {
        if self.buffer.id == buffer_id {
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
            let buffer_index = extra_buffers
                .iter()
                .position(|candidate| candidate.id == buffer_id)?;
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
        if self.view(view_id).is_none() {
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

    pub fn split_active_view(&mut self, axis: SplitAxis) -> Option<ViewId> {
        self.split_active_view_with_placement(axis, false, 0.5)
    }

    pub fn split_active_view_with_placement(
        &mut self,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) -> Option<ViewId> {
        let source_view = self.active_view()?;
        let mut new_view = EditorViewState::new(
            source_view.buffer_id,
            source_view.show_control_chars && self.buffer.artifact_summary.has_control_chars(),
        );
        new_view.show_line_numbers = source_view.show_line_numbers;
        let new_view_id = new_view.id;
        if self.root_pane.split_view(
            self.active_view_id,
            axis,
            new_view_id,
            new_view_first,
            ratio,
        ) {
            self.views.push(new_view);
            self.active_view_id = new_view_id;
            Some(new_view_id)
        } else {
            None
        }
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
        let (show_line_numbers, show_control_chars) = {
            let source_view = self.view(target_view_id)?;
            (
                source_view.show_line_numbers,
                source_view.show_control_chars,
            )
        };
        let mut new_view =
            EditorViewState::new(buffer.id, buffer.artifact_summary.has_control_chars());
        new_view.show_line_numbers = show_line_numbers;
        new_view.show_control_chars =
            show_control_chars && buffer.artifact_summary.has_control_chars();
        let new_view_id = new_view.id;

        if !self
            .root_pane
            .split_view(target_view_id, axis, new_view_id, new_view_first, ratio)
        {
            return None;
        }

        self.extra_buffers.push(buffer);
        self.views.push(new_view);
        self.active_view_id = new_view_id;
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
        if self.views.is_empty() {
            return false;
        }

        let mut ordered_view_ids = Vec::with_capacity(self.views.len());
        self.root_pane
            .collect_view_ids_in_order(&mut ordered_view_ids);

        if ordered_view_ids.len() < self.views.len() {
            for view in &self.views {
                if !ordered_view_ids.contains(&view.id) {
                    ordered_view_ids.push(view.id);
                }
            }
        }

        let Some(root_pane) =
            PaneNode::balanced_from_view_ids(&ordered_view_ids, SplitAxis::Vertical)
        else {
            return false;
        };

        self.root_pane = root_pane;
        self.sync_active_buffer_to_active_view()
    }

    pub fn promote_view_to_new_tab(&mut self, view_id: ViewId) -> Option<WorkspaceTab> {
        if !self.can_promote_view(view_id) {
            return None;
        }

        let plan = self.build_view_promotion_plan(view_id)?;
        let (remaining_views, promoted_views) = self.take_partitioned_views(&plan.promoted_view_ids);
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
        let mut ordered_buffer_ids = Self::ordered_buffer_ids(&views, &ordered_view_ids);

        let mut buffers = std::iter::once(buffer)
            .chain(extra_buffers)
            .map(|buffer| (buffer.id, buffer))
            .collect::<HashMap<_, _>>();
        Self::append_missing_buffer_ids(&mut ordered_buffer_ids, &views);

        let mut views_by_buffer = Self::group_views_by_buffer(views);
        let view_order = Self::view_order_lookup(&ordered_view_ids);

        ordered_buffer_ids
            .into_iter()
            .filter_map(|buffer_id| {
                Self::take_file_tab_parts(
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

    fn repair_restored_state(&mut self) {
        let valid_buffer_ids = self
            .buffers()
            .map(|buffer| buffer.id)
            .collect::<HashSet<_>>();
        self.views
            .retain(|view| valid_buffer_ids.contains(&view.buffer_id));

        let valid_view_ids = self
            .views
            .iter()
            .map(|view| view.id)
            .collect::<HashSet<_>>();

        if valid_view_ids.is_empty() || !self.root_pane.retain_views(&valid_view_ids) {
            self.reset_to_single_view();
            return;
        }

        let mut pane_view_ids = HashSet::new();
        self.root_pane.collect_view_ids(&mut pane_view_ids);
        self.views.retain(|view| pane_view_ids.contains(&view.id));

        if self.views.is_empty() {
            self.reset_to_single_view();
            return;
        }

        if !pane_view_ids.contains(&self.active_view_id) {
            self.active_view_id = self.root_pane.first_view_id();
        }

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

    fn push_buffer_if_missing(&mut self, buffer: BufferState) {
        if self.buffer.id == buffer.id || self.extra_buffers.iter().any(|item| item.id == buffer.id)
        {
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

    fn build_view_promotion_plan(&mut self, view_id: ViewId) -> Option<ViewPromotionPlan> {
        let promoted_buffer_id = self.view(view_id)?.buffer_id;
        let promoted_view_ids = self.view_ids_for_buffer(promoted_buffer_id);
        let remaining_view_ids = self.view_ids_excluding_buffer(promoted_buffer_id);

        if promoted_view_ids.is_empty() || remaining_view_ids.is_empty() {
            return None;
        }

        let promoted_root = self.retained_root_for_views(&promoted_view_ids)?;
        if !self.root_pane.retain_views(&remaining_view_ids) {
            return None;
        }

        let promoted_active_view_id = Self::resolved_active_view_id(
            &promoted_view_ids,
            view_id,
            &promoted_root,
        );
        let remaining_active_view_id = Self::resolved_active_view_id(
            &remaining_view_ids,
            self.active_view_id,
            &self.root_pane,
        );
        let replacement_buffer_id = self.view(remaining_active_view_id)?.buffer_id;

        Some(ViewPromotionPlan {
            promoted_buffer_id,
            promoted_view_ids,
            promoted_root,
            promoted_active_view_id,
            remaining_active_view_id,
            replacement_buffer_id,
        })
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
        retained_root.retain_views(view_ids).then_some(retained_root)
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

    fn sync_active_buffer_to_active_view(&mut self) -> bool {
        let Some(active_buffer_id) = self.active_view().map(|view| view.buffer_id) else {
            return false;
        };

        if self.buffer.id == active_buffer_id {
            return true;
        }

        let Some(buffer_index) = self
            .extra_buffers
            .iter()
            .position(|buffer| buffer.id == active_buffer_id)
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
        views.iter()
            .find(|view| view.id == active_view_id)
            .map(|view| view.buffer_id)
    }

    fn ordered_buffer_ids(views: &[EditorViewState], ordered_view_ids: &[ViewId]) -> Vec<BufferId> {
        let buffer_by_view_id = views
            .iter()
            .map(|view| (view.id, view.buffer_id))
            .collect::<HashMap<_, _>>();
        let mut ordered_buffer_ids = Vec::new();

        for view_id in ordered_view_ids {
            if let Some(buffer_id) = buffer_by_view_id.get(view_id)
                && !ordered_buffer_ids.contains(buffer_id)
            {
                ordered_buffer_ids.push(*buffer_id);
            }
        }

        ordered_buffer_ids
    }

    fn append_missing_buffer_ids(
        ordered_buffer_ids: &mut Vec<BufferId>,
        views: &[EditorViewState],
    ) {
        for buffer_id in views.iter().map(|view| view.buffer_id) {
            if !ordered_buffer_ids.contains(&buffer_id) {
                ordered_buffer_ids.push(buffer_id);
            }
        }
    }

    fn group_views_by_buffer(views: Vec<EditorViewState>) -> HashMap<BufferId, Vec<EditorViewState>> {
        views.into_iter().fold(HashMap::new(), |mut groups, view| {
            groups.entry(view.buffer_id).or_default().push(view);
            groups
        })
    }

    fn view_order_lookup(ordered_view_ids: &[ViewId]) -> HashMap<ViewId, usize> {
        ordered_view_ids
            .iter()
            .enumerate()
            .map(|(index, view_id)| (*view_id, index))
            .collect()
    }

    fn take_file_tab_parts(
        buffer_id: BufferId,
        root_pane: &PaneNode,
        active_view_id: ViewId,
        active_buffer_id: Option<BufferId>,
        buffers: &mut HashMap<BufferId, BufferState>,
        views_by_buffer: &mut HashMap<BufferId, Vec<EditorViewState>>,
        view_order: &HashMap<ViewId, usize>,
    ) -> Option<FileTabParts> {
        let buffer = buffers.remove(&buffer_id)?;
        let mut views = views_by_buffer.remove(&buffer_id)?;
        Self::sort_views_by_layout_order(&mut views, view_order);
        let root_pane = Self::file_root_pane(root_pane, &views);
        let active_view_id =
            Self::file_active_view_id(buffer_id, active_buffer_id, active_view_id, &views, &root_pane);

        Some(FileTabParts {
            buffer,
            views,
            root_pane,
            active_view_id,
        })
    }

    fn sort_views_by_layout_order(
        views: &mut [EditorViewState],
        view_order: &HashMap<ViewId, usize>,
    ) {
        views.sort_by_key(|view| view_order.get(&view.id).copied().unwrap_or(usize::MAX));
    }

    fn file_root_pane(root_pane: &PaneNode, views: &[EditorViewState]) -> PaneNode {
        let file_view_ids = views.iter().map(|view| view.id).collect::<HashSet<_>>();
        let mut file_root = root_pane.clone();
        if !file_root.retain_views(&file_view_ids) {
            PaneNode::leaf(views[0].id)
        } else {
            file_root
        }
    }

    fn file_active_view_id(
        buffer_id: BufferId,
        active_buffer_id: Option<BufferId>,
        active_view_id: ViewId,
        views: &[EditorViewState],
        root_pane: &PaneNode,
    ) -> ViewId {
        let file_view_ids = views.iter().map(|view| view.id).collect::<HashSet<_>>();
        if active_buffer_id == Some(buffer_id) && file_view_ids.contains(&active_view_id) {
            active_view_id
        } else {
            root_pane.first_view_id()
        }
    }
}
