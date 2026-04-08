mod model;
mod ops;

use crate::app::domain::{BufferState, EditorViewState, PaneNode, RestoredBufferState, WorkspaceTab};
use crate::app::services::file_service::FileService;
use model::{SessionManifest, SessionPaneNode, SessionTab, SessionView};
use ops::{collect_stale_buffer_files, write_atomic, BUFFER_FILE_EXTENSION};
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
    pub font_size: f32,
    pub word_wrap: bool,
    pub logging_enabled: bool,
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

    pub fn load(&self) -> io::Result<Option<RestoredSession>> {
        let Some(manifest) = self.load_manifest()? else {
            return Ok(None);
        };

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
            font_size: manifest.font_size,
            word_wrap: manifest.word_wrap,
            logging_enabled: manifest.logging_enabled,
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
                let buffer = &tab.buffer;
                let temp_path = self.buffer_path(&buffer.temp_id);
                write_atomic(&temp_path, buffer.content.as_bytes())?;
                active_temp_paths.insert(temp_path);

                Ok(SessionTab {
                    buffer_id: buffer.id,
                    name: buffer.name.clone(),
                    path: buffer.path.clone(),
                    is_dirty: buffer.is_dirty,
                    temp_id: buffer.temp_id.clone(),
                    encoding: buffer.encoding.clone(),
                    has_bom: buffer.has_bom,
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
        let (content, encoding, has_bom) = self.restore_tab_content(&tab);
        let buffer = BufferState::restored(RestoredBufferState {
            id: tab.buffer_id,
            name: tab.name,
            content,
            path: tab.path,
            is_dirty: tab.is_dirty,
            temp_id: tab.temp_id,
            encoding,
            has_bom,
        });
        let control_chars_allowed = buffer.artifact_summary.has_control_chars();
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
        WorkspaceTab::restored(buffer, views, root_pane, active_view_id)
    }

    fn restore_tab_content(&self, tab: &SessionTab) -> (String, String, bool) {
        if let Ok(content) = fs::read_to_string(self.buffer_path(&tab.temp_id)) {
            return (content, tab.encoding.clone(), tab.has_bom);
        }

        self.restore_from_original_path(tab)
    }

    fn restore_from_original_path(&self, tab: &SessionTab) -> (String, String, bool) {
        match &tab.path {
            Some(path) => match FileService::read_file(path) {
                Ok(file_content) => (
                    file_content.content,
                    file_content.encoding,
                    file_content.has_bom,
                ),
                Err(_) => (String::new(), tab.encoding.clone(), tab.has_bom),
            },
            None => (String::new(), tab.encoding.clone(), tab.has_bom),
        }
    }
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
