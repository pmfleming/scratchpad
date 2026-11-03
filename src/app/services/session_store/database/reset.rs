//! Explicit fresh start; keep the old bytes temporarily without migrating them.
use super::{DatabaseWorker, create_private_directory, error, private_file};
use crate::app::services::session_store::SessionPersistRequest;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SESSION_FILES: [&str; 4] = [
    "session.db-wal",
    "session.db-shm",
    "session.db-journal",
    "session.db",
];

pub(in crate::app::services::session_store) fn start_new_session(
    root: &Path,
    worker: Option<&DatabaseWorker>,
    request: SessionPersistRequest,
) -> io::Result<(DatabaseWorker, PathBuf)> {
    let ownership = if let Some(worker) = worker {
        worker.close_for_replacement()?
    } else {
        create_private_directory(root)?;
        let ownership = private_file(&root.join("session.db.lock"))?;
        ownership.try_lock().map_err(error)?;
        ownership
    };
    let backup = backup_session_files(root, &std::env::temp_dir())?;
    let result = (|| {
        let worker = DatabaseWorker::open_owned(root.to_owned(), ownership)?;
        worker.call(move |db| {
            let generation = request.generation;
            futures_lite::future::block_on(db.persist(request, generation)).map(|_| ())
        })?;
        Ok((worker, backup.clone()))
    })();
    result.map_err(|cause: io::Error| {
        error(format!(
            "Could not create the new session: {cause}. Previous session backup: {}. Temporary-file cleanup may delete this backup.",
            backup.display()
        ))
    })
}

fn backup_session_files(root: &Path, temporary_root: &Path) -> io::Result<PathBuf> {
    let backup = tempfile::Builder::new()
        .prefix("scratchpad-session-backup-")
        .tempdir_in(temporary_root)?;
    create_private_directory(backup.path())?;
    let mut copied = Vec::new();
    // Copy and sync the complete set before removing any source. This also
    // handles temporary storage on a different filesystem from the session.
    for name in SESSION_FILES {
        let source = root.join(name);
        match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => return Err(error(format!("{} is not a regular file", source.display()))),
            Err(cause) if cause.kind() == io::ErrorKind::NotFound => continue,
            Err(cause) => return Err(cause),
        }
        let destination = backup.path().join(name);
        fs::copy(source, &destination)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o600))?;
        }
        private_file(&destination)?.sync_all()?;
        copied.push(name);
    }
    #[cfg(unix)]
    fs::File::open(backup.path())?.sync_all()?;
    // Do not delete on app exit: leave this private backup to OS temp cleanup.
    let backup = backup.keep();
    for name in copied {
        fs::remove_file(root.join(name)).map_err(|cause| {
            error(format!(
                "Could not move the previous session: {cause}. Backup retained at {}. No new session was created.",
                backup.display()
            ))
        })?;
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_database_and_sidecars_but_leaves_other_files() {
        let root = tempfile::tempdir().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        for name in SESSION_FILES {
            fs::write(root.path().join(name), name).unwrap();
        }
        fs::write(root.path().join("settings.toml"), "settings").unwrap();
        let backup = backup_session_files(root.path(), temporary.path()).unwrap();
        for name in SESSION_FILES {
            assert_eq!(fs::read_to_string(backup.join(name)).unwrap(), name);
            assert!(!root.path().join(name).exists());
        }
        assert!(root.path().join("settings.toml").exists());
    }

    #[test]
    fn incomplete_backup_does_not_remove_any_source() {
        let root = tempfile::tempdir().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        fs::write(root.path().join("session.db-wal"), "journal").unwrap();
        fs::create_dir(root.path().join("session.db")).unwrap();
        assert!(backup_session_files(root.path(), temporary.path()).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("session.db-wal")).unwrap(),
            "journal"
        );
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }

    #[test]
    fn newer_session_is_backed_up_and_replaced_only_on_request() {
        use super::super::tests::{request, tab};
        use crate::app::services::session_store::SessionStore;
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().to_owned());
        store.persist(&[tab("old text")], 0, 16.0, false).unwrap();
        store
            .worker()
            .unwrap()
            .call(|db| {
                futures_lite::future::block_on(async {
                    db.conn
                        .execute("PRAGMA user_version=999", ())
                        .await
                        .map_err(error)?;
                    db.checkpoint().await
                })
            })
            .unwrap();
        drop(store);
        let previous = fs::read(root.path().join("session.db")).unwrap();
        let mut store = SessionStore::new(root.path().to_owned());
        assert!(store.load().is_err());
        assert_eq!(fs::read(root.path().join("session.db")).unwrap(), previous);
        let backup = store
            .start_new_session(request(&[tab("fresh text")]))
            .unwrap();
        assert_eq!(fs::read(backup.join("session.db")).unwrap(), previous);
        assert_eq!(
            store.load().unwrap().unwrap().tabs[0]
                .active_buffer()
                .text(),
            "fresh text"
        );
        drop(store);
        assert_eq!(
            SessionStore::new(root.path().to_owned())
                .load()
                .unwrap()
                .unwrap()
                .tabs[0]
                .active_buffer()
                .text(),
            "fresh text"
        );
        fs::remove_dir_all(backup).unwrap();
    }

    #[test]
    fn replacing_an_open_store_retires_stale_clones_and_retains_ownership() {
        use super::super::tests::{request, tab};
        use crate::app::services::session_store::SessionStore;
        let root = tempfile::tempdir().unwrap();
        let mut store = SessionStore::new(root.path().to_owned());
        store.persist(&[tab("old text")], 0, 16.0, false).unwrap();
        let stale = store.clone();
        let backup = store
            .start_new_session(request(&[tab("new text")]))
            .unwrap();
        assert!(stale.persist(&[tab("stale text")], 0, 16.0, false).is_err());
        assert!(futures_lite::future::block_on(super::super::Database::open(root.path())).is_err());
        assert_eq!(
            store.load().unwrap().unwrap().tabs[0]
                .active_buffer()
                .text(),
            "new text"
        );
        // The closed original database and WAL can still be opened as a pair.
        let saved = SessionStore::new(backup.clone());
        assert_eq!(
            saved.load().unwrap().unwrap().tabs[0]
                .active_buffer()
                .text(),
            "old text"
        );
        drop(saved);
        fs::remove_dir_all(backup).unwrap();
    }

    #[test]
    fn another_owner_prevents_reset() {
        use super::super::tests::{request, tab};
        use crate::app::services::session_store::SessionStore;
        let root = tempfile::tempdir().unwrap();
        let owner = SessionStore::new(root.path().to_owned());
        owner.persist(&[tab("keep")], 0, 16.0, false).unwrap();
        let mut other = SessionStore::new(root.path().to_owned());
        assert!(other.load().is_err());
        assert!(other.start_new_session(request(&[tab("replace")])).is_err());
        assert_eq!(
            owner.load().unwrap().unwrap().tabs[0]
                .active_buffer()
                .text(),
            "keep"
        );
    }
}
