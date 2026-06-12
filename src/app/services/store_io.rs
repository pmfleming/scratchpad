use crate::app::diagnostics;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, |file| file.write_all(bytes))
}

pub(crate) fn write_atomic_with<F>(path: &Path, write: F) -> io::Result<()>
where
    F: FnOnce(&mut fs::File) -> io::Result<()>,
{
    let temp_path = temp_path_for(path);
    let mut file = fs::File::create(&temp_path).inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "atomic_write_create_temp",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
    })?;
    if let Err(error) = write(&mut file) {
        diagnostics::record_io_error_with_details(
            "atomic_write_content",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
        drop(file);
        let _ = remove_file_if_exists(&temp_path);
        return Err(error);
    }
    if let Err(error) = file.flush() {
        diagnostics::record_io_error_with_details(
            "atomic_write_flush",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
        drop(file);
        let _ = remove_file_if_exists(&temp_path);
        return Err(error);
    }
    if let Err(error) = file.sync_all() {
        diagnostics::record_io_error_with_details(
            "atomic_write_sync",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
        drop(file);
        let _ = remove_file_if_exists(&temp_path);
        return Err(error);
    }
    drop(file);

    if let Err(error) = replace_file(&temp_path, path) {
        diagnostics::record_io_error_with_details(
            "atomic_write_replace",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
        let _ = remove_file_if_exists(&temp_path);
        return Err(error);
    }

    Ok(())
}

fn temp_path_for(path: &Path) -> std::path::PathBuf {
    path.with_extension(format!(
        "{}.write",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ))
}

fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    // On Windows, `std::fs::rename` uses MoveFileExW with replace semantics.
    // Do not delete `to` first: if replacement fails, the user's file must
    // remain intact, and there must not be a brief destination-missing window
    // where another process with the user's filesystem rights can substitute it.
    fs::rename(from, to)
}

pub(crate) fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            diagnostics::record_io_error(
                "remove_file_if_exists",
                Some(path),
                "store_io::remove_file_if_exists",
                &error,
            );
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{temp_path_for, write_atomic_with};
    use std::fs;
    use std::io::{self, Write};

    #[test]
    fn write_atomic_with_leaves_existing_target_when_replace_fails() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        fs::write(&path, b"old").unwrap();
        let temp_path = temp_path_for(&path);

        let error = write_atomic_with(&path, |file| {
            file.write_all(b"new")?;
            fs::rename(&temp_path, directory.path().join("moved-temp"))?;
            Ok(())
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read(&path).unwrap(), b"old");
    }

    #[test]
    fn write_atomic_with_removes_temp_file_after_write_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        fs::write(&path, b"old").unwrap();
        let temp_path = temp_path_for(&path);

        let error = write_atomic_with(&path, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("injected write failure"))
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert!(!temp_path.exists());
    }

    #[test]
    fn write_atomic_with_replaces_existing_target() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        fs::write(&path, b"old").unwrap();

        write_atomic_with(&path, |file| file.write_all(b"new")).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
    }
}
