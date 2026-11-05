//! Explicit fresh start with private backup readers and atomic live-reference rebinding.
use super::{DatabaseWorker, create_private_directory, error, private_file};
use crate::app::services::session_store::SessionPersistRequest;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SESSION_FILES: [&str; 5] = [
    "session.db-wal",
    "session.db-shm",
    "session.db-tshm",
    "session.db-journal",
    "session.db",
];

pub(in crate::app::services::session_store) fn start_new_session(
    root: &Path,
    worker: Option<&DatabaseWorker>,
    mut request: SessionPersistRequest,
) -> io::Result<(DatabaseWorker, PathBuf)> {
    if let Some(worker) = worker
        && request
            .tabs
            .iter()
            .chain(request.closed_tabs.iter().map(|closed| &closed.tab))
            .any(|tab| tab.preserve_existing)
    {
        request = worker.prepare_reset_request(request)?;
    }
    let old = worker.map(DatabaseWorker::chunk_store);
    let previous_path = old.as_ref().and_then(|store| store.epoch().path());
    let ownership = if let Some(worker) = worker {
        match worker.close_for_replacement() {
            Ok(file) => file,
            Err(cause) => {
                if !worker.writes_blocked() {
                    return Err(cause);
                }
                let file = private_file(&root.join("session.db.lock"))?;
                file.try_lock().map_err(error)?;
                if let Some(old) = &old {
                    old.close_readers();
                }
                file
            }
        }
    } else {
        create_private_directory(root)?;
        let ownership = private_file(&root.join("session.db.lock"))?;
        ownership.try_lock().map_err(error)?;
        ownership
    };
    let backup = match backup_session_files(root, &std::env::temp_dir()) {
        Ok(backup) => backup,
        Err(cause) => {
            if let Some(old) = &old
                && let Some(path) = &previous_path
            {
                let _ = old.epoch().reopen(path);
            }
            return Err(cause);
        }
    };
    if let Some(old) = &old {
        let path = previous_path
            .as_ref()
            .filter(|path| **path != root.join("session.db"))
            .cloned()
            .unwrap_or_else(|| backup.join("session.db"));
        old.epoch().reopen(&path)?;
    }
    let result = (|| {
        let worker = DatabaseWorker::open_owned(root.to_owned(), ownership)?;
        let mut copy = crate::app::services::chunk_store::Rebind::new(worker.chunk_store());
        if let Some(old) = &old {
            copy.copy_live(old)?;
        }
        request.copy_to_store(&mut copy)?;
        worker.call(move |db| {
            let generation = request.generation;
            futures_lite::future::block_on(db.persist(request, generation)).map(|_| ())
        })?;
        copy.publish()?;
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

#[cfg(test)]
mod live_text_tests {
    use super::*;
    use crate::app::domain::{BufferState, CursorRange, WorkspaceTab};
    use crate::app::services::session_store::{SessionPersistRequest, SessionStore};
    #[test]
    fn reset_rebinds_large_live_and_history_only_chunks_and_keeps_old_clones_readable() {
        let root = tempfile::tempdir().unwrap();
        let mut session = SessionStore::new(root.path().to_owned());
        let store = session.chunk_store_handle().unwrap();
        let mut tab = WorkspaceTab::new(BufferState::new("live".into(), "base".into(), None));
        tab.active_buffer_mut()
            .document_mut()
            .bind_store(store.clone())
            .unwrap();
        let text = "λ🦀\n".repeat(100_000);
        tab.active_buffer_mut()
            .document_mut()
            .replace_char_ranges_with_undo(
                &[(0..0, text.clone())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        session
            .persist(std::slice::from_ref(&tab), 0, 14., false)
            .unwrap();
        let old = tab.clone();
        let old_epoch = store.epoch().id;
        let backup = session
            .start_new_session(SessionPersistRequest::capture(
                std::slice::from_ref(&tab),
                0,
                14.,
                false,
            ))
            .unwrap();
        let fresh = session.chunk_store_handle().unwrap();
        assert_ne!(fresh.epoch().id, old_epoch);
        tab.active_buffer_mut()
            .document_mut()
            .bind_store(fresh)
            .unwrap();
        assert_eq!(old.active_buffer().text(), format!("{text}base"));
        let document = tab.active_buffer_mut().document_mut();
        document.undo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "base");
        document.redo_last_operation().unwrap();
        assert_eq!(document.extract_text(), format!("{text}base"));
        session
            .persist(std::slice::from_ref(&tab), 0, 14., false)
            .unwrap();
        assert_eq!(
            session.load().unwrap().unwrap().tabs[0]
                .active_buffer()
                .text(),
            format!("{text}base")
        );
        drop(old);
        drop(tab);
        drop(session);
        fs::remove_dir_all(backup).unwrap();
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use crate::app::domain::{BufferState, WorkspaceTab};
    use crate::app::services::session_store::{SessionPersistRequest, SessionStore};
    #[test]
    fn failed_backup_keeps_evicted_text_readable_and_reset_can_be_retried() {
        let root = tempfile::tempdir().unwrap();
        let mut session = SessionStore::new(root.path().to_owned());
        let handle = session.chunk_store_handle().unwrap();
        let source = root.path().join("source");
        fs::write(&source, "λ🦀\n".repeat(60_000)).unwrap();
        let (document, _) = crate::app::domain::TextDocument::import_utf8_file(
            &source,
            0,
            &crate::app::services::chunk_store::ImportContext {
                store: Some(handle.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut tab = WorkspaceTab::new(BufferState::new("live".into(), String::new(), None));
        *tab.active_buffer_mut().document_mut() = document;
        session
            .persist(std::slice::from_ref(&tab), 0, 14., false)
            .unwrap();
        handle.set_limits(0, 0);
        fs::create_dir(root.path().join("session.db-journal")).unwrap();
        assert!(
            session
                .start_new_session(SessionPersistRequest::capture(
                    std::slice::from_ref(&tab),
                    0,
                    14.,
                    false
                ))
                .is_err()
        );
        assert!(session.storage_unavailable());
        assert!(tab.active_buffer().document().try_content_digest().is_ok());
        fs::remove_dir(root.path().join("session.db-journal")).unwrap();
        let backup = session
            .start_new_session(SessionPersistRequest::capture(
                std::slice::from_ref(&tab),
                0,
                14.,
                false,
            ))
            .unwrap();
        tab.active_buffer_mut()
            .document_mut()
            .bind_store(session.chunk_store_handle().unwrap())
            .unwrap();
        assert_eq!(
            tab.active_buffer().document().try_content_digest().unwrap(),
            blake3::hash(&fs::read(source).unwrap()).to_hex().as_str()
        );
        drop(tab);
        drop(session);
        fs::remove_dir_all(backup).unwrap();
    }
}
