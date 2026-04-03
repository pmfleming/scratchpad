use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_TAB_ID: AtomicU64 = AtomicU64::new(1);

pub struct TabState {
    pub name: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub temp_id: String,
}

impl TabState {
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
}

fn next_temp_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_TEMP_TAB_ID.fetch_add(1, Ordering::Relaxed);
    format!("tab-{timestamp}-{sequence}")
}

#[cfg(test)]
mod tests {
    use super::TabState;
    use std::path::PathBuf;

    #[test]
    fn new_tab_starts_clean() {
        let tab = TabState::new("Untitled".to_owned(), "hello".to_owned(), None);

        assert_eq!(tab.name, "Untitled");
        assert_eq!(tab.content, "hello");
        assert_eq!(tab.path, None);
        assert!(!tab.is_dirty);
        assert!(tab.temp_id.starts_with("tab-"));
    }

    #[test]
    fn display_name_prefixes_dirty_marker() {
        let mut tab = TabState::new(
            "notes.txt".to_owned(),
            String::new(),
            Some(PathBuf::from("notes.txt")),
        );

        assert_eq!(tab.display_name(), "notes.txt");

        tab.is_dirty = true;

        assert_eq!(tab.display_name(), "*notes.txt");
    }

    #[test]
    fn restored_tab_preserves_session_metadata() {
        let tab = TabState::restored(
            "draft.md".to_owned(),
            "content".to_owned(),
            Some(PathBuf::from("draft.md")),
            true,
            "tab-restore-1".to_owned(),
        );

        assert!(tab.is_dirty);
        assert_eq!(tab.temp_id, "tab-restore-1");
    }
}
