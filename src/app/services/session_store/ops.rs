use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const BUFFER_FILE_EXTENSION: &str = "tmp";

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = path.with_extension(format!(
        "{}.write",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ));
    fs::write(&temp_path, bytes)?;

    if path.exists() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    fs::rename(temp_path, path)
}

pub(crate) fn collect_stale_buffer_files(
    root: &Path,
    manifest_path: &Path,
    active_temp_paths: &HashSet<PathBuf>,
) -> io::Result<Vec<PathBuf>> {
    let mut stale_paths = Vec::new();

    if !root.exists() {
        return Ok(stale_paths);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if is_stale_buffer_file(&path, manifest_path, active_temp_paths) {
            stale_paths.push(path);
        }
    }

    Ok(stale_paths)
}

fn is_stale_buffer_file(
    path: &Path,
    manifest_path: &Path,
    active_temp_paths: &HashSet<PathBuf>,
) -> bool {
    if path == manifest_path || active_temp_paths.contains(path) {
        return false;
    }

    path.extension().and_then(|ext| ext.to_str()) == Some(BUFFER_FILE_EXTENSION)
}
