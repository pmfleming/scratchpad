use crate::app::domain::{BufferId, ViewId, WorkspaceTab};
use crate::app::theme;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingAction {
    CloseTab(usize),
    CloseView { tab_index: usize, view_id: ViewId },
    SaveConflict { tab_index: usize, view_id: ViewId },
}

#[derive(Clone)]
pub struct TabManager {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub pending_action: Option<PendingAction>,
    pub(crate) session_dirty: bool,
    pub(crate) pending_scroll_to_active: bool,
    pub(crate) buffer_tab_index: HashMap<BufferId, usize>,
}

impl Default for TabManager {
    fn default() -> Self {
        let mut manager = Self {
            tabs: vec![WorkspaceTab::untitled()],
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: true,
            buffer_tab_index: HashMap::new(),
        };
        manager.rebuild_buffer_tab_index();
        manager
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

    pub fn evict_inactive_tab_state(&mut self) {
        let active_index = self.active_tab_index;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if index != active_index {
                tab.clear_transient_view_state();
            }
        }
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
        self.rebuild_buffer_tab_index();
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    pub fn insert_tab(&mut self, index: usize, tab: WorkspaceTab) {
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active_tab_index = index;
        self.rebuild_buffer_tab_index();
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
        self.rebuild_buffer_tab_index();
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

        self.rebuild_buffer_tab_index();
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
        true
    }

    pub fn set_tabs(&mut self, tabs: Vec<WorkspaceTab>, active_tab_index: usize) {
        self.tabs = tabs;
        self.active_tab_index = active_tab_index.min(self.tabs.len().saturating_sub(1));
        self.rebuild_buffer_tab_index();
    }

    pub fn rebuild_buffer_tab_index(&mut self) {
        self.buffer_tab_index.clear();
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            for buffer in tab.buffers() {
                self.buffer_tab_index.insert(buffer.id, tab_index);
            }
        }
    }

    pub fn tab_index_for_buffer(&self, buffer_id: BufferId) -> Option<usize> {
        self.buffer_tab_index.get(&buffer_id).copied()
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
            .map(|t| t.display_name())
            .unwrap_or_else(|| format!("tab#{index}<missing>"))
    }

    pub fn describe_active_tab(&self) -> String {
        self.describe_tab_at(self.active_tab_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::{BufferState, SplitAxis};

    fn tab(name: &str) -> WorkspaceTab {
        WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
    }

    #[test]
    fn buffer_tab_index_tracks_tab_mutations() {
        let first = tab("first.txt");
        let first_id = first.active_buffer().id;

        let mut second = tab("second.txt");
        let second_id = second.active_buffer().id;
        let split_buffer = BufferState::new("split.txt".to_owned(), String::new(), None);
        let split_id = split_buffer.id;
        second
            .open_buffer_as_split(split_buffer, SplitAxis::Vertical, true, 0.5)
            .unwrap();

        let mut manager = TabManager::new();
        manager.set_tabs(vec![first, second], 0);

        assert_eq!(manager.tab_index_for_buffer(first_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(second_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(split_id), Some(1));

        assert!(manager.reorder_tab(1, 0));
        assert_eq!(manager.tab_index_for_buffer(second_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(split_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(1));

        manager.close_tab_internal(0);
        assert_eq!(manager.tab_index_for_buffer(second_id), None);
        assert_eq!(manager.tab_index_for_buffer(split_id), None);
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(0));
    }
}
