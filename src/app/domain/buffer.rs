use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub struct BufferState {
    pub name: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub temp_id: String,
    pub line_count: usize,
    pub encoding: String,
    pub has_bom: bool,
}

impl BufferState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        Self::with_encoding(name, content, path, "UTF-8".to_string(), false)
    }

    pub fn with_encoding(
        name: String,
        content: String,
        path: Option<PathBuf>,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        let line_count = content.lines().count().max(1);
        Self {
            name,
            content,
            path,
            is_dirty: false,
            temp_id: next_temp_id(),
            line_count,
            encoding,
            has_bom,
        }
    }

    pub fn restored(
        name: String,
        content: String,
        path: Option<PathBuf>,
        is_dirty: bool,
        temp_id: String,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        let line_count = content.lines().count().max(1);
        Self {
            name,
            content,
            path,
            is_dirty,
            temp_id,
            line_count,
            encoding,
            has_bom,
        }
    }

    pub fn display_name(&self) -> String {
        let marker = if self.is_dirty { "*" } else { "" };
        format!("{}{}", marker, self.name)
    }

    pub fn overflow_context_label(&self) -> Option<String> {
        self.path.as_ref().map(|path| path.display().to_string())
    }
}

fn next_temp_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_TEMP_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
    format!("buffer-{timestamp}-{sequence}")
}
