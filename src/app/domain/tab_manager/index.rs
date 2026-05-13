use super::TabManager;
use crate::app::domain::{BufferId, WorkspaceTab};
use crate::app::services::session_store::ColdSessionTab;

impl TabManager {
    pub(crate) fn set_cold_session_tab(&mut self, index: usize, tab: ColdSessionTab) {
        if index < self.tabs.len() {
            self.cold_session_tabs.insert(index, tab);
        }
    }

    pub(crate) fn take_cold_session_tab(&mut self, index: usize) -> Option<ColdSessionTab> {
        self.cold_session_tabs.remove(&index)
    }

    pub(crate) fn cold_session_tabs(&self) -> &std::collections::HashMap<usize, ColdSessionTab> {
        &self.cold_session_tabs
    }

    pub(crate) fn replace_restored_tab(&mut self, index: usize, tab: WorkspaceTab) -> bool {
        let Some(slot) = self.tabs.get_mut(index) else {
            return false;
        };
        let removed = std::mem::replace(slot, tab);
        self.remove_tab_buffers(&removed);
        self.index_tab_buffers(index);
        true
    }

    pub fn rebuild_buffer_tab_index(&mut self) {
        self.buffer_tab_index.clear();
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            for buffer in tab.buffers() {
                self.buffer_tab_index.insert(buffer.id, tab_index);
            }
        }
    }

    pub(super) fn index_tab_buffers(&mut self, tab_index: usize) {
        if let Some(tab) = self.tabs.get(tab_index) {
            for buffer in tab.buffers() {
                self.buffer_tab_index.insert(buffer.id, tab_index);
            }
        }
    }

    pub(super) fn remove_tab_buffers(&mut self, tab: &WorkspaceTab) {
        for buffer in tab.buffers() {
            self.buffer_tab_index.remove(&buffer.id);
        }
    }

    pub(super) fn shift_buffer_tab_indices(&mut self, start_index: usize, delta: isize) {
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

    pub(super) fn shift_cold_tab_indices(&mut self, start_index: usize, delta: isize) {
        let old = std::mem::take(&mut self.cold_session_tabs);
        for (tab_index, cold_tab) in old {
            let next_index = shifted_tab_index(tab_index, start_index.., delta);
            self.cold_session_tabs.insert(next_index, cold_tab);
        }
    }

    pub(super) fn shift_cold_tab_range(
        &mut self,
        range: std::ops::RangeInclusive<usize>,
        delta: isize,
    ) {
        let old = std::mem::take(&mut self.cold_session_tabs);
        for (tab_index, cold_tab) in old {
            let next_index = shifted_tab_index(tab_index, range.clone(), delta);
            self.cold_session_tabs.insert(next_index, cold_tab);
        }
    }

    pub(super) fn refresh_buffer_tab_index_range(
        &mut self,
        range: std::ops::RangeInclusive<usize>,
    ) {
        for tab_index in range {
            self.index_tab_buffers(tab_index);
        }
    }

    pub fn tab_index_for_buffer(&self, buffer_id: BufferId) -> Option<usize> {
        self.buffer_tab_index.get(&buffer_id).copied()
    }
}

fn shifted_tab_index(
    tab_index: usize,
    shifted_range: impl std::ops::RangeBounds<usize>,
    delta: isize,
) -> usize {
    if shifted_range.contains(&tab_index) {
        if delta.is_positive() {
            tab_index + delta as usize
        } else {
            tab_index.saturating_sub(delta.unsigned_abs())
        }
    } else {
        tab_index
    }
}
