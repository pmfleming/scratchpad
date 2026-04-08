use crate::app::domain::{BufferState, EditorViewState, PaneNode, SplitAxis, SplitPath, ViewId};
use std::collections::HashSet;

pub struct WorkspaceTab {
    pub buffer: BufferState,
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
        let mut tab = Self {
            buffer,
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

    pub fn resize_split(&mut self, path: SplitPath, ratio: f32) -> bool {
        self.root_pane.resize_split(&path, ratio)
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
    }

    fn reset_to_single_view(&mut self) {
        let initial_view = EditorViewState::new(
            self.buffer.id,
            self.buffer.artifact_summary.has_control_chars(),
        );
        self.active_view_id = initial_view.id;
        self.root_pane = PaneNode::leaf(initial_view.id);
        self.views = vec![initial_view];
    }
}
