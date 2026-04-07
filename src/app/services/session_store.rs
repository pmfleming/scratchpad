use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::file_service::FileService;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SESSION_DIR_NAME: &str = "scratchpad";
const SESSION_MANIFEST_NAME: &str = "session.json";
const BUFFER_FILE_EXTENSION: &str = "tmp";
const SESSION_VERSION: u32 = 2;

pub struct SessionStore {
    root: PathBuf,
    manifest_path: PathBuf,
}

pub struct RestoredSession {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub font_size: f32,
    pub word_wrap: bool,
}

#[derive(Serialize, Deserialize)]
struct SessionManifest {
    version: u32,
    active_tab_index: usize,
    font_size: f32,
    word_wrap: bool,
    tabs: Vec<SessionTab>,
}

#[derive(Serialize, Deserialize)]
struct SessionTab {
    name: String,
    path: Option<PathBuf>,
    is_dirty: bool,
    temp_id: String,
    encoding: String,
    has_bom: bool,
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

        let active_tab_index = manifest.active_tab_index.min(tabs.len() - 1);

        Ok(Some(RestoredSession {
            tabs,
            active_tab_index,
            font_size: manifest.font_size,
            word_wrap: manifest.word_wrap,
        }))
    }

    pub fn persist(
        &self,
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
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
                    name: buffer.name.clone(),
                    path: buffer.path.clone(),
                    is_dirty: buffer.is_dirty,
                    temp_id: buffer.temp_id.clone(),
                    encoding: buffer.encoding.clone(),
                    has_bom: buffer.has_bom,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;

        self.remove_stale_buffer_files(&active_temp_paths)?;

        let manifest = SessionManifest {
            version: SESSION_VERSION,
            active_tab_index: active_tab_index.min(tabs.len().saturating_sub(1)),
            font_size,
            word_wrap,
            tabs: session_tabs,
        };
        let json = serde_json::to_vec_pretty(&manifest).map_err(invalid_data)?;
        write_atomic(&self.manifest_path, &json)
    }

    fn remove_stale_buffer_files(&self, active_temp_paths: &HashSet<PathBuf>) -> io::Result<()> {
        let stale_paths = self.collect_stale_buffer_files(active_temp_paths)?;

        for path in stale_paths {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }

        Ok(())
    }

    fn collect_stale_buffer_files(
        &self,
        active_temp_paths: &HashSet<PathBuf>,
    ) -> io::Result<Vec<PathBuf>> {
        let mut stale_paths = Vec::new();

        if !self.root.exists() {
            return Ok(stale_paths);
        }

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();

            if self.is_stale_buffer_file(&path, active_temp_paths) {
                stale_paths.push(path);
            }
        }

        Ok(stale_paths)
    }

    fn is_stale_buffer_file(&self, path: &Path, active_temp_paths: &HashSet<PathBuf>) -> bool {
        if path == self.manifest_path || active_temp_paths.contains(path) {
            return false;
        }

        path.extension().and_then(|ext| ext.to_str()) == Some(BUFFER_FILE_EXTENSION)
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

        if manifest.version != SESSION_VERSION {
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    fn restore_tab(&self, tab: SessionTab) -> WorkspaceTab {
        let (content, encoding, has_bom) = self.restore_tab_content(&tab);
        let buffer = BufferState::restored(
            tab.name,
            content,
            tab.path,
            tab.is_dirty,
            tab.temp_id,
            encoding,
            has_bom,
        );
        WorkspaceTab::new(buffer)
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

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = path.with_extension(format!(
        "{}.write",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ));
    fs::write(&temp_path, bytes)?;

    if path.exists() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    fs::rename(temp_path, path)
}
