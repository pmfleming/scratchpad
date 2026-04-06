use crate::app::domain::{BufferState, WorkspaceTab};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SESSION_DIR_NAME: &str = "scratchpad";
const SESSION_MANIFEST_NAME: &str = "session.json";
const BUFFER_FILE_EXTENSION: &str = "tmp";
const SESSION_VERSION: u32 = 1;

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
        if !self.manifest_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&self.manifest_path)?;
        let manifest: SessionManifest = serde_json::from_str(&raw).map_err(invalid_data)?;

        if manifest.version != SESSION_VERSION {
            return Ok(None);
        }

        let mut tabs = Vec::with_capacity(manifest.tabs.len());
        for tab in manifest.tabs {
            let content = match fs::read_to_string(self.buffer_path(&tab.temp_id)) {
                Ok(content) => content,
                Err(_) => match &tab.path {
                    Some(path) => fs::read_to_string(path).unwrap_or_default(),
                    None => String::new(),
                },
            };

            let buffer = BufferState::restored(tab.name, content, tab.path, tab.is_dirty, tab.temp_id);
            tabs.push(WorkspaceTab::new(buffer));
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
        if !self.root.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path == self.manifest_path || active_temp_paths.contains(&path) {
                continue;
            }

            if path.extension().and_then(|ext| ext.to_str()) == Some(BUFFER_FILE_EXTENSION) {
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
        }

        Ok(())
    }

    fn buffer_path(&self, temp_id: &str) -> PathBuf {
        self.root.join(format!("{temp_id}.{BUFFER_FILE_EXTENSION}"))
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

#[cfg(test)]
mod tests {
    use super::SessionStore;
    use crate::app::domain::{BufferState, WorkspaceTab};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn persists_and_restores_open_tabs() {
        let root = std::env::temp_dir().join(format!(
            "scratchpad-session-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = SessionStore::new(root.clone());
        let tabs = vec![
            WorkspaceTab::new(BufferState::restored(
                "notes.txt".to_owned(),
                "alpha".to_owned(),
                Some(PathBuf::from("notes.txt")),
                true,
                "buffer-a".to_owned(),
            )),
            WorkspaceTab::new(BufferState::restored(
                "Untitled".to_owned(),
                "beta".to_owned(),
                None,
                false,
                "buffer-b".to_owned(),
            )),
        ];

        store.persist(&tabs, 1, 18.0, false).unwrap();
        let restored = store.load().unwrap().unwrap();

        assert_eq!(restored.tabs.len(), 2);
        assert_eq!(restored.active_tab_index, 1);
        assert_eq!(restored.font_size, 18.0);
        assert!(!restored.word_wrap);
        assert_eq!(restored.tabs[0].buffer.content, "alpha");
        assert!(restored.tabs[0].buffer.is_dirty);
        assert_eq!(restored.tabs[1].buffer.content, "beta");

        fs::remove_dir_all(root).unwrap();
    }
}