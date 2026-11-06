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
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // Each writer owns one same-filesystem stage. Dropping it cleans up on
    // every early return, including a failed replacement and unwinding.
    let mut stage = tempfile::NamedTempFile::new_in(parent).inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "atomic_write_create_temp",
            Some(path),
            "store_io::write_atomic_with",
            error,
            [("temp_directory", parent.display().to_string())],
        );
    })?;
    let mut operation = "atomic_write_content";
    (|| {
        write(stage.as_file_mut())?;
        operation = "atomic_write_flush";
        stage.as_file_mut().flush()?;
        operation = "atomic_write_sync";
        stage.as_file().sync_all()
    })()
    .inspect_err(|error| record_atomic_error(operation, path, stage.path(), error))?;

    // persist replaces atomically, without deleting the destination first.
    stage.persist(path).map_err(|failure| {
        record_atomic_error(
            "atomic_write_replace",
            path,
            failure.file.path(),
            &failure.error,
        );
        failure.error
    })?;
    Ok(())
}

fn record_atomic_error(operation: &'static str, path: &Path, stage: &Path, error: &io::Error) {
    diagnostics::record_io_error_with_details(
        operation,
        Some(path),
        "store_io::write_atomic_with",
        error,
        [("temp_path", stage.display().to_string())],
    );
}

#[cfg(test)]
mod tests {
    use super::{write_atomic, write_atomic_with};
    use std::{
        fs,
        io::{self, Write},
    };

    #[test]
    fn failed_atomic_writes_preserve_the_target_and_remove_staging_files() {
        for replace_failure in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("state");
            let original = if replace_failure {
                fs::create_dir(&path).unwrap();
                path.join("original")
            } else {
                path.clone()
            };
            fs::write(&original, b"old").unwrap();
            let error = write_atomic_with(&path, |file| {
                file.write_all(b"partial")?;
                if replace_failure {
                    Ok(())
                } else {
                    Err(io::Error::other("injected write failure"))
                }
            })
            .unwrap_err();
            if !replace_failure {
                assert_eq!(error.kind(), io::ErrorKind::Other);
            }
            assert_eq!(fs::read(original).unwrap(), b"old");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
            let invoked = std::cell::Cell::new(false);
            assert!(
                write_atomic_with(&directory.path().join("missing/state"), |_| {
                    invoked.set(true);
                    Ok(())
                })
                .is_err()
            );
            assert!(!invoked.get());
        }
    }

    #[test]
    fn overlapping_atomic_writers_publish_independent_complete_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        let stale = path.with_extension("toml.write");
        fs::write(&path, b"old").unwrap();
        fs::write(&stale, b"untouched").unwrap();
        // Nested calls deterministically overlap staging lifetimes, without a
        // thread scheduler or timing assumption.
        write_atomic_with(&path, |file| {
            file.write_all(b"first")?;
            write_atomic(&path, b"second")?;
            assert_eq!(fs::read(&path)?, b"second");
            file.write_all(b" tail")
        })
        .unwrap();
        assert_eq!(fs::read(path).unwrap(), b"first tail");
        assert_eq!(fs::read(stale).unwrap(), b"untouched");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
