use crate::app::domain::{ViewId, WorkspaceTab};
use crate::app::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PendingAction {
    CloseTab(usize),
}

#[derive(Clone)]
pub struct TabManager {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub pending_action: Option<PendingAction>,
    pub(crate) session_dirty: bool,
    pub(crate) pending_scroll_to_active: bool,
}

impl Default for TabManager {
    fn default() -> Self {
        Self {
            tabs: vec![WorkspaceTab::untitled()],
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: true,
        }
    }
}

impl TabManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    pub fn estimated_tab_strip_width(&self, spacing: f32) -> f32 {
        if self.tabs.is_empty() {
            return 0.0;
        }

        (self.tabs.len() as f32 * theme::TAB_BUTTON_WIDTH)
            + ((self.tabs.len().saturating_sub(1)) as f32 * spacing)
    }

    pub fn append_tab(&mut self, tab: WorkspaceTab) {
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    pub fn create_untitled_tab(&mut self) {
        self.append_tab(WorkspaceTab::untitled());
    }

    pub fn close_tab_internal(&mut self, index: usize) {
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.tabs.push(WorkspaceTab::untitled());
            self.active_tab_index = 0;
        }

        if self.active_tab_index > index {
            self.active_tab_index -= 1;
        }

        self.active_tab_index = self.active_tab_index.min(self.tabs.len() - 1);
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    pub fn reorder_tab(&mut self, from_index: usize, to_index: usize) -> bool {
        let tabs_len = self.tabs.len();
        if from_index >= tabs_len || to_index >= tabs_len || from_index == to_index {
            return false;
        }

        let moved_tab = self.tabs.remove(from_index);
        self.tabs.insert(to_index, moved_tab);

        if self.active_tab_index == from_index {
            self.active_tab_index = to_index;
        } else if from_index < self.active_tab_index && to_index >= self.active_tab_index {
            self.active_tab_index -= 1;
        } else if from_index > self.active_tab_index && to_index <= self.active_tab_index {
            self.active_tab_index += 1;
        }

        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
        true
    }

    pub fn find_tab_by_path(&self, candidate: &std::path::Path) -> Option<(usize, ViewId)> {
        self.tabs.iter().enumerate().find_map(|(tab_index, tab)| {
            tab.views.iter().find_map(|view| {
                tab.buffer_by_id(view.buffer_id)
                    .and_then(|buffer| buffer.path.as_ref())
                    .is_some_and(|path| crate::app::paths_match(path, candidate))
                    .then_some((tab_index, view.id))
            })
        })
    }

    pub fn describe_tab_at(&self, index: usize) -> String {
        self.tabs
            .get(index)
            .map(|t| t.describe())
            .unwrap_or_else(|| format!("tab#{index}<missing>"))
    }

    pub fn describe_active_tab(&self) -> String {
        self.describe_tab_at(self.active_tab_index)
    }
}
