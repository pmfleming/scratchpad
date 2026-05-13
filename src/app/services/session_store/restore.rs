use super::model::{
    SessionBuffer, SessionBufferPayload, SessionTab, SessionTabParts, SessionTabShell,
};
use super::{
    RestoreStatus, RestoreStatusLevel, SESSION_IO_PARALLEL_MAX_WORKERS,
    SESSION_IO_PARALLEL_MIN_ITEMS, SessionStore,
};
use crate::app::diagnostics;
use crate::app::domain::buffer::{
    BufferTextMetadata, buffer_text_metadata, detected_text_format_and_metadata,
};
use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, EditorViewState, EncodingSource, PaneNode,
    RestoredBufferState, TextDocument, TextFormatMetadata, WorkspaceTab,
};
use crate::app::services::file_service::{FileContent, FileService};
use crate::app::utils::pluralize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::thread;

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
            return tabs
                .into_iter()
                .map(|tab| self.restore_tab_parts_with_summary(tab.into_parts()))
                .collect();
        }

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

fn workspace_tab_from_restored_buffers(
    shell: SessionTabShell,
    buffers: &mut Vec<BufferState>,
) -> WorkspaceTab {
    let root_pane = PaneNode::from(shell.root_pane);
    let active_view_id = if root_pane.contains_view(shell.active_view_id) {
        shell.active_view_id
    } else {
        root_pane.first_view_id()
    };
    let mut visible_control_char_buffer_ids = HashSet::new();
    let mut active_buffer_id = None;
    let mut views = Vec::with_capacity(shell.views.len());
    for view in shell.views {
        if view.show_control_chars {
            visible_control_char_buffer_ids.insert(view.buffer_id);
        }
        if view.id == active_view_id {
            active_buffer_id = Some(view.buffer_id);
        }
        views.push(EditorViewState::restored(
            view.id,
            view.buffer_id,
            view.show_line_numbers,
        ));
    }

    for buffer in buffers.iter_mut() {
        buffer.show_control_chars = buffer.artifact_summary.has_control_chars()
            && (buffer.show_control_chars || visible_control_char_buffer_ids.contains(&buffer.id));
    }
    let active_buffer_id = active_buffer_id
        .or_else(|| buffers.first().map(|buffer| buffer.id))
        .expect("restored workspace should contain at least one buffer");
    let active_buffer_index = buffers
        .iter()
        .position(|buffer| buffer.id == active_buffer_id)
        .unwrap_or(0);
    let active_buffer = buffers.remove(active_buffer_index);
    WorkspaceTab::restored_with_buffers(
        active_buffer,
        std::mem::take(buffers),
        views,
        root_pane,
        active_view_id,
    )
}

impl SessionStore {
    fn restore_buffer_content(&self, buffer: &SessionBuffer) -> RestoredBufferContent {
        let session_disk_state = session_disk_state(buffer);
        let session_path = self.buffer_path(&buffer.temp_id);
        let session_text = match fs::read_to_string(&session_path) {
            Ok(content) => Some(content),
            Err(error) => {
                diagnostics::record_io_error_with_details(
                    "session_read_buffer_snapshot",
                    Some(&session_path),
                    "session_store::restore_buffer_content",
                    &error,
                    [("temp_id", buffer.temp_id.clone())],
                );
                None
            }
        };

        match (&buffer.path, session_text) {
            (Some(path), Some(content)) => {
                self.restore_saved_buffer(buffer, path, content, session_disk_state)
            }
            (Some(path), None) => self.restore_buffer_from_disk(buffer, path),
            (None, Some(content)) => RestoredBufferContent::from_session(
                buffer,
                content,
                None,
                buffer.is_dirty,
                BufferFreshness::InSync,
            ),
            (None, None) => RestoredBufferContent::empty(buffer, BufferFreshness::InSync),
        }
    }

    fn restore_saved_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        session_disk_state: Option<DiskFileState>,
    ) -> RestoredBufferContent {
        match FileService::read_disk_state(path).ok() {
            Some(disk_state) if Some(disk_state.clone()) != session_disk_state => {
                self.restore_changed_disk_buffer(buffer, path, content, disk_state)
            }
            Some(disk_state) => {
                self.restore_unchanged_disk_buffer(buffer, path, content, disk_state)
            }
            None => RestoredBufferContent::from_session(
                buffer,
                content,
                None,
                buffer.is_dirty,
                BufferFreshness::MissingOnDisk,
            ),
        }
    }

    fn restore_unchanged_disk_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        if !buffer.is_dirty {
            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                false,
                BufferFreshness::InSync,
            );
        }

        if disk_text_matches(path, &content) {
            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                false,
                BufferFreshness::InSync,
            );
        }

        RestoredBufferContent::from_session(
            buffer,
            content,
            Some(disk_state),
            true,
            BufferFreshness::InSync,
        )
    }

    fn restore_changed_disk_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        if buffer.is_dirty {
            if disk_text_matches(path, &content) {
                return RestoredBufferContent::from_session(
                    buffer,
                    content,
                    Some(disk_state),
                    false,
                    BufferFreshness::InSync,
                );
            }

            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                true,
                BufferFreshness::ConflictOnDisk,
            );
        }

        match FileService::read_file(path) {
            Ok(file_content) => RestoredBufferContent::from_file_content(
                file_content,
                Some(disk_state),
                BufferFreshness::InSync,
            ),
            Err(_) => RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                false,
                BufferFreshness::StaleOnDisk,
            ),
        }
    }

    fn restore_buffer_from_disk(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
    ) -> RestoredBufferContent {
        match FileService::read_file(path) {
            Ok(file_content) => RestoredBufferContent::from_file_content(
                file_content,
                FileService::read_disk_state(path).ok(),
                BufferFreshness::InSync,
            ),
            Err(_) => RestoredBufferContent::empty(buffer, BufferFreshness::MissingOnDisk),
        }
    }
}

fn restore_worker_count(item_count: usize) -> usize {
    thread::available_parallelism()
        .map(|parallelism| {
            parallelism
                .get()
                .min(SESSION_IO_PARALLEL_MAX_WORKERS)
                .min(item_count)
        })
        .unwrap_or(1)
        .max(1)
}

struct RestoredBufferContent {
    document: TextDocument,
    format: TextFormatMetadata,
    text_metadata: BufferTextMetadata,
    disk_state: Option<DiskFileState>,
    is_dirty: bool,
    freshness: BufferFreshness,
}

impl RestoredBufferContent {
    fn new(
        document: TextDocument,
        format: TextFormatMetadata,
        text_metadata: BufferTextMetadata,
        disk_state: Option<DiskFileState>,
        is_dirty: bool,
        freshness: BufferFreshness,
    ) -> Self {
        Self {
            document,
            format,
            text_metadata,
            disk_state,
            is_dirty,
            freshness,
        }
    }

    fn from_file_content(
        file_content: FileContent,
        disk_state: Option<DiskFileState>,
        freshness: BufferFreshness,
    ) -> Self {
        Self::new(
            file_content.document,
            file_content.format,
            file_content.text_metadata,
            disk_state,
            false,
            freshness,
        )
    }

    fn from_session(
        buffer: &SessionBuffer,
        content: String,
        disk_state: Option<DiskFileState>,
        is_dirty: bool,
        freshness: BufferFreshness,
    ) -> Self {
        let (format, text_metadata) = session_buffer_format_and_metadata(buffer, &content);
        let document =
            TextDocument::with_preferred_line_ending(content, text_metadata.preferred_line_ending);
        let mut document = document;
        document.restore_exported_history(buffer.text_history.clone());
        Self::new(
            document,
            format,
            text_metadata,
            disk_state,
            is_dirty,
            freshness,
        )
    }

    fn empty(buffer: &SessionBuffer, freshness: BufferFreshness) -> Self {
        Self::from_session(buffer, String::new(), None, buffer.is_dirty, freshness)
    }
}

fn disk_text_matches(path: &Path, session_text: &str) -> bool {
    FileService::read_file(path)
        .map(|file_content| file_content.document.extract_text() == session_text)
        .unwrap_or(false)
}

fn cold_buffer_shell(buffer: SessionBuffer) -> BufferState {
    let (format, text_metadata) = session_buffer_format_and_metadata(&buffer, "");
    let disk_state = session_disk_state(&buffer);
    let mut restored_buffer = BufferState::restored_with_text_metadata(
        RestoredBufferState {
            id: buffer.id,
            name: buffer.name,
            content: String::new(),
            path: buffer.path,
            is_dirty: buffer.is_dirty,
            temp_id: buffer.temp_id,
            format,
            disk_state,
            freshness: BufferFreshness::InSync,
            show_control_chars: buffer.show_control_chars,
            right_to_left_reading_order: buffer.right_to_left_reading_order,
        },
        text_metadata,
    );
    restored_buffer.is_settings_file = buffer.is_settings_file;
    restored_buffer
}

fn session_buffer_format_and_metadata(
    buffer: &SessionBuffer,
    content: &str,
) -> (TextFormatMetadata, BufferTextMetadata) {
    let encoding_source = if buffer.has_bom {
        EncodingSource::Bom
    } else {
        EncodingSource::Heuristic
    };

    if let Some(mut format) = buffer.format.clone() {
        let text_metadata = buffer_text_metadata(content, &mut format);
        return (format, text_metadata);
    }

    detected_text_format_and_metadata(
        content,
        buffer.encoding.clone(),
        buffer.has_bom,
        encoding_source,
        false,
    )
}

fn session_disk_state(buffer: &SessionBuffer) -> Option<DiskFileState> {
    Some(DiskFileState {
        modified_millis: buffer.disk_modified_millis,
        len: buffer.disk_len?,
    })
}
