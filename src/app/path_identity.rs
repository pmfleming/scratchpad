//! Native pathname ownership and short-lived observations of physical identity.
pub(crate) mod encoding;
use std::{
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
};

/// How a comparison pathname was obtained. Failure is retained, never lowercased
/// or converted lossily into a supposedly canonical identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathResolution {
    Resolved,
    Unresolved(io::ErrorKind),
    /// Even an absolute fallback could not be constructed.
    Unanchored(io::ErrorKind),
}

/// Evidence from a fresh comparison, not a persistent file identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathMatch {
    /// The same absolute pathname, including an exactly spelled missing target.
    SameLocation,
    /// Two simultaneously open handles identify the same file.
    SameFile,
    Different,
    /// Resolution or identity inspection failed; no alias match is established.
    Uncertain,
}

impl PathMatch {
    #[must_use]
    pub fn is_match(self) -> bool {
        matches!(self, Self::SameLocation | Self::SameFile)
    }
}

/// A native pathname key for an index, with explicit resolution evidence.
///
/// Equality/hash compare the exact anchored request spelling, not physical identity.
/// The original absolute request is retained so decisions can recheck symlinks
/// and replacements rather than trust this cached observation. No case folding,
/// lossy Unicode conversion, or lexical `..` simplification is performed.
///
/// For example, two existing hard-link paths have distinct keys but `compare`
/// returns `SameFile`. Missing `note` and `./note` remain uncertain; repeating
/// exactly the same absolute missing path yields `SameLocation`. That result
/// describes pathname ownership, not the existence or identity of a file.
#[derive(Clone, Debug)]
pub struct CanonicalPathKey {
    requested: PathBuf,
    comparison: PathBuf,
    resolution: PathResolution,
}

impl PartialEq for CanonicalPathKey {
    fn eq(&self, other: &Self) -> bool {
        self.requested.as_os_str() == other.requested.as_os_str()
    }
}
impl Eq for CanonicalPathKey {}
impl Hash for CanonicalPathKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.requested.as_os_str().hash(state);
    }
}

impl CanonicalPathKey {
    /// Anchor relative paths to the current directory before filesystem lookup.
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        if path.is_absolute() {
            return Self::resolve(path.to_owned());
        }
        match std::env::current_dir() {
            Ok(base) => Self::from_path_at(path, &base),
            Err(error) => Self {
                requested: path.to_owned(),
                comparison: path.to_owned(),
                resolution: PathResolution::Unanchored(error.kind()),
            },
        }
    }

    /// Use a caller's known absolute base. Windows drive-relative/root-relative
    /// inputs that cannot be anchored by joining remain explicitly unanchored.
    #[must_use]
    pub fn from_path_at(path: &Path, base: &Path) -> Self {
        Self::resolve(base.join(path))
    }

    fn resolve(requested: PathBuf) -> Self {
        if !requested.is_absolute() {
            return Self {
                comparison: requested.clone(),
                requested,
                resolution: PathResolution::Unanchored(io::ErrorKind::InvalidInput),
            };
        }
        let (comparison, resolution) = match std::fs::canonicalize(&requested) {
            Ok(path) => (path, PathResolution::Resolved),
            Err(error) => (requested.clone(), PathResolution::Unresolved(error.kind())),
        };
        Self {
            requested,
            comparison,
            resolution,
        }
    }

    #[must_use]
    /// The resolved pathname or fallback spelling, accompanied by `resolution`.
    /// Use only as an index hint; `compare` re-observes the original requests.
    pub fn as_path(&self) -> &Path {
        &self.comparison
    }

    /// Absolute spelling supplied by the caller, before symlink resolution.
    #[must_use]
    pub fn requested_path(&self) -> &Path {
        &self.requested
    }

    #[must_use]
    pub fn resolution(&self) -> PathResolution {
        self.resolution
    }

    /// Recheck both original requests. Identity handles live only for this check;
    /// neither inode numbers nor Windows file IDs are retained in session state.
    #[must_use]
    pub fn compare(&self, other: &Self) -> PathMatch {
        let left = Self::resolve(self.requested.clone());
        let right = Self::resolve(other.requested.clone());
        if matches!(left.resolution, PathResolution::Unanchored(_))
            || matches!(right.resolution, PathResolution::Unanchored(_))
        {
            return PathMatch::Uncertain;
        }
        if left.requested.as_os_str() == right.requested.as_os_str()
            || (left.resolution == PathResolution::Resolved
                && right.resolution == PathResolution::Resolved
                && left.comparison.as_os_str() == right.comparison.as_os_str())
        {
            return PathMatch::SameLocation;
        }
        match super::platform::file_identity::same_file(&left.requested, &right.requested) {
            Ok(true) => PathMatch::SameFile,
            Ok(false) => PathMatch::Different,
            Err(_) => PathMatch::Uncertain,
        }
    }

    /// Deliberate ownership policy: exact absolute location or verified alias.
    /// An uncertain comparison never redirects an open to a different spelling.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self.compare(other).is_match()
    }
}

/// Compare current pathname ownership, including verified physical aliases.
/// Filesystem changes after the check remain possible; this is not a lock.
#[must_use]
pub fn paths_match(left: &Path, right: &Path) -> bool {
    CanonicalPathKey::from_path(left).matches(&CanonicalPathKey::from_path(right))
}

#[cfg(test)]
mod tests;
