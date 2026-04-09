mod model;
mod ops;

use crate::app::domain::{
    BufferState, EditorViewState, PaneNode, RestoredBufferState, WorkspaceTab,
};
use crate::app::services::file_service::FileService;
use crate::app::services::settings_store::AppSettings;
use model::{SessionBuffer, SessionManifest, SessionPaneNode, SessionTab, SessionView};
use ops::{BUFFER_FILE_EXTENSION, collect_stale_buffer_files, write_atomic};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

const SESSION_DIR_NAME: &str = "scratchpad";
const SESSION_MANIFEST_NAME: &str = "session.json";

pub use model::SESSION_VERSION;

pub struct SessionStore {
    root: PathBuf,
    manifest_path: PathBuf,
}

pub struct RestoredSession {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub legacy_settings: AppSettings,
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
        for tab in manifest.tabs {
            tabs.push(self.restore_tab(tab));
        }

        if tabs.is_empty() {
            return Ok(None);
        }

        Ok(Some(RestoredSession {
            active_tab_index: manifest.active_tab_index.min(tabs.len() - 1),
            tabs,
            legacy_settings,
        }))
    }

    pub fn persist(
        &self,
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
        logging_enabled: bool,
    ) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;

        let mut active_temp_paths = HashSet::with_capacity(tabs.len());
        let session_tabs = tabs
            .iter()
            .map(|tab| {
                for buffer in tab.buffers() {
                    let temp_path = self.buffer_path(&buffer.temp_id);
                    write_atomic(&temp_path, buffer.content.as_bytes())?;
                    active_temp_paths.insert(temp_path);
                }

                Ok(SessionTab {
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
                })
            })
            .collect::<io::Result<Vec<_>>>()?;

        self.remove_stale_buffer_files(&active_temp_paths)?;

        let manifest = SessionManifest {
            version: SESSION_VERSION,
            active_tab_index: active_tab_index.min(tabs.len().saturating_sub(1)),
            font_size,
            word_wrap,
            logging_enabled,
            tabs: session_tabs,
        };
        let json = serde_json::to_vec_pretty(&manifest).map_err(invalid_data)?;
        write_atomic(&self.manifest_path, &json)
    }

    fn remove_stale_buffer_files(&self, active_temp_paths: &HashSet<PathBuf>) -> io::Result<()> {
        let stale_paths =
            collect_stale_buffer_files(&self.root, &self.manifest_path, active_temp_paths)?;

        for path in stale_paths {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
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

    fn restore_tab(&self, tab: SessionTab) -> WorkspaceTab {
        let mut buffers = self.restore_buffers(&tab);
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

    fn restore_buffers(&self, tab: &SessionTab) -> Vec<BufferState> {
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
                            temp_id,
                            encoding,
                            has_bom,
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
                let (content, encoding, has_bom) = self.restore_buffer_content(&buffer);
                BufferState::restored(RestoredBufferState {
                    id: buffer.id,
                    name: buffer.name,
                    content,
                    path: buffer.path,
                    is_dirty: buffer.is_dirty,
                    temp_id: buffer.temp_id,
                    encoding,
                    has_bom,
                })
            })
            .collect()
    }

    fn restore_buffer_content(&self, buffer: &SessionBuffer) -> (String, String, bool) {
        if let Ok(content) = fs::read_to_string(self.buffer_path(&buffer.temp_id)) {
            return (content, buffer.encoding.clone(), buffer.has_bom);
        }

        match &buffer.path {
            Some(path) => match FileService::read_file(path) {
                Ok(file_content) => (
                    file_content.content,
                    file_content.encoding,
                    file_content.has_bom,
                ),
                Err(_) => (String::new(), buffer.encoding.clone(), buffer.has_bom),
            },
            None => (String::new(), buffer.encoding.clone(), buffer.has_bom),
        }
    }
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
