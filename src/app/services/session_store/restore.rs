use super::model::{SessionBufferPayload, SessionTab, SessionTabParts};
use super::parallel;
use super::{RestoreStatus, RestoreStatusLevel};
use crate::app::domain::{BufferFreshness, BufferState, RestoredBufferState, WorkspaceTab};
use crate::app::utils::pluralize;
use std::collections::HashMap;
use std::io;

mod buffer_content;
mod tab_build;
use buffer_content::{cold_buffer_shell, session_disk_state};
use tab_build::workspace_tab_from_restored_buffers;
#[cfg(test)]
mod tests;

/// An owned, coherent database read. Reconstruction never performs database I/O.
pub(super) struct Restorer {
    contents: HashMap<String, String>,
}

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
            BufferFreshness::ConflictOnDisk => self.conflicted_buffers += 1,
            BufferFreshness::MissingOnDisk => self.missing_buffers += 1,
            _ => {}
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
        (self.reloaded_clean_buffers > 0).then(|| RestoreStatus {
            level: RestoreStatusLevel::Info,
            message: format!(
                "Reloaded {} from disk during session restore.",
                pluralize(self.reloaded_clean_buffers, "clean file")
            ),
        })
    }
}
impl Restorer {
    pub(super) fn new(contents: HashMap<String, String>) -> Self {
        Self { contents }
    }
    pub(super) fn restore_tabs_ordered(
        &mut self,
        tabs: Vec<SessionTab>,
    ) -> io::Result<Vec<(WorkspaceTab, RestoreSummary)>> {
        let workers = parallel::worker_count(16);
        if tabs.len() < 512 || workers == 1 {
            return Ok(self.restore_sequential(tabs));
        }
        let total = tabs.len();
        let chunk_size = total.div_ceil(workers);
        let mut tabs = tabs.into_iter();
        let jobs = (0..workers).map(|_| {
            let chunk = tabs.by_ref().take(chunk_size).collect::<Vec<_>>();
            let contents = chunk
                .iter()
                .flat_map(|tab| &tab.buffers)
                .filter_map(|buffer| self.contents.remove_entry(&buffer.temp_id))
                .collect();
            (chunk, contents)
        });
        let batches = parallel::ordered("session-reconstruct", jobs, |(tabs, contents)| {
            Ok(Restorer::new(contents).restore_sequential(tabs))
        })?;
        let mut restored = Vec::with_capacity(total);
        for batch in batches {
            restored.extend(batch);
        }
        Ok(restored)
    }
    fn restore_sequential(&mut self, tabs: Vec<SessionTab>) -> Vec<(WorkspaceTab, RestoreSummary)> {
        tabs.into_iter()
            .map(|tab| self.restore_tab_with_summary(tab))
            .collect()
    }
    pub(super) fn restore_tabs_active_first(
        &mut self,
        tabs: Vec<SessionTab>,
        active: usize,
        quarantined: &std::collections::HashSet<String>,
    ) -> Vec<(usize, WorkspaceTab, Option<SessionTabParts>, RestoreSummary)> {
        if tabs.is_empty() {
            return Vec::new();
        }
        let active = active.min(tabs.len() - 1);
        let mut tabs = tabs
            .into_iter()
            .map(SessionTab::into_parts)
            .enumerate()
            .collect::<Vec<_>>();
        tabs.rotate_left(active);
        tabs.into_iter()
            .map(|(index, tab)| {
                if index == active
                    && !tab
                        .payload
                        .buffers
                        .iter()
                        .any(|buffer| quarantined.contains(&buffer.temp_id))
                {
                    let (tab, summary) = self.restore_tab_parts_with_summary(tab);
                    (index, tab, None, summary)
                } else {
                    (
                        index,
                        Self::cold_tab_shell(&tab),
                        Some(tab),
                        RestoreSummary::default(),
                    )
                }
            })
            .collect()
    }
    pub(super) fn restore_tab_with_summary(
        &mut self,
        tab: SessionTab,
    ) -> (WorkspaceTab, RestoreSummary) {
        self.restore_tab_parts_with_summary(tab.into_parts())
    }
    fn restore_tab_parts_with_summary(
        &mut self,
        tab: SessionTabParts,
    ) -> (WorkspaceTab, RestoreSummary) {
        let mut summary = RestoreSummary::default();
        let mut buffers = self.restore_buffers(tab.payload, &mut summary);
        (
            workspace_tab_from_restored_buffers(tab.shell, &mut buffers),
            summary,
        )
    }
    fn cold_tab_shell(tab: &SessionTabParts) -> WorkspaceTab {
        let mut buffers = tab.payload.buffers.iter().map(cold_buffer_shell).collect();
        workspace_tab_from_restored_buffers(tab.shell.clone(), &mut buffers)
    }
    fn restore_buffers(
        &mut self,
        payload: SessionBufferPayload,
        summary: &mut RestoreSummary,
    ) -> Vec<BufferState> {
        payload
            .buffers
            .into_iter()
            .map(|mut buffer| {
                let mut restored = self.restore_buffer_content(&mut buffer);
                // Clean disk-backed buffers have history metadata but no recovery text.
                if !buffer.text_history.is_empty() {
                    restored
                        .document
                        .restore_exported_history(std::mem::take(&mut buffer.text_history));
                }
                if !buffer.is_dirty
                    && restored.freshness == BufferFreshness::InSync
                    && restored.disk_state.is_some()
                    && restored.disk_state != session_disk_state(&buffer)
                {
                    summary.reloaded_clean_buffers += 1;
                }
                summary.record(restored.freshness);
                let mut state = BufferState::restored_with_document_text_metadata(
                    RestoredBufferState {
                        id: buffer.id,
                        name: buffer.name,
                        content: String::new(),
                        path: buffer.path,
                        is_dirty: restored.is_dirty,
                        temp_id: buffer.temp_id,
                        format: restored.format,
                        disk_state: restored.disk_state,
                        freshness: restored.freshness,
                        show_control_chars: buffer.show_control_chars,
                        right_to_left_reading_order: buffer.right_to_left_reading_order,
                    },
                    restored.document,
                    restored.text_metadata,
                );
                state.is_settings_file = buffer.is_settings_file;
                state
            })
            .collect()
    }
}
