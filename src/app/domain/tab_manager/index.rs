use super::TabManager;
use crate::app::CanonicalPathKey;
use crate::app::domain::{BufferId, ViewId, WorkspaceTab};
use crate::app::services::session_store::ColdSessionTab;
use std::collections::{HashMap, HashSet};

fn next_path_index_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl TabManager {
    pub(crate) fn set_cold_session_tab(&mut self, index: usize, tab: ColdSessionTab) {
        if index < self.tabs.len() {
            self.cold_session_tabs.insert(index, tab);
        }
    }

    pub(crate) fn append_cold_file_tabs(&mut self, tabs: Vec<(WorkspaceTab, ColdSessionTab)>) {
        for (tab, cold_tab) in tabs {
            self.tabs.push(tab);
            let index = self.tabs.len() - 1;
            self.cold_session_tabs.insert(index, cold_tab);
            self.index_tab_buffers(index);
        }
        self.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }

    pub(crate) fn take_cold_session_tab(&mut self, index: usize) -> Option<ColdSessionTab> {
        self.cold_session_tabs.remove(&index)
    }

    pub(crate) fn cold_session_tabs(&self) -> &HashMap<usize, ColdSessionTab> {
        &self.cold_session_tabs
    }

    pub(crate) fn replace_restored_tab(&mut self, index: usize, mut tab: WorkspaceTab) -> bool {
        let Some(slot) = self.tabs.get_mut(index) else {
            return false;
        };
        // A background load may have started before the user adjusted zoom.
        for view in tab.layout.views_mut() {
            if let Some(current) = slot.layout.view(view.id) {
                view.set_zoom_factor(current.zoom_factor());
            }
        }
        let removed = std::mem::replace(slot, tab);
        self.remove_tab_buffers(&removed);
        self.dedupe_duplicate_path_owners();
        self.rebuild_buffer_tab_index();
        index < self.tabs.len()
    }

    pub fn rebuild_buffer_tab_index(&mut self) {
        self.path_index_generation = next_path_index_generation();
        self.buffer_tab_index.clear();
        self.path_tab_index.clear();
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            index_tab(
                &mut self.buffer_tab_index,
                &mut self.path_tab_index,
                tab_index,
                tab,
            );
        }
    }

    pub(super) fn index_tab_buffers(&mut self, tab_index: usize) {
        self.path_index_generation = next_path_index_generation();
        if let Some(tab) = self.tabs.get(tab_index) {
            index_tab(
                &mut self.buffer_tab_index,
                &mut self.path_tab_index,
                tab_index,
                tab,
            );
        }
    }

    pub(super) fn remove_tab_buffers(&mut self, tab: &WorkspaceTab) {
        self.path_index_generation = next_path_index_generation();
        for buffer in tab.buffers() {
            self.buffer_tab_index.remove(&buffer.id);
            if let Some(key) = buffer.path_key.as_ref() {
                self.path_tab_index.remove(key);
            }
        }
    }

    pub(super) fn shift_buffer_tab_indices(&mut self, start_index: usize, delta: isize) {
        for tab_index in self.buffer_tab_index.values_mut() {
            *tab_index = shifted_tab_index(*tab_index, start_index.., delta);
        }
    }

    pub(super) fn shift_path_tab_indices(&mut self, start_index: usize, delta: isize) {
        for (_, tab_index, _) in self.path_tab_index.values_mut() {
            *tab_index = shifted_tab_index(*tab_index, start_index.., delta);
        }
    }

    pub(super) fn remap_tab_indices_after_reorder(&mut self, from_index: usize, to_index: usize) {
        for tab_index in self.buffer_tab_index.values_mut() {
            *tab_index = reordered_tab_index(*tab_index, from_index, to_index);
        }
        for (_, tab_index, _) in self.path_tab_index.values_mut() {
            *tab_index = reordered_tab_index(*tab_index, from_index, to_index);
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
        let confirms = |owner: &(BufferId, usize, ViewId)| {
            self.tabs
                .get(owner.1)
                .and_then(|tab| tab.buffer_by_id(owner.0))
                .and_then(|buffer| buffer.path_key.as_ref())
                .is_some_and(|stored| stored.matches(key))
        };
        self.path_tab_index
            .get(key)
            .filter(|owner| confirms(owner))
            .copied()
            .or_else(|| {
                self.path_tab_index
                    .values()
                    .find(|owner| confirms(owner))
                    .copied()
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

/// Records the buffers and file paths `tab` owns at `tab_index`.
fn index_tab(
    buffer_tab_index: &mut HashMap<BufferId, usize>,
    path_tab_index: &mut HashMap<CanonicalPathKey, (BufferId, usize, ViewId)>,
    tab_index: usize,
    tab: &WorkspaceTab,
) {
    for buffer in tab.buffers() {
        buffer_tab_index.insert(buffer.id, tab_index);
    }
    for (key, owner) in tab_path_entries(tab_index, tab) {
        let previous = path_tab_index.insert(key.clone(), owner);
        debug_assert!(
            previous.is_none() || previous == Some(owner),
            "duplicate CanonicalPathKey indexed for {}",
            key.as_path().display()
        );
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

fn reordered_tab_index(tab_index: usize, from_index: usize, to_index: usize) -> usize {
    if tab_index == from_index {
        to_index
    } else if from_index < to_index && (from_index + 1..=to_index).contains(&tab_index) {
        tab_index - 1
    } else if to_index < from_index && (to_index..from_index).contains(&tab_index) {
        tab_index + 1
    } else {
        tab_index
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
