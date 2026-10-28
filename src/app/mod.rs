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
pub(crate) mod platform_file;
pub mod services;
pub mod shortcut_keymap;
pub(crate) mod shortcut_tooltips;
pub mod shortcuts;
pub mod startup;
pub(crate) mod system_appearance;
pub(crate) mod text_history;
pub mod theme;
pub mod ui;
pub mod utils;

pub use app_state::ScratchpadApp;

use std::path::Path;

/// Owned, best-effort comparison text for a path, not a persistent file identity.
///
/// Construction attempts filesystem canonicalization and otherwise retains the
/// input spelling. Windows keys are lowercased; other platforms preserve case.
/// Equality and hashing compare the resulting lossy text, not file metadata:
/// distinct non-UTF-8 paths can collide, while aliases of missing paths can differ.
/// Keys constructed at different times can differ as filesystem state changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CanonicalPathKey(String);

impl CanonicalPathKey {
    /// Attempt to canonicalize `path`, falling back silently on any I/O error.
    ///
    /// The fallback does not make relative paths absolute or simplify `.`/`..`.
    /// Both paths through construction convert non-UTF-8 text lossily and apply
    /// Windows lowercasing; the key is not guaranteed to be a canonical pathname.
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        Self(normalize_path(path))
    }

    /// Borrow the stored comparison text, not necessarily the original spelling.
    ///
    /// This is neither a display-label contract nor a stable file identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Compare two independently constructed [`CanonicalPathKey`] values.
///
/// This can query the filesystem twice, without a shared filesystem snapshot.
/// Equality is not proof that paths identify the same file: lossy text can
/// collide, and different spellings of a missing path can compare unequal.
#[must_use]
pub fn paths_match(left: &Path, right: &Path) -> bool {
    CanonicalPathKey::from_path(left) == CanonicalPathKey::from_path(right)
}

fn normalize_path(path: &Path) -> String {
    let normalized = std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned();

    #[cfg(target_os = "windows")]
    {
        normalized.to_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::CanonicalPathKey;
    use std::path::Path;

    #[test]
    fn canonical_path_keys_resolve_existing_dot_spellings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("buffer.txt");
        std::fs::write(&path, "path-key fixture").unwrap();
        let dotted = directory.path().join(".").join("buffer.txt");
        let canonical = std::fs::canonicalize(&path).unwrap();
        let key = CanonicalPathKey::from_path(&dotted);

        assert_eq!(key, CanonicalPathKey::from_path(&canonical));
        assert!(super::paths_match(&path, &dotted));
        #[cfg(target_os = "windows")]
        assert_eq!(key.as_str(), canonical.to_string_lossy().to_lowercase());
        #[cfg(not(target_os = "windows"))]
        assert_eq!(key.as_str(), canonical.to_string_lossy().as_ref());
    }

    #[test]
    fn canonical_path_keys_keep_missing_dot_spellings_distinct() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.txt");
        let dotted = directory.path().join(".").join("missing.txt");
        assert!(std::fs::canonicalize(&path).is_err());
        assert!(std::fs::canonicalize(&dotted).is_err());

        assert_ne!(
            CanonicalPathKey::from_path(&path),
            CanonicalPathKey::from_path(&dotted)
        );
        assert!(!super::paths_match(&path, &dotted));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn canonical_path_keys_are_case_insensitive_on_windows() {
        assert_eq!(
            CanonicalPathKey::from_path(Path::new("C:/Scratchpad/Foo.txt")),
            CanonicalPathKey::from_path(Path::new("C:/Scratchpad/foo.txt"))
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn canonical_path_keys_preserve_case_on_case_sensitive_platforms() {
        assert_ne!(
            CanonicalPathKey::from_path(Path::new("/tmp/ScratchpadCaseProbe.txt")),
            CanonicalPathKey::from_path(Path::new("/tmp/scratchpadcaseprobe.txt"))
        );
    }
}
