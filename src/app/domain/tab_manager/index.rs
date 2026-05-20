use super::TabManager;
use crate::app::CanonicalPathKey;
use crate::app::domain::{BufferId, ViewId, WorkspaceTab};
use crate::app::services::session_store::ColdSessionTab;
use std::collections::HashSet;

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
        self.dedupe_duplicate_path_owners();
        self.rebuild_buffer_tab_index();
        index < self.tabs.len()
    }

    pub fn rebuild_buffer_tab_index(&mut self) {
        self.buffer_tab_index.clear();
        self.path_tab_index.clear();
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            for buffer in tab.buffers() {
                self.buffer_tab_index.insert(buffer.id, tab_index);
            }
            for (key, owner) in tab_path_entries(tab_index, tab) {
                let previous = self.path_tab_index.insert(key.clone(), owner);
                debug_assert!(
                    previous.is_none() || previous == Some(owner),
                    "duplicate CanonicalPathKey indexed for {}",
                    key.as_str()
                );
            }
        }
    }

    pub(super) fn index_tab_buffers(&mut self, tab_index: usize) {
        if let Some(tab) = self.tabs.get(tab_index) {
            for buffer in tab.buffers() {
                self.buffer_tab_index.insert(buffer.id, tab_index);
            }
            let path_entries = tab_path_entries(tab_index, tab);
            for (key, owner) in path_entries {
                let previous = self.path_tab_index.insert(key.clone(), owner);
                debug_assert!(
                    previous.is_none() || previous == Some(owner),
                    "duplicate CanonicalPathKey indexed for {}",
                    key.as_str()
                );
            }
        }
    }

    pub(super) fn remove_tab_buffers(&mut self, tab: &WorkspaceTab) {
        for buffer in tab.buffers() {
            self.buffer_tab_index.remove(&buffer.id);
            if let Some(key) = buffer.path_key.as_ref() {
                self.path_tab_index.remove(key);
            }
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

    pub(super) fn shift_path_tab_indices(&mut self, start_index: usize, delta: isize) {
        for (_, tab_index, _) in self.path_tab_index.values_mut() {
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

    #[must_use]
    pub fn tab_index_for_buffer(&self, buffer_id: BufferId) -> Option<usize> {
        self.buffer_tab_index.get(&buffer_id).copied()
    }

    #[must_use]
    pub(crate) fn path_owner(&self, key: &CanonicalPathKey) -> Option<(BufferId, usize, ViewId)> {
        self.path_tab_index.get(key).copied()
    }

    pub(crate) fn scan_tab_by_path_key(&self, key: &CanonicalPathKey) -> Option<(usize, ViewId)> {
        self.tabs.iter().enumerate().find_map(|(tab_index, tab)| {
            tab.layout.views().iter().find_map(|view| {
                tab.buffer_by_id(view.buffer_id)
                    .and_then(|buffer| buffer.path_key.as_ref())
                    .is_some_and(|path_key| path_key == key)
                    .then_some((tab_index, view.id))
            })
        })
    }

    pub(crate) fn dedupe_duplicate_path_owners(&mut self) {
        let mut seen = HashSet::<CanonicalPathKey>::new();
        let mut deduped_tabs = Vec::with_capacity(self.tabs.len());
        let old_tabs = std::mem::take(&mut self.tabs);

        for mut tab in old_tabs {
            if tab.remove_duplicate_path_buffers(&mut seen) {
                deduped_tabs.push(tab);
            }
        }

        if deduped_tabs.is_empty() {
            deduped_tabs.push(WorkspaceTab::untitled());
        }

        self.tabs = deduped_tabs;
        self.active_tab_index = self.active_tab_index.min(self.tabs.len().saturating_sub(1));
    }
}

fn tab_path_entries(
    tab_index: usize,
    tab: &WorkspaceTab,
) -> Vec<(CanonicalPathKey, (BufferId, usize, ViewId))> {
    tab.buffers()
        .filter_map(|buffer| {
            let key = buffer.path_key.clone()?;
            let view_id = first_view_for_buffer(tab, buffer.id)?;
            Some((key, (buffer.id, tab_index, view_id)))
        })
        .collect()
}

fn first_view_for_buffer(tab: &WorkspaceTab, buffer_id: BufferId) -> Option<ViewId> {
    tab.layout
        .views()
        .iter()
        .find(|view| view.buffer_id == buffer_id)
        .map(|view| view.id)
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
