use crate::app::CanonicalPathKey;
use crate::app::domain::tab::summary;
use crate::app::domain::{BufferId, ViewId, WorkspaceTab};
use crate::app::services::session_store::ColdSessionTab;
use crate::app::theme;
use std::collections::HashMap;

mod display;
mod index;
#[cfg(test)]
mod tests;

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
    pub(crate) path_tab_index: HashMap<CanonicalPathKey, (BufferId, usize, ViewId)>,
    pub(crate) cold_session_tabs: HashMap<usize, ColdSessionTab>,
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
            path_tab_index: HashMap::new(),
            cold_session_tabs: HashMap::new(),
        };
        manager.rebuild_buffer_tab_index();
        manager
    }
}

impl TabManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
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

    #[must_use]
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

    pub(crate) fn append_restored_tab(&mut self, tab: WorkspaceTab) -> bool {
        let previous_len = self.tabs.len();
        self.tabs.push(tab);
        self.dedupe_duplicate_path_owners();
        self.rebuild_buffer_tab_index();
        self.tabs.len() > previous_len
    }

    pub fn insert_tab(&mut self, index: usize, tab: WorkspaceTab) {
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active_tab_index = index;
        self.shift_buffer_tab_indices(index, 1);
        self.shift_path_tab_indices(index, 1);
        self.shift_cold_tab_indices(index, 1);
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
        self.cold_session_tabs.remove(&index);
        if self.tabs.is_empty() {
            self.tabs.push(WorkspaceTab::untitled());
            self.active_tab_index = 0;
            self.index_tab_buffers(0);
        } else {
            self.shift_buffer_tab_indices(index, -1);
            self.shift_path_tab_indices(index, -1);
            self.shift_cold_tab_indices(index, -1);
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
        let moved_cold_tab = self.cold_session_tabs.remove(&from_index);
        if from_index < to_index {
            self.shift_cold_tab_range((from_index + 1)..=to_index, -1);
        } else {
            self.shift_cold_tab_range(to_index..=(from_index - 1), 1);
        }
        if let Some(cold_tab) = moved_cold_tab {
            self.cold_session_tabs.insert(to_index, cold_tab);
        }
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
        self.cold_session_tabs.clear();
        self.dedupe_duplicate_path_owners();
        self.active_tab_index = active_tab_index.min(self.tabs.len().saturating_sub(1));
        self.rebuild_buffer_tab_index();
    }

    pub(crate) fn reorder_tabs_by_original_indices(&mut self, original_indices: &[usize]) {
        if self.tabs.len() != original_indices.len() {
            return;
        }

        let old_cold_tabs = std::mem::take(&mut self.cold_session_tabs);
        let mut indexed_tabs = std::mem::take(&mut self.tabs)
            .into_iter()
            .enumerate()
            .map(|(position, tab)| {
                let original_index = original_indices[position];
                (original_index, position, tab)
            })
            .collect::<Vec<_>>();
        indexed_tabs.sort_by_key(|(original_index, _, _)| *original_index);
        self.tabs = indexed_tabs
            .into_iter()
            .enumerate()
            .map(|(new_index, (_, old_position, tab))| {
                if let Some(cold_tab) = old_cold_tabs.get(&old_position).cloned() {
                    self.cold_session_tabs.insert(new_index, cold_tab);
                }
                tab
            })
            .collect();
        self.dedupe_duplicate_path_owners();
        self.rebuild_buffer_tab_index();
    }

    #[must_use]
    pub fn find_tab_by_path(&self, candidate: &std::path::Path) -> Option<(usize, ViewId)> {
        let key = CanonicalPathKey::from_path(candidate);
        self.find_tab_by_path_key(&key)
    }

    #[must_use]
    pub(crate) fn find_tab_by_path_key(&self, key: &CanonicalPathKey) -> Option<(usize, ViewId)> {
        let indexed = self
            .path_tab_index
            .get(key)
            .and_then(|(buffer_id, tab_index, view_id)| {
                let tab = self.tabs.get(*tab_index)?;
                let buffer = tab.buffer_by_id(*buffer_id)?;
                (buffer.path_key.as_ref() == Some(key)
                    && tab
                        .layout
                        .view(*view_id)
                        .is_some_and(|view| view.buffer_id == *buffer_id))
                .then_some((*tab_index, *view_id))
            });

        #[cfg(debug_assertions)]
        {
            let scanned = self.scan_tab_by_path_key(key);
            debug_assert_eq!(indexed, scanned, "path index mismatch for {}", key.as_str());
            indexed.or(scanned)
        }

        #[cfg(not(debug_assertions))]
        {
            indexed
        }
    }

    pub fn describe_tab_at(&self, index: usize) -> String {
        self.tabs
            .get(index)
            .map_or_else(|| format!("tab#{index}<missing>"), summary::display_name)
    }

    #[must_use]
    pub fn describe_active_tab(&self) -> String {
        self.describe_tab_at(self.active_tab_index)
    }
}
