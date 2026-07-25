#[cfg(target_os = "linux")]
use crate::app::diagnostics;
#[cfg(not(target_os = "linux"))]
use crate::app::services::store_io::write_atomic_with;
use std::fs::File;
#[cfg(target_os = "linux")]
use std::fs::{self, OpenOptions};
use std::io;
#[cfg(target_os = "linux")]
use std::io::{Seek, Write};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

pub(super) fn write_document_with<F>(path: &Path, write: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    imp::write_document_with(path, write)
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{File, Path, io, write_atomic_with};

    pub(super) fn write_document_with<F>(path: &Path, write: F) -> io::Result<()>
    where
        F: FnOnce(&mut File) -> io::Result<()>,
    {
        write_atomic_with(path, write)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{File, OpenOptions, Path, PathBuf, Seek, Write, diagnostics, fs, io};
    use rustix::fs::XattrFlags;
    use std::ffi::{OsStr, OsString};
    use std::os::linux::fs::MetadataExt;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static ARTIFACT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    pub(super) fn write_document_with<F>(path: &Path, write: F) -> io::Result<()>
    where
        F: FnOnce(&mut File) -> io::Result<()>,
    {
        let target = resolve_write_target(path)?;
        let existing = existing_regular_file(&target)?;
        let (stage, mut stage_file) = create_artifact(&target, "write", 0o666)?;

        write_and_sync(&mut stage_file, write)?;
        drop(stage_file);

        let Some((original, metadata)) = existing else {
            fs::rename(stage.path(), &target)?;
            sync_parent(&target)?;
            return Ok(());
        };

        if metadata.st_nlink() > 1 {
            return replace_in_place_with_recovery(&target, stage.path());
        }

        match prepare_atomic_replacement(stage.path(), &original, &metadata) {
            Ok(()) => {
                fs::rename(stage.path(), &target)?;
                sync_parent(&target)?;
                Ok(())
            }
            Err(error) => {
                diagnostics::record_warning(
                    "document_atomic_metadata_fallback",
                    Some(&target),
                    "file_service::document_write",
                    format!(
                        "Could not copy all Unix metadata to an atomic replacement; using a recovery-backed in-place save: {error}"
                    ),
                );
                replace_in_place_with_recovery(&target, stage.path())
            }
        }
    }

    fn resolve_write_target(path: &Path) -> io::Result<PathBuf> {
        match fs::symlink_metadata(path) {
            // Canonicalizing an existing path makes a save through a symlink update
            // its target rather than replacing the link itself.
            Ok(_) => fs::canonicalize(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let file_name = path.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "save path has no file name")
                })?;
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                Ok(fs::canonicalize(parent)?.join(file_name))
            }
            Err(error) => Err(error),
        }
    }

    fn existing_regular_file(path: &Path) -> io::Result<Option<(File, fs::Metadata)>> {
        match File::open(path) {
            Ok(file) => {
                let metadata = file.metadata()?;
                if !metadata.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("save target is not a regular file: {}", path.display()),
                    ));
                }
                Ok(Some((file, metadata)))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write_and_sync<F>(file: &mut File, write: F) -> io::Result<()>
    where
        F: FnOnce(&mut File) -> io::Result<()>,
    {
        write(file)?;
        file.flush()?;
        file.sync_all()
    }

    fn prepare_atomic_replacement(
        stage_path: &Path,
        original: &File,
        metadata: &fs::Metadata,
    ) -> io::Result<()> {
        // chown can clear set-id mode bits, so restore permissions after ownership.
        std::os::unix::fs::chown(stage_path, Some(metadata.st_uid()), Some(metadata.st_gid()))?;
        fs::set_permissions(stage_path, metadata.permissions())?;
        let stage = OpenOptions::new().read(true).write(true).open(stage_path)?;
        copy_extended_attributes(original, &stage)?;
        stage.sync_all()
    }

    fn copy_extended_attributes(original: &File, replacement: &File) -> io::Result<()> {
        for name in extended_attribute_names(original)? {
            let name = OsStr::from_bytes(&name);
            let value = extended_attribute_value(original, name)?;
            rustix::fs::fsetxattr(replacement, name, &value, XattrFlags::empty())?;
        }
        Ok(())
    }

    fn extended_attribute_names(file: &File) -> io::Result<Vec<Vec<u8>>> {
        let required = rustix::fs::flistxattr(file, &mut [] as &mut [u8])?;
        if required == 0 {
            return Ok(Vec::new());
        }

        let mut raw = vec![0_u8; required];
        let used = rustix::fs::flistxattr(file, &mut raw)?;
        raw.truncate(used);
        Ok(raw
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(<[u8]>::to_vec)
            .collect())
    }

    fn extended_attribute_value(file: &File, name: &OsStr) -> io::Result<Vec<u8>> {
        let required = rustix::fs::fgetxattr(file, name, &mut [] as &mut [u8])?;
        let mut value = vec![0_u8; required];
        if required > 0 {
            let used = rustix::fs::fgetxattr(file, name, &mut value)?;
            value.truncate(used);
        }
        Ok(value)
    }

    fn replace_in_place_with_recovery(target: &Path, staged: &Path) -> io::Result<()> {
        let (mut recovery, mut recovery_file) = create_artifact(target, "recovery", 0o600)?;
        let mut original = File::open(target)?;
        io::copy(&mut original, &mut recovery_file)?;
        recovery_file.flush()?;
        recovery_file.sync_all()?;
        drop(recovery_file);
        sync_parent(target)?;

        let result = (|| {
            let mut staged_file = File::open(staged)?;
            let mut target_file = OpenOptions::new().write(true).open(target)?;
            target_file.seek(io::SeekFrom::Start(0))?;
            let written = io::copy(&mut staged_file, &mut target_file)?;
            target_file.set_len(written)?;
            target_file.flush()?;
            target_file.sync_all()
        })();

        if let Err(error) = result {
            recovery.preserve();
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "in-place save failed; recovery copy retained at {}: {error}",
                    recovery.path().display()
                ),
            ));
        }

        if let Err(error) = recovery.remove() {
            recovery.preserve();
            diagnostics::record_io_error_with_details(
                "document_recovery_cleanup",
                Some(target),
                "file_service::document_write",
                &error,
                [("recovery_path", recovery.path().display().to_string())],
            );
        } else if let Err(error) = sync_parent(target) {
            diagnostics::record_io_error(
                "document_recovery_cleanup_sync",
                Some(target),
                "file_service::document_write",
                &error,
            );
        }

        Ok(())
    }

    fn create_artifact(target: &Path, label: &str, mode: u32) -> io::Result<(Artifact, File)> {
        let parent = target.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "save target has no parent")
        })?;
        let file_name = target.file_name().unwrap_or_else(|| OsStr::new("document"));

        for _ in 0..128 {
            let sequence = ARTIFACT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let mut artifact_name = OsString::from(".");
            artifact_name.push(file_name);
            artifact_name.push(format!(
                ".scratchpad-{label}-{}-{sequence}",
                std::process::id()
            ));
            let artifact_path = parent.join(artifact_name);
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(mode)
                .open(&artifact_path)
            {
                Ok(file) => return Ok((Artifact::new(artifact_path), file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique Scratchpad save artifact",
        ))
    }

    fn sync_parent(path: &Path) -> io::Result<()> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        File::open(parent)?.sync_all()
    }

    struct Artifact {
        path: PathBuf,
        preserve: bool,
    }

    impl Artifact {
        fn new(path: PathBuf) -> Self {
            Self {
                path,
                preserve: false,
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn preserve(&mut self) {
            self.preserve = true;
        }

        fn remove(&mut self) -> io::Result<()> {
            fs::remove_file(&self.path)
        }
    }

    impl Drop for Artifact {
        fn drop(&mut self) {
            if !self.preserve {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            File, OsStr, Path, PathBuf, Write, XattrFlags, extended_attribute_value, fs,
            replace_in_place_with_recovery, write_document_with,
        };
        use std::os::linux::fs::MetadataExt;
        use std::os::unix::fs::{PermissionsExt, symlink};

        #[test]
        fn atomic_save_preserves_executable_permissions() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("script.sh");
            fs::write(&path, b"old").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();

            write_document_with(&path, |file| file.write_all(b"new")).unwrap();

            assert_eq!(fs::read(&path).unwrap(), b"new");
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o751
            );
        }

        #[test]
        fn atomic_save_preserves_user_extended_attributes_when_supported() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("tagged.txt");
            fs::write(&path, b"old").unwrap();
            let name = OsStr::new("user.scratchpad-test");
            if rustix::fs::setxattr(&path, name, b"kept", XattrFlags::empty()).is_err() {
                return;
            }

            write_document_with(&path, |file| file.write_all(b"new")).unwrap();

            let file = File::open(&path).unwrap();
            assert_eq!(extended_attribute_value(&file, name).unwrap(), b"kept");
        }

        #[test]
        fn save_through_symlink_updates_target_without_replacing_link() {
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("target.txt");
            let link = directory.path().join("link.txt");
            fs::write(&target, b"old").unwrap();
            symlink(&target, &link).unwrap();

            write_document_with(&link, |file| file.write_all(b"new")).unwrap();

            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read(&target).unwrap(), b"new");
            assert_eq!(fs::read(&link).unwrap(), b"new");
        }

        #[test]
        fn hard_link_save_preserves_inode_and_all_linked_names() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("document.txt");
            let alias = directory.path().join("alias.txt");
            fs::write(&path, b"old content").unwrap();
            fs::hard_link(&path, &alias).unwrap();
            let inode = fs::metadata(&path).unwrap().st_ino();

            write_document_with(&path, |file| file.write_all(b"new")).unwrap();

            assert_eq!(fs::metadata(&path).unwrap().st_ino(), inode);
            assert_eq!(fs::metadata(&alias).unwrap().st_ino(), inode);
            assert_eq!(fs::read(&path).unwrap(), b"new");
            assert_eq!(fs::read(&alias).unwrap(), b"new");
            assert!(recovery_artifacts(directory.path()).is_empty());
        }

        #[test]
        fn failed_in_place_save_retains_original_recovery_copy() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("document.txt");
            let invalid_stage = directory.path().join("not-a-readable-file");
            fs::write(&path, b"recover me").unwrap();
            fs::create_dir(&invalid_stage).unwrap();

            let error = replace_in_place_with_recovery(&path, &invalid_stage).unwrap_err();

            let recoveries = recovery_artifacts(directory.path());
            assert_eq!(recoveries.len(), 1);
            assert_eq!(fs::read(&recoveries[0]).unwrap(), b"recover me");
            assert!(
                error
                    .to_string()
                    .contains(&recoveries[0].display().to_string())
            );
        }

        fn recovery_artifacts(directory: &Path) -> Vec<PathBuf> {
            fs::read_dir(directory)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(OsStr::to_str)
                        .is_some_and(|name| name.contains(".scratchpad-recovery-"))
                })
                .collect()
        }
    }
}
