use super::model::{SessionBufferPayload, SessionTab, SessionTabParts};
use super::{
    RestoreStatus, RestoreStatusLevel, SESSION_IO_PARALLEL_MAX_WORKERS,
    SESSION_IO_PARALLEL_MIN_ITEMS, SessionStore,
};
use crate::app::domain::{BufferFreshness, BufferState, RestoredBufferState, WorkspaceTab};
use crate::app::utils::pluralize;
use std::thread;

mod buffer_content;
mod tab_build;
use buffer_content::{cold_buffer_shell, session_disk_state};
use tab_build::workspace_tab_from_restored_buffers;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct RestoreSummary {
    pub(super) reloaded_clean_buffers: usize,
    conflicted_buffers: usize,
    missing_buffers: usize,
}

impl RestoreSummary {
    pub(super) fn merge(&mut self, other: Self) {
        self.reloaded_clean_buffers += other.reloaded_clean_buffers;
        self.conflicted_buffers += other.conflicted_buffers;
        self.missing_buffers += other.missing_buffers;
    }

    pub(super) fn record(&mut self, freshness: BufferFreshness) {
        match freshness {
            BufferFreshness::InSync
            | BufferFreshness::AutoReloaded
            | BufferFreshness::StaleOnDisk => {}
            BufferFreshness::ConflictOnDisk => self.conflicted_buffers += 1,
            BufferFreshness::MissingOnDisk => self.missing_buffers += 1,
        }
    }

    pub(super) fn into_status(self) -> Option<RestoreStatus> {
        if self.conflicted_buffers > 0 || self.missing_buffers > 0 {
            return Some(RestoreStatus {
                level: RestoreStatusLevel::Warning,
                message: format!(
                    "Session restore found {} and {}.",
                    pluralize(self.conflicted_buffers, "disk conflict"),
                    pluralize(self.missing_buffers, "missing file")
                ),
            });
        }

        if self.reloaded_clean_buffers > 0 {
            return Some(RestoreStatus {
                level: RestoreStatusLevel::Info,
                message: format!(
                    "Reloaded {} from disk during session restore.",
                    pluralize(self.reloaded_clean_buffers, "clean file")
                ),
            });
        }

        None
    }
}

impl SessionStore {
    pub(super) fn restore_tabs_ordered(
        &self,
        tabs: Vec<SessionTab>,
    ) -> Vec<(WorkspaceTab, RestoreSummary)> {
        let total = tabs.len();
        if total < SESSION_IO_PARALLEL_MIN_ITEMS || restore_worker_count(total) <= 1 {
            return self.restore_tabs_ordered_sequential(tabs);
        }

        self.restore_tabs_ordered_parallel(tabs, total)
    }

    fn restore_tabs_ordered_sequential(
        &self,
        tabs: Vec<SessionTab>,
    ) -> Vec<(WorkspaceTab, RestoreSummary)> {
        tabs.into_iter()
            .map(|tab| self.restore_tab_parts_with_summary(tab.into_parts()))
            .collect()
    }

    fn restore_tabs_ordered_parallel(
        &self,
        tabs: Vec<SessionTab>,
        total: usize,
    ) -> Vec<(WorkspaceTab, RestoreSummary)> {
        let workers = restore_worker_count(total);
        let chunk_size = total.div_ceil(workers);
        let mut iter = tabs.into_iter().enumerate();
        let mut restored = Vec::with_capacity(total);

        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                let chunk = iter.by_ref().take(chunk_size).collect::<Vec<_>>();
                if chunk.is_empty() {
                    break;
                }
                handles.push(scope.spawn(move || {
                    let mut restored = Vec::with_capacity(chunk.len());
                    for (index, tab) in chunk {
                        let (restored_tab, summary) =
                            self.restore_tab_parts_with_summary(tab.into_parts());
                        restored.push((index, restored_tab, summary));
                    }
                    restored
                }));
            }

            for handle in handles {
                restored.extend(handle.join().expect("session restore worker panicked"));
            }
        });

        restored.sort_by_key(|(index, _, _)| *index);
        restored
            .into_iter()
            .map(|(_, tab, summary)| (tab, summary))
            .collect()
    }

    pub(super) fn restore_tabs_active_first(
        &self,
        tabs: Vec<SessionTab>,
        active_tab_index: usize,
    ) -> Vec<(usize, WorkspaceTab, Option<SessionTabParts>, RestoreSummary)> {
        if tabs.is_empty() {
            return Vec::new();
        }

        let active_tab_index = active_tab_index.min(tabs.len() - 1);
        let mut indexed_tabs = tabs
            .into_iter()
            .map(SessionTab::into_parts)
            .enumerate()
            .collect::<Vec<_>>();
        indexed_tabs.rotate_left(active_tab_index);
        indexed_tabs
            .into_iter()
            .map(|(index, tab)| {
                if index == active_tab_index {
                    let (restored_tab, summary) = self.restore_tab_parts_with_summary(tab);
                    (index, restored_tab, None, summary)
                } else {
                    let shell = self.cold_tab_shell(&tab);
                    (index, shell, Some(tab), RestoreSummary::default())
                }
            })
            .collect()
    }

    pub(super) fn restore_tab_with_summary(
        &self,
        tab: SessionTab,
    ) -> (WorkspaceTab, RestoreSummary) {
        self.restore_tab_parts_with_summary(tab.into_parts())
    }

    fn restore_tab_parts_with_summary(
        &self,
        tab: SessionTabParts,
    ) -> (WorkspaceTab, RestoreSummary) {
        let mut summary = RestoreSummary::default();
        let tab = self.restore_tab_parts(tab, &mut summary);
        (tab, summary)
    }

    fn restore_tab_parts(
        &self,
        tab: SessionTabParts,
        summary: &mut RestoreSummary,
    ) -> WorkspaceTab {
        let SessionTabParts { shell, payload } = tab;
        let mut buffers = self.restore_buffers(&payload, summary);
        workspace_tab_from_restored_buffers(shell, &mut buffers)
    }

    pub(crate) fn restore_cold_session_tab(
        &self,
        tab: SessionTabParts,
    ) -> (WorkspaceTab, Option<RestoreStatus>) {
        let mut summary = RestoreSummary::default();
        let tab = self.restore_tab_parts(tab, &mut summary);
        (tab, summary.into_status())
    }

    pub(crate) fn cold_tab_shell(&self, tab: &SessionTabParts) -> WorkspaceTab {
        let mut buffers = tab
            .payload
            .buffers
            .iter()
            .cloned()
            .map(cold_buffer_shell)
            .collect::<Vec<_>>();
        workspace_tab_from_restored_buffers(tab.shell.clone(), &mut buffers)
    }

    fn restore_buffers(
        &self,
        payload: &SessionBufferPayload,
        summary: &mut RestoreSummary,
    ) -> Vec<BufferState> {
        payload
            .buffers
            .iter()
            .map(|buffer| {
                let restored = self.restore_buffer_content(buffer);
                if !buffer.is_dirty
                    && restored.freshness == BufferFreshness::InSync
                    && restored.disk_state.is_some()
                    && restored.disk_state != session_disk_state(buffer)
                {
                    summary.reloaded_clean_buffers += 1;
                }
                summary.record(restored.freshness);
                let mut restored_buffer = BufferState::restored_with_document_text_metadata(
                    RestoredBufferState {
                        id: buffer.id,
                        name: buffer.name.clone(),
                        content: String::new(),
                        path: buffer.path.clone(),
                        is_dirty: restored.is_dirty,
                        temp_id: buffer.temp_id.clone(),
                        format: restored.format,
                        disk_state: restored.disk_state,
                        freshness: restored.freshness,
                        show_control_chars: buffer.show_control_chars,
                        right_to_left_reading_order: buffer.right_to_left_reading_order,
                    },
                    restored.document,
                    restored.text_metadata,
                );
                restored_buffer.is_settings_file = buffer.is_settings_file;
                restored_buffer
            })
            .collect()
    }
}

fn restore_worker_count(item_count: usize) -> usize {
    thread::available_parallelism()
        .map_or(1, |parallelism| {
            parallelism
                .get()
                .min(SESSION_IO_PARALLEL_MAX_WORKERS)
                .min(item_count)
        })
        .max(1)
}
