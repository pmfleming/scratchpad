use crate::app::domain::tab::summary;
use crate::app::domain::{BufferId, ViewId, WorkspaceTab};
use crate::app::theme;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayTabSlot {
    Workspace(usize),
    Settings,
}

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

    pub(crate) fn set_active_tab_index_clamped(&mut self, index: usize) {
        self.active_tab_index = index.min(self.tabs.len().saturating_sub(1));
    }

    pub fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    pub(crate) fn clear_session_dirty(&mut self) {
        self.session_dirty = false;
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
        self.index_tab_buffers(self.active_tab_index);
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    pub(crate) fn append_restored_tab(&mut self, tab: WorkspaceTab) {
        self.tabs.push(tab);
        self.index_tab_buffers(self.tabs.len() - 1);
    }

    pub fn insert_tab(&mut self, index: usize, tab: WorkspaceTab) {
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active_tab_index = index;
        self.shift_buffer_tab_indices(index, 1);
        self.index_tab_buffers(index);
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    pub fn create_untitled_tab(&mut self) {
        self.append_tab(WorkspaceTab::untitled());
    }

    pub fn close_tab_internal(&mut self, index: usize) {
        let removed = self.tabs.remove(index);
        self.remove_tab_buffers(&removed);
        if self.tabs.is_empty() {
            self.tabs.push(WorkspaceTab::untitled());
            self.active_tab_index = 0;
            self.index_tab_buffers(0);
        } else {
            self.shift_buffer_tab_indices(index, -1);
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
        let changed_start = from_index.min(to_index);
        let changed_end = from_index.max(to_index);

        if self.active_tab_index == from_index {
            self.active_tab_index = to_index;
        } else if from_index < self.active_tab_index && to_index >= self.active_tab_index {
            self.active_tab_index -= 1;
        } else if from_index > self.active_tab_index && to_index <= self.active_tab_index {
            self.active_tab_index += 1;
        }

        self.refresh_buffer_tab_index_range(changed_start..=changed_end);
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

    fn index_tab_buffers(&mut self, tab_index: usize) {
        if let Some(tab) = self.tabs.get(tab_index) {
            for buffer in tab.buffers() {
                self.buffer_tab_index.insert(buffer.id, tab_index);
            }
        }
    }

    fn remove_tab_buffers(&mut self, tab: &WorkspaceTab) {
        for buffer in tab.buffers() {
            self.buffer_tab_index.remove(&buffer.id);
        }
    }

    fn shift_buffer_tab_indices(&mut self, start_index: usize, delta: isize) {
        for tab_index in self.buffer_tab_index.values_mut() {
            if *tab_index >= start_index {
                if delta.is_positive() {
                    *tab_index += delta as usize;
                } else {
                    *tab_index = tab_index.saturating_sub(delta.unsigned_abs());
                }
            }
        }
    }

    fn refresh_buffer_tab_index_range(&mut self, range: std::ops::RangeInclusive<usize>) {
        for tab_index in range {
            self.index_tab_buffers(tab_index);
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
            .map(summary::display_name)
            .unwrap_or_else(|| format!("tab#{index}<missing>"))
    }

    pub(crate) fn total_tab_slots(&self, settings_open: bool) -> usize {
        self.tabs.len() + usize::from(settings_open)
    }

    pub(crate) fn settings_slot_index(
        &self,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Option<usize> {
        settings_open.then_some(settings_tab_index.min(self.tabs.len()))
    }

    pub(crate) fn tab_slot_is_settings(
        &self,
        slot_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> bool {
        self.display_tab_slot(slot_index, settings_open, settings_tab_index)
            == Some(DisplayTabSlot::Settings)
    }

    pub(crate) fn workspace_index_for_slot(
        &self,
        slot_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Option<usize> {
        match self.display_tab_slot(slot_index, settings_open, settings_tab_index)? {
            DisplayTabSlot::Workspace(index) => Some(index),
            DisplayTabSlot::Settings => None,
        }
    }

    pub(crate) fn slot_for_workspace_index(
        &self,
        workspace_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> usize {
        match self.settings_slot_index(settings_open, settings_tab_index) {
            Some(settings_index) if workspace_index >= settings_index => workspace_index + 1,
            _ => workspace_index,
        }
    }

    pub(crate) fn active_tab_slot_index(
        &self,
        showing_settings: bool,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> usize {
        if showing_settings {
            self.settings_slot_index(settings_open, settings_tab_index)
                .unwrap_or(self.tabs.len())
        } else {
            self.slot_for_workspace_index(self.active_tab_index, settings_open, settings_tab_index)
        }
    }

    pub(crate) fn display_tab_slot(
        &self,
        slot_index: usize,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Option<DisplayTabSlot> {
        if slot_index >= self.total_tab_slots(settings_open) {
            return None;
        }

        match self.settings_slot_index(settings_open, settings_tab_index) {
            Some(settings_index) if slot_index == settings_index => Some(DisplayTabSlot::Settings),
            Some(settings_index) if slot_index > settings_index => {
                Some(DisplayTabSlot::Workspace(slot_index - 1))
            }
            _ => Some(DisplayTabSlot::Workspace(slot_index)),
        }
    }

    pub(crate) fn display_tab_slots(
        &self,
        settings_open: bool,
        settings_tab_index: usize,
    ) -> Vec<DisplayTabSlot> {
        (0..self.total_tab_slots(settings_open))
            .filter_map(|slot_index| {
                self.display_tab_slot(slot_index, settings_open, settings_tab_index)
            })
            .collect()
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

        let restored = tab("restored.txt");
        let restored_id = restored.active_buffer().id;
        manager.append_restored_tab(restored);
        assert_eq!(manager.active_tab_index, 0);
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(second_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(split_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(restored_id), Some(2));

        let appended = tab("appended.txt");
        let appended_id = appended.active_buffer().id;
        manager.append_tab(appended);
        assert_eq!(manager.active_tab_index, 3);
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(second_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(split_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(restored_id), Some(2));
        assert_eq!(manager.tab_index_for_buffer(appended_id), Some(3));

        let inserted = tab("inserted.txt");
        let inserted_id = inserted.active_buffer().id;
        manager.insert_tab(1, inserted);
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(inserted_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(second_id), Some(2));
        assert_eq!(manager.tab_index_for_buffer(split_id), Some(2));
        assert_eq!(manager.tab_index_for_buffer(restored_id), Some(3));
        assert_eq!(manager.tab_index_for_buffer(appended_id), Some(4));

        assert!(manager.reorder_tab(2, 0));
        assert_eq!(manager.tab_index_for_buffer(second_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(split_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(inserted_id), Some(2));
        assert_eq!(manager.tab_index_for_buffer(restored_id), Some(3));
        assert_eq!(manager.tab_index_for_buffer(appended_id), Some(4));

        manager.close_tab_internal(0);
        assert_eq!(manager.tab_index_for_buffer(second_id), None);
        assert_eq!(manager.tab_index_for_buffer(split_id), None);
        assert_eq!(manager.tab_index_for_buffer(first_id), Some(0));
        assert_eq!(manager.tab_index_for_buffer(inserted_id), Some(1));
        assert_eq!(manager.tab_index_for_buffer(restored_id), Some(2));
        assert_eq!(manager.tab_index_for_buffer(appended_id), Some(3));
    }
}
