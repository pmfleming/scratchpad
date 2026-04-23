mod model;
mod ops;

use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, DocumentSnapshot, EditorViewState, EncodingSource,
    PaneNode, RestoredBufferState, TextFormatMetadata, WorkspaceTab,
};
use crate::app::services::file_service::FileService;
use crate::app::services::settings_store::AppSettings;
use crate::app::services::store_io::{remove_file_if_exists, write_atomic};
use model::{SessionBuffer, SessionManifest, SessionPaneNode, SessionTab, SessionView};
use ops::{BUFFER_FILE_EXTENSION, collect_stale_buffer_files};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SESSION_DIR_NAME: &str = "scratchpad";
const SESSION_MANIFEST_NAME: &str = "session.json";

pub use model::SESSION_VERSION;

#[derive(Clone)]
pub struct SessionStore {
    root: PathBuf,
    manifest_path: PathBuf,
}

pub(crate) struct SessionPersistRequest {
    active_tab_index: usize,
    font_size: f32,
    word_wrap: bool,
    tabs: Vec<CapturedSessionTab>,
}

struct CapturedSessionTab {
    session_tab: SessionTab,
    buffer_snapshots: Vec<CapturedSessionBuffer>,
}

struct CapturedSessionBuffer {
    temp_id: String,
    snapshot: DocumentSnapshot,
}

pub struct RestoredSession {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub legacy_settings: AppSettings,
    pub restore_status: Option<RestoreStatus>,
}

#[derive(Clone)]
pub struct RestoreStatus {
    pub level: RestoreStatusLevel,
    pub message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RestoreStatusLevel {
    Info,
    Warning,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join(SESSION_DIR_NAME))
    }
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        let manifest_path = root.join(SESSION_MANIFEST_NAME);
        Self {
            root,
            manifest_path,
        }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn load(&self) -> io::Result<Option<RestoredSession>> {
        let Some(manifest) = self.load_manifest()? else {
            return Ok(None);
        };
        let legacy_settings = manifest.legacy_settings();

        let mut tabs = Vec::with_capacity(manifest.tabs.len());
        let mut restore_summary = RestoreSummary::default();
        for tab in manifest.tabs {
            tabs.push(self.restore_tab(tab, &mut restore_summary));
        }

        if tabs.is_empty() {
            return Ok(None);
        }

        Ok(Some(RestoredSession {
            active_tab_index: manifest.active_tab_index.min(tabs.len() - 1),
            tabs,
            legacy_settings,
            restore_status: restore_summary.into_status(),
        }))
    }

    pub fn persist(
        &self,
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
    ) -> io::Result<()> {
        self.persist_request(SessionPersistRequest::capture(
            tabs,
            active_tab_index,
            font_size,
            word_wrap,
        ))
    }

    pub(crate) fn persist_request(&self, request: SessionPersistRequest) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;

        let mut active_temp_paths = HashSet::new();
        let mut session_tabs = Vec::with_capacity(request.tabs.len());

        for captured_tab in request.tabs {
            for buffer in captured_tab.buffer_snapshots {
                let temp_path = self.buffer_path(&buffer.temp_id);
                FileService::write_snapshot_utf8(&temp_path, &buffer.snapshot)?;
                active_temp_paths.insert(temp_path);
            }
            session_tabs.push(captured_tab.session_tab);
        }

        self.remove_stale_buffer_files(&active_temp_paths)?;

        let manifest = SessionManifest {
            version: SESSION_VERSION,
            active_tab_index: request
                .active_tab_index
                .min(session_tabs.len().saturating_sub(1)),
            font_size: request.font_size,
            word_wrap: request.word_wrap,
            tabs: session_tabs,
        };
        let json = serde_json::to_vec_pretty(&manifest).map_err(invalid_data)?;
        write_atomic(&self.manifest_path, &json)
    }

    fn remove_stale_buffer_files(&self, active_temp_paths: &HashSet<PathBuf>) -> io::Result<()> {
        let stale_paths =
            collect_stale_buffer_files(&self.root, &self.manifest_path, active_temp_paths)?;

        for path in stale_paths {
            remove_file_if_exists(&path)?;
        }

        Ok(())
    }

    fn buffer_path(&self, temp_id: &str) -> PathBuf {
        self.root.join(format!("{temp_id}.{BUFFER_FILE_EXTENSION}"))
    }

    fn load_manifest(&self) -> io::Result<Option<SessionManifest>> {
        if !self.manifest_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&self.manifest_path)?;
        let manifest: SessionManifest = serde_json::from_str(&raw).map_err(invalid_data)?;

        if manifest.version != model::SESSION_VERSION {
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    fn restore_tab(&self, tab: SessionTab, summary: &mut RestoreSummary) -> WorkspaceTab {
        let mut buffers = self.restore_buffers(&tab, summary);
        let control_chars_allowed = buffers
            .iter()
            .any(|buffer| buffer.artifact_summary.has_control_chars());
        let views = tab
            .views
            .into_iter()
            .map(|view| {
                EditorViewState::restored(
                    view.id,
                    view.buffer_id,
                    view.show_line_numbers,
                    view.show_control_chars && control_chars_allowed,
                )
            })
            .collect::<Vec<_>>();
        let root_pane = PaneNode::from(tab.root_pane);
        let active_view_id = if root_pane.contains_view(tab.active_view_id) {
            tab.active_view_id
        } else {
            root_pane.first_view_id()
        };
        let active_buffer_id = views
            .iter()
            .find(|view| view.id == active_view_id)
            .map(|view| view.buffer_id)
            .or_else(|| buffers.first().map(|buffer| buffer.id))
            .expect("restored workspace should contain at least one buffer");
        let active_buffer_index = buffers
            .iter()
            .position(|buffer| buffer.id == active_buffer_id)
            .unwrap_or(0);
        let active_buffer = buffers.remove(active_buffer_index);
        WorkspaceTab::restored_with_buffers(
            active_buffer,
            buffers,
            views,
            root_pane,
            active_view_id,
        )
    }

    fn restore_buffers(&self, tab: &SessionTab, summary: &mut RestoreSummary) -> Vec<BufferState> {
        let session_buffers = if tab.buffers.is_empty() {
            tab.buffer_id
                .zip(tab.name.clone())
                .zip(tab.is_dirty)
                .zip(tab.temp_id.clone())
                .zip(tab.encoding.clone())
                .zip(tab.has_bom)
                .map(
                    |(((((buffer_id, name), is_dirty), temp_id), encoding), has_bom)| {
                        vec![SessionBuffer {
                            id: buffer_id,
                            name,
                            path: tab.path.clone(),
                            is_dirty,
                            is_settings_file: false,
                            temp_id,
                            format: None,
                            encoding,
                            has_bom,
                            disk_modified_millis: None,
                            disk_len: None,
                        }]
                    },
                )
                .unwrap_or_default()
        } else {
            tab.buffers.clone()
        };

        session_buffers
            .into_iter()
            .map(|buffer| {
                let restored = self.restore_buffer_content(&buffer);
                if !buffer.is_dirty
                    && restored.freshness == BufferFreshness::InSync
                    && restored.disk_state.is_some()
                    && restored.disk_state != session_disk_state(&buffer)
                {
                    summary.reloaded_clean_buffers += 1;
                }
                summary.record(restored.freshness);
                let mut restored_buffer = BufferState::restored(RestoredBufferState {
                    id: buffer.id,
                    name: buffer.name,
                    content: restored.content,
                    path: buffer.path,
                    is_dirty: buffer.is_dirty,
                    temp_id: buffer.temp_id,
                    format: restored.format,
                    disk_state: restored.disk_state,
                    freshness: restored.freshness,
                });
                restored_buffer.is_settings_file = buffer.is_settings_file;
                restored_buffer
            })
            .collect()
    }

    fn restore_buffer_content(&self, buffer: &SessionBuffer) -> RestoredBufferContent {
        let session_disk_state = session_disk_state(buffer);
        let session_text = fs::read_to_string(self.buffer_path(&buffer.temp_id)).ok();

        match (&buffer.path, session_text) {
            (Some(path), Some(content)) => {
                self.restore_saved_buffer(buffer, path, content, session_disk_state)
            }
            (Some(path), None) => self.restore_buffer_from_disk(buffer, path),
            (None, Some(content)) => {
                RestoredBufferContent::from_session(buffer, content, None, BufferFreshness::InSync)
            }
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
            Some(disk_state) => RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                BufferFreshness::InSync,
            ),
            None => RestoredBufferContent::from_session(
                buffer,
                content,
                None,
                BufferFreshness::MissingOnDisk,
            ),
        }
    }

    fn restore_changed_disk_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        if buffer.is_dirty {
            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                BufferFreshness::ConflictOnDisk,
            );
        }

        match FileService::read_file(path) {
            Ok(file_content) => RestoredBufferContent::new(
                file_content.content,
                file_content.format,
                Some(disk_state),
                BufferFreshness::InSync,
            ),
            Err(_) => RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
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
            Ok(file_content) => RestoredBufferContent::new(
                file_content.content,
                file_content.format,
                FileService::read_disk_state(path).ok(),
                BufferFreshness::InSync,
            ),
            Err(_) => RestoredBufferContent::empty(buffer, BufferFreshness::MissingOnDisk),
        }
    }
}

impl SessionPersistRequest {
    pub(crate) fn capture(
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
    ) -> Self {
        Self {
            active_tab_index,
            font_size,
            word_wrap,
            tabs: tabs.iter().map(CapturedSessionTab::capture).collect(),
        }
    }
}

impl CapturedSessionTab {
    fn capture(tab: &WorkspaceTab) -> Self {
        Self {
            session_tab: SessionTab {
                buffers: tab.buffers().map(SessionBuffer::from).collect(),
                buffer_id: None,
                name: None,
                path: None,
                is_dirty: None,
                temp_id: None,
                encoding: None,
                has_bom: None,
                active_view_id: tab.active_view_id,
                views: tab.views.iter().map(SessionView::from).collect(),
                root_pane: SessionPaneNode::from(&tab.root_pane),
            },
            buffer_snapshots: tab
                .buffers()
                .map(|buffer| CapturedSessionBuffer {
                    temp_id: buffer.temp_id.clone(),
                    snapshot: buffer.document_snapshot(),
                })
                .collect(),
        }
    }
}

#[derive(Default)]
struct RestoreSummary {
    reloaded_clean_buffers: usize,
    conflicted_buffers: usize,
    missing_buffers: usize,
}

impl RestoreSummary {
    fn record(&mut self, freshness: BufferFreshness) {
        match freshness {
            BufferFreshness::InSync | BufferFreshness::StaleOnDisk => {}
            BufferFreshness::ConflictOnDisk => self.conflicted_buffers += 1,
            BufferFreshness::MissingOnDisk => self.missing_buffers += 1,
        }
    }

    fn into_status(self) -> Option<RestoreStatus> {
        if self.conflicted_buffers > 0 || self.missing_buffers > 0 {
            return Some(RestoreStatus {
                level: RestoreStatusLevel::Warning,
                message: format!(
                    "Session restored with {} disk conflict(s) and {} missing file(s).",
                    self.conflicted_buffers, self.missing_buffers
                ),
            });
        }

        if self.reloaded_clean_buffers > 0 {
            return Some(RestoreStatus {
                level: RestoreStatusLevel::Info,
                message: format!(
                    "Reloaded {} clean file(s) from disk during session restore.",
                    self.reloaded_clean_buffers
                ),
            });
        }

        None
    }
}

struct RestoredBufferContent {
    content: String,
    format: TextFormatMetadata,
    disk_state: Option<DiskFileState>,
    freshness: BufferFreshness,
}

impl RestoredBufferContent {
    fn new(
        content: String,
        format: TextFormatMetadata,
        disk_state: Option<DiskFileState>,
        freshness: BufferFreshness,
    ) -> Self {
        Self {
            content,
            format,
            disk_state,
            freshness,
        }
    }

    fn from_session(
        buffer: &SessionBuffer,
        content: String,
        disk_state: Option<DiskFileState>,
        freshness: BufferFreshness,
    ) -> Self {
        let format = session_buffer_format(buffer, &content);
        Self::new(content, format, disk_state, freshness)
    }

    fn empty(buffer: &SessionBuffer, freshness: BufferFreshness) -> Self {
        Self::from_session(buffer, String::new(), None, freshness)
    }
}

fn session_buffer_format(buffer: &SessionBuffer, content: &str) -> TextFormatMetadata {
    if let Some(mut format) = buffer.format.clone() {
        format.refresh_from_text(content);
        return format;
    }

    TextFormatMetadata::detected(
        content,
        buffer.encoding.clone(),
        buffer.has_bom,
        if buffer.has_bom {
            EncodingSource::Bom
        } else {
            EncodingSource::Heuristic
        },
        false,
    )
}

fn session_disk_state(buffer: &SessionBuffer) -> Option<DiskFileState> {
    Some(DiskFileState {
        modified_millis: buffer.disk_modified_millis,
        len: buffer.disk_len?,
    })
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{SessionPersistRequest, SessionStore};
    use crate::app::domain::{BufferState, WorkspaceTab};

    #[test]
    fn persist_request_uses_captured_snapshot_content() {
        let tempdir = tempfile::tempdir().expect("create session dir");
        let store = SessionStore::new(tempdir.path().to_path_buf());
        let mut tab = WorkspaceTab::new(BufferState::new(
            "notes.txt".to_owned(),
            "before".to_owned(),
            None,
        ));
        let temp_id = tab.active_buffer().temp_id.clone();
        let request = SessionPersistRequest::capture(&[tab.clone()], 0, 14.0, true);

        tab.active_buffer_mut().replace_text("after".to_owned());
        store
            .persist_request(request)
            .expect("persist captured session");

        let persisted = std::fs::read_to_string(store.buffer_path(&temp_id))
            .expect("read persisted buffer payload");
        assert_eq!(persisted, "before");
    }
}
