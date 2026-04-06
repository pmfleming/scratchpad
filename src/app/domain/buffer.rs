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
}

impl BufferState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        Self::restored(name, content, path, false, next_temp_id())
    }

    pub fn restored(
        name: String,
        content: String,
        path: Option<PathBuf>,
        is_dirty: bool,
        temp_id: String,
    ) -> Self {
        Self {
            name,
            content,
            path,
            is_dirty,
            temp_id,
        }
    }

    pub fn display_name(&self) -> String {
        let marker = if self.is_dirty { "*" } else { "" };
        format!("{}{}", marker, self.name)
    }

    pub fn overflow_context_label(&self) -> Option<String> {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
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

#[cfg(test)]
mod tests {
    use super::BufferState;
    use std::path::PathBuf;

    #[test]
    fn new_buffer_starts_clean() {
        let buffer = BufferState::new("Untitled".to_owned(), "hello".to_owned(), None);

        assert_eq!(buffer.name, "Untitled");
        assert_eq!(buffer.content, "hello");
        assert_eq!(buffer.path, None);
        assert!(!buffer.is_dirty);
        assert!(buffer.temp_id.starts_with("buffer-"));
    }

    #[test]
    fn display_name_prefixes_dirty_marker() {
        let mut buffer = BufferState::new(
            "notes.txt".to_owned(),
            String::new(),
            Some(PathBuf::from("notes.txt")),
        );

        assert_eq!(buffer.display_name(), "notes.txt");

        buffer.is_dirty = true;

        assert_eq!(buffer.display_name(), "*notes.txt");
    }

    #[test]
    fn restored_buffer_preserves_session_metadata() {
        let buffer = BufferState::restored(
            "draft.md".to_owned(),
            "content".to_owned(),
            Some(PathBuf::from("draft.md")),
            true,
            "buffer-restore-1".to_owned(),
        );

        assert!(buffer.is_dirty);
        assert_eq!(buffer.temp_id, "buffer-restore-1");
    }

    #[test]
    fn overflow_context_uses_path_when_available() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            String::new(),
            Some(PathBuf::from("docs\\notes.txt")),
        );

        assert!(buffer.overflow_context_label().unwrap().contains("notes.txt"));
    }
}
