#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::float_cmp,
    clippy::inline_always,
    clippy::large_stack_arrays,
    clippy::many_single_char_names,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unused_self
)]

pub mod app_paths;
pub mod app_state;
pub mod capacity_metrics;
pub mod chrome;
pub mod color_contrast;
pub mod commands;
pub mod diagnostics;
pub mod domain;
pub mod fonts;
pub mod memory_budget;
pub mod platform;
pub mod services;
pub mod shortcut_keymap;
pub(crate) mod shortcut_tooltips;
pub mod shortcuts;
pub mod startup;
pub(crate) mod text_history;
pub mod theme;
pub mod ui;
pub mod utils;

pub use app_state::ScratchpadApp;

use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CanonicalPathKey(String);

impl CanonicalPathKey {
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        Self(normalize_path(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[must_use]
pub fn paths_match(left: &Path, right: &Path) -> bool {
    CanonicalPathKey::from_path(left) == CanonicalPathKey::from_path(right)
}

fn normalize_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}
