use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use std::fs;
use std::process::Command;

fn tab(text: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new("recovered".into(), text.into(), None))
}

#[test]
fn export_is_private_readable_and_never_overwrites() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().join("source"));
    store
        .persist(&[tab("λ\0🦀\r\n"), tab("")], 1, 18.0, true)
        .unwrap();
    let destination = directory.path().join("backup");
    store.export_recovery(&destination).unwrap();
    assert!(!destination.join("session.db").exists());
    let original = fs::read(destination.join("session.json")).unwrap();
    assert!(store.export_recovery(&destination).is_err());
    assert_eq!(
        fs::read(destination.join("session.json")).unwrap(),
        original
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(destination.join("session.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(manifest["active_tab_index"], 1);
    for (index, expected) in ["λ\0🦀\r\n", ""].into_iter().enumerate() {
        let id = manifest["tabs"][index]["buffers"][0]["temp_id"]
            .as_str()
            .unwrap();
        assert_eq!(
            fs::read_to_string(destination.join(format!("{id}.tmp"))).unwrap(),
            expected
        );
    }
    // Recovery export is a human-readable escape hatch, not a legacy importer.
    assert!(
        SessionStore::new(destination.clone())
            .load()
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fs::read(destination.join("session.json")).unwrap(),
        original
    );
}

#[test]
fn empty_committed_session_exports_but_corrupt_source_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().join("source"));
    store.persist(&[], 0, 16.0, false).unwrap();
    let destination = directory.path().join("empty");
    store.export_recovery(&destination).unwrap();
    assert!(SessionStore::new(destination).load().unwrap().is_none());
    fs::create_dir(directory.path().join("bad")).unwrap();
    fs::write(directory.path().join("bad/session.db"), b"broken database").unwrap();
    let bad = SessionStore::new(directory.path().join("bad"));
    let destination = directory.path().join("not-created");
    assert!(bad.export_recovery(&destination).is_err());
    assert!(!destination.exists());
}

#[test]
fn command_refuses_a_running_instance_then_exports_after_it_closes() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let store = SessionStore::new(source.clone());
    store
        .persist(&[tab("offline recovery")], 0, 16.0, false)
        .unwrap();
    drop(store);
    let lock = fs::File::create(source.join("instance.lock")).unwrap();
    lock.lock().unwrap();
    let destination = directory.path().join("export");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_session_backup"))
            .arg(&source)
            .arg(&destination)
            .output()
            .unwrap()
    };
    assert!(!run().status.success());
    assert!(!destination.exists());
    drop(lock);
    assert!(run().status.success());
    assert!(destination.join("session.json").exists());
}

#[test]
fn bounded_parallel_restore_preserves_all_tab_order_and_content() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_owned());
    let tabs: Vec<_> = (0..1000)
        .map(|index| tab(&format!("unique text {index} λ")))
        .collect();
    store.persist(&tabs, 733, 16.0, false).unwrap();
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), tabs.len());
    assert_eq!(restored.active_tab_index, 733);
    for (index, restored) in restored.tabs.iter().enumerate() {
        assert_eq!(
            restored.buffers.buffer.text(),
            format!("unique text {index} λ")
        );
    }
}
