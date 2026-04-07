mod app_state;
pub mod chrome;
pub mod commands;
pub mod domain;
pub mod services;
pub mod theme;
pub mod ui;

pub use app_state::ScratchpadApp;

use std::fs;
use std::path::Path;

pub fn paths_match(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}
