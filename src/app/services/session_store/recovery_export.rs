//! Logical v7 export, not a raw copy of a potentially live database/WAL.
use super::{SessionStore, database::ReadScope};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

impl SessionStore {
    /// Export one committed session to a new private directory. Existing paths
    /// are never overwritten. The manifest is written last; an error can leave
    /// an incomplete directory, which must not be treated as a valid backup.
    pub fn export_recovery(&self, destination: &Path) -> io::Result<()> {
        let stored = self.read_session(ReadScope::All)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no committed session to export")
        })?;
        let mut directory = fs::DirBuilder::new();
        directory.recursive(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(destination)?;
        // Enumerate validated manifest references, never arbitrary database keys.
        for buffer in stored.manifest.tabs.iter().flat_map(|tab| &tab.buffers) {
            if let Some(text) = stored.contents.get(&buffer.temp_id) {
                write_new(
                    &destination.join(format!("{}.tmp", buffer.temp_id)),
                    |file| file.write_all(text.as_bytes()),
                )?;
            }
        }
        write_new(&destination.join("session.json"), |file| {
            serde_json::to_writer(file, &stored.manifest).map_err(io::Error::other)
        })?;
        #[cfg(unix)]
        {
            File::open(destination)?.sync_all()?;
            let parent = destination
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

fn write_new(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = BufWriter::new(options.open(path)?);
    write(&mut file)?;
    file.flush()?;
    file.get_ref().sync_all()
}
