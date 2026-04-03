use std::path::PathBuf;

pub struct TabState {
    pub name: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
}

impl TabState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        Self {
            name,
            content,
            path,
            is_dirty: false,
        }
    }

    pub fn display_name(&self) -> String {
        let marker = if self.is_dirty { "*" } else { "" };
        format!("{}{}", marker, self.name)
    }
}
