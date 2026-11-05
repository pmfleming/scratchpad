use super::{FileContent, FileService};
use crate::app::domain::{BufferState, DiskFileState};
use std::{io, path::Path};

pub(crate) struct ObservedFile {
    pub content: FileContent,
    pub state: DiskFileState,
    pub digest: String,
}

impl FileService {
    /// Run on an I/O worker. Hash decoded text before accepting its metadata;
    /// retry replacements/writes that occur while the file is being read.
    pub(crate) fn read_observed_file(path: &Path) -> io::Result<ObservedFile> {
        for _ in 0..3 {
            let before = Self::read_disk_state(path)?;
            let content = Self::read_file(path)?;
            let digest = content.document.content_digest();
            let after = Self::read_disk_state(path)?;
            if before == after {
                return Ok(ObservedFile {
                    content,
                    state: after,
                    digest,
                });
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "File kept changing while it was being read. Current text was kept.",
        ))
    }

    pub(crate) fn buffer_from_observed(path: &Path, observed: ObservedFile) -> BufferState {
        Self::build_buffer_with_digest(
            path,
            observed.content,
            Some(observed.state),
            Some(observed.digest),
        )
    }
}
