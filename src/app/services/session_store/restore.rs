use super::model::{SessionBuffer, SessionTab, SessionTabParts};
use super::parallel;
use super::{RestoreStatus, RestoreStatusLevel};
use crate::app::domain::{BufferFreshness, BufferState, RestoredBufferState, WorkspaceTab};
use crate::app::utils::pluralize;
use std::io;

mod buffer_content;
mod tab_build;
use buffer_content::{RestoredBufferContent, cold_buffer_shell};
use tab_build::workspace_tab_from_restored_buffers;
#[cfg(test)]
mod tests;

/// An owned, coherent database read. Reconstruction never performs database I/O.
pub(super) struct Restorer;
type StartupTab = (usize, WorkspaceTab, Option<SessionTabParts>, RestoreSummary);

#[derive(Default)]
pub(super) struct RestoreSummary {
    conflicted_buffers: usize,
    missing_buffers: usize,
}
impl RestoreSummary {
    pub(super) fn merge(&mut self, other: Self) {
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
        None
    }
}
impl Restorer {
    pub(super) fn restore_tabs_ordered(
        tabs: Vec<SessionTab>,
    ) -> io::Result<Vec<(WorkspaceTab, RestoreSummary)>> {
        let workers = parallel::worker_count(16);
        if tabs.len() < 512 || workers == 1 {
            return Self::restore_sequential(tabs);
        }
        let total = tabs.len();
        let chunk_size = total.div_ceil(workers);
        let mut tabs = tabs.into_iter();
        let jobs = (0..workers).map(|_| tabs.by_ref().take(chunk_size).collect::<Vec<_>>());
        let batches = parallel::ordered("session-reconstruct", jobs, Self::restore_sequential)?;
        let mut restored = Vec::with_capacity(total);
        for batch in batches {
            restored.extend(batch);
        }
        Ok(restored)
    }
    fn restore_sequential(
        tabs: Vec<SessionTab>,
    ) -> io::Result<Vec<(WorkspaceTab, RestoreSummary)>> {
        tabs.into_iter()
            .map(Self::restore_tab_with_summary)
            .collect()
    }
    pub(super) fn restore_tabs_active_first(
        tabs: Vec<SessionTab>,
        active: usize,
        quarantined: &std::collections::HashSet<String>,
    ) -> io::Result<Vec<StartupTab>> {
        if tabs.is_empty() {
            return Ok(Vec::new());
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
                        .buffers
                        .iter()
                        .any(|buffer| quarantined.contains(&buffer.temp_id))
                {
                    let (tab, summary) = Self::restore_tab_parts_with_summary(tab)?;
                    Ok((index, tab, None, summary))
                } else {
                    Ok((
                        index,
                        Self::cold_tab_shell(&tab),
                        Some(tab),
                        RestoreSummary::default(),
                    ))
                }
            })
            .collect()
    }
    pub(super) fn restore_tab_with_summary(
        tab: SessionTab,
    ) -> io::Result<(WorkspaceTab, RestoreSummary)> {
        Self::restore_tab_parts_with_summary(tab.into_parts())
    }
    fn restore_tab_parts_with_summary(
        tab: SessionTabParts,
    ) -> io::Result<(WorkspaceTab, RestoreSummary)> {
        let mut summary = RestoreSummary::default();
        let mut buffers = Self::restore_buffers(tab.buffers, &mut summary)?;
        Ok((
            workspace_tab_from_restored_buffers(tab.shell, &mut buffers),
            summary,
        ))
    }
    pub(super) fn cold_tab_shell(tab: &SessionTabParts) -> WorkspaceTab {
        let mut buffers = tab.buffers.iter().map(cold_buffer_shell).collect();
        workspace_tab_from_restored_buffers(tab.shell.clone(), &mut buffers)
    }
    fn restore_buffers(
        buffers: Vec<SessionBuffer>,
        summary: &mut RestoreSummary,
    ) -> io::Result<Vec<BufferState>> {
        buffers
            .into_iter()
            .map(|mut buffer| {
                let restored = RestoredBufferContent::from_references(&mut buffer)?;
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
                state.disk_content_digest = buffer.disk_content_digest;
                state.storage_committed_revision = Some(state.storage_revision());
                state.pending_source_check = state.path.is_some();
                state.is_settings_file = buffer.is_settings_file;
                state.observed_disk_digest = buffer.observed_disk_digest;
                state.observed_disk_missing = buffer.observed_disk_missing;
                state.pending_disk_observation = buffer.pending_disk_observation;
                Ok(state)
            })
            .collect()
    }
}
