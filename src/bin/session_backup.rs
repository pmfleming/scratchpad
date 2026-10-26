//! Offline logical recovery export. Keep the source database and WAL together.
use scratchpad::app::services::session_store::SessionStore;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: session_backup SOURCE_SESSION_DIRECTORY NEW_EXPORT_DIRECTORY (close Scratchpad first)",
        ));
    }
    let source = Path::new(&args[0]);
    let destination = Path::new(&args[1]);
    if !source.join("session.db").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "source contains no session database",
        ));
    }
    let _instance = lock_instance(source)?;
    SessionStore::new(source.to_owned()).export_recovery(destination)?;
    println!("Recovery export written to {}", destination.display());
    Ok(())
}

fn lock_instance(source: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(source.join("instance.lock"))?;
    file.try_lock()
        .map_err(|error| io::Error::other(format!("close Scratchpad before exporting: {error}")))?;
    Ok(file)
}
