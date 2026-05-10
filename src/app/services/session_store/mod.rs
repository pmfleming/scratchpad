mod model;
mod ops;
mod restore;

use crate::app::diagnostics;
use crate::app::domain::{DocumentSnapshot, WorkspaceTab};
use crate::app::services::file_service::FileService;
use crate::app::services::settings_store::AppSettings;
use crate::app::services::store_io::{remove_file_if_exists, write_atomic};
use model::{SessionBuffer, SessionManifest, SessionPaneNode, SessionTab, SessionView};
use ops::{BUFFER_FILE_EXTENSION, collect_stale_buffer_files};
use restore::RestoreSummary;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

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
        fs::create_dir_all(&self.root).inspect_err(|error| {
            diagnostics::record_io_error(
                "session_create_root",
                Some(&self.root),
                "session_store::persist_request",
                &error,
            );
        })?;

        let mut active_temp_paths = HashSet::new();
        let mut session_tabs = Vec::with_capacity(request.tabs.len());

        for captured_tab in request.tabs {
            for buffer in captured_tab.buffer_snapshots {
                let temp_path = self.buffer_path(&buffer.temp_id);
                FileService::write_snapshot_utf8(&temp_path, &buffer.snapshot).inspect_err(
                    |error| {
                        diagnostics::record_io_error_with_details(
                            "session_write_buffer_snapshot",
                            Some(&temp_path),
                            "session_store::persist_request",
                            &error,
                            [("temp_id", buffer.temp_id.clone())],
                        );
                    },
                )?;
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
        let json = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            let error = invalid_data(error);
            record_session_io_error(
                "session_serialize_manifest",
                &self.manifest_path,
                "session_store::persist_request",
                &error,
            );
            error
        })?;
        write_atomic(&self.manifest_path, &json).inspect_err(|error| {
            record_session_io_error(
                "session_write_manifest",
                &self.manifest_path,
                "session_store::persist_request",
                &error,
            );
        })
    }

    fn remove_stale_buffer_files(&self, active_temp_paths: &HashSet<PathBuf>) -> io::Result<()> {
        let stale_paths =
            collect_stale_buffer_files(&self.root, &self.manifest_path, active_temp_paths)
                .inspect_err(|error| {
                    diagnostics::record_io_error(
                        "session_collect_stale_buffers",
                        Some(&self.root),
                        "session_store::remove_stale_buffer_files",
                        &error,
                    );
                })?;

        for path in stale_paths {
            remove_file_if_exists(&path).inspect_err(|error| {
                diagnostics::record_io_error(
                    "session_remove_stale_buffer",
                    Some(&path),
                    "session_store::remove_stale_buffer_files",
                    &error,
                );
            })?;
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

        let raw = fs::read_to_string(&self.manifest_path).inspect_err(|error| {
            record_session_io_error(
                "session_read_manifest",
                &self.manifest_path,
                "session_store::load_manifest",
                &error,
            );
        })?;
        let manifest: SessionManifest = serde_json::from_str(&raw).map_err(|error| {
            let error = invalid_data(error);
            record_session_io_error(
                "session_parse_manifest",
                &self.manifest_path,
                "session_store::load_manifest",
                &error,
            );
            error
        })?;

        if manifest.version != model::SESSION_VERSION {
            diagnostics::record_warning(
                "session_version_mismatch",
                Some(&self.manifest_path),
                "session_store::load_manifest",
                format!(
                    "Session manifest version {} is not supported by version {}.",
                    manifest.version,
                    model::SESSION_VERSION
                ),
            );
            return Ok(None);
        }

        Ok(Some(manifest))
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
                .filter(|buffer| buffer.is_dirty || buffer.path.is_none())
                .map(|buffer| CapturedSessionBuffer {
                    temp_id: buffer.temp_id.clone(),
                    snapshot: buffer.document_snapshot(),
                })
                .collect(),
        }
    }
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn record_session_io_error(
    operation: &'static str,
    path: &std::path::Path,
    scope: &'static str,
    error: &io::Error,
) {
    diagnostics::record_io_error(operation, Some(path), scope, error);
}
