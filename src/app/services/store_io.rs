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
    let temp_path = path.with_extension(format!(
        "{}.write",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ));
    let mut file = fs::File::create(&temp_path).inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "atomic_write_create_temp",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
    })?;
    write(&mut file).inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "atomic_write_content",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
    })?;
    file.flush().inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "atomic_write_flush",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
    })?;
    drop(file);

    remove_file_if_exists(path)?;
    fs::rename(&temp_path, path).inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "atomic_write_replace",
            Some(path),
            "store_io::write_atomic_with",
            &error,
            [("temp_path", temp_path.display().to_string())],
        );
    })
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
