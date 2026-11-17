//! Physical identity is compared while both handles are alive, never cached.
#[cfg(windows)]
pub(crate) use windows_file_identity::same_file;

#[cfg(unix)]
use std::{fs::File, io, path::Path};

#[cfg(unix)]
pub(crate) fn same_file(left: &Path, right: &Path) -> io::Result<bool> {
    let left = open(left)?;
    let right = open(right)?;
    compare(&left, &right)
}

#[cfg(unix)]
fn open(path: &Path) -> io::Result<File> {
    // Linux O_PATH observes identity without opening a FIFO/device for data I/O.
    #[cfg(target_os = "linux")]
    {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        Ok(File::from(fd))
    }
    #[cfg(not(target_os = "linux"))]
    {
        File::open(path)
    }
}

#[cfg(unix)]
fn compare(left: &File, right: &File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    if left.ino() == 0 || right.ino() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "filesystem did not provide an inode",
        ));
    }
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}
