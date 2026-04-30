pub mod app_state;
pub mod capacity_metrics;
pub mod chrome;
pub mod color_contrast;
pub mod commands;
pub mod domain;
pub mod fonts;
pub mod services;
pub mod shortcuts;
pub mod startup;
pub mod theme;
pub mod transactions;
pub mod ui;
pub mod utils;

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
