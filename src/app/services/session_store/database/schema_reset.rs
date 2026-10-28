use std::{fmt, fs, io, path::Path};

#[derive(Debug)]
pub(super) struct UnsupportedSchema {
    pub version: i64,
    pub application: i64,
}

impl fmt::Display for UnsupportedSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "schema version {} (application ID {:#x}) cannot be upgraded by this build",
            self.version, self.application
        )
    }
}
impl std::error::Error for UnsupportedSchema {}

/// Only called for a positively identified unsupported schema, with all database
/// handles closed and the session ownership lock still held. Never reset on an
/// arbitrary open, migration, corruption or I/O error.
pub(super) fn discard(root: &Path) -> io::Result<()> {
    for name in ["session.db-wal", "session.db-shm", "session.db"] {
        match fs::remove_file(root.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
