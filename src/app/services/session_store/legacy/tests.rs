use super::*;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::session_store::{SessionActiveSurface, SessionStore, session_manifest};

fn fixture(root: &Path, text: &str) -> String {
    fs::create_dir_all(root).unwrap();
    let tab = WorkspaceTab::new(BufferState::new("legacy.txt".into(), text.into(), None));
    let id = tab.active_buffer().temp_id.clone();
    let capture = SessionPersistRequest::capture(&[tab], 0, 17.0, false);
    let tabs = capture
        .tabs
        .into_iter()
        .map(|tab| tab.session_tab)
        .collect();
    let manifest = session_manifest(0, SessionActiveSurface::Workspace, 17.0, false, tabs);
    fs::write(
        root.join("session.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(root.join(format!("{id}.tmp")), text).unwrap();
    id
}

#[test]
fn import_is_atomic_preserves_originals_and_is_not_repeated() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let id = fixture(root, "original\0λ\r\n");
    let manifest = fs::read(root.join("session.json")).unwrap();
    let store = SessionStore::new(root.to_owned());
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs[0].active_buffer().text(), "original\0λ\r\n");
    assert_eq!(restored.legacy_settings.editor.font_size, 17.0);
    assert_eq!(fs::read(root.join("session.json")).unwrap(), manifest);
    assert_eq!(
        fs::read_to_string(root.join(format!("{id}.tmp"))).unwrap(),
        "original\0λ\r\n"
    );
    store.persist(&[], 0, 16.0, false).unwrap();
    drop(store);
    assert!(SessionStore::new(root.to_owned()).load().unwrap().is_none());
}

#[test]
fn failed_import_protects_data_and_retries_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let id = fixture(directory.path(), "recover me");
    let snapshot = directory.path().join(format!("{id}.tmp"));
    fs::remove_file(&snapshot).unwrap();
    let store = SessionStore::new(directory.path().to_owned());
    assert!(store.load().is_err());
    assert!(store.persist(&[], 0, 16.0, false).is_err());
    fs::write(snapshot, "recover me").unwrap();
    drop(store);
    let restored = SessionStore::new(directory.path().to_owned())
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(restored.tabs[0].active_buffer().text(), "recover me");
}

#[test]
fn primary_manifest_precedes_fallback_even_when_corrupt() {
    let directory = tempfile::tempdir().unwrap();
    let primary = directory.path().join("primary");
    let fallback = directory.path().join("fallback");
    fixture(&fallback, "fallback");
    fs::create_dir(&primary).unwrap();
    fs::write(primary.join("session.json"), "{malformed").unwrap();
    let store = SessionStore::with_fallback(primary.clone(), fallback.clone());
    assert!(store.load().is_err());
    drop(store);
    fs::remove_file(primary.join("session.json")).unwrap();
    let restored = SessionStore::with_fallback(primary, fallback)
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(restored.tabs[0].active_buffer().text(), "fallback");
}

#[test]
fn legacy_view_control_flag_survives_import() {
    let directory = tempfile::tempdir().unwrap();
    fixture(directory.path(), "a\u{1}b");
    let path = directory.path().join("session.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["tabs"][0]["views"][0]["show_control_chars"] = true.into();
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let restored = SessionStore::new(directory.path().to_owned())
        .load()
        .unwrap()
        .unwrap();
    assert!(restored.tabs[0].active_buffer().show_control_chars);
}

#[test]
fn traversal_identifier_and_newer_manifest_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    fixture(directory.path(), "safe");
    let path = directory.path().join("session.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["version"] = 999.into();
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        SessionStore::new(directory.path().to_owned())
            .persist(&[], 0, 16.0, false)
            .is_err()
    );
    manifest["version"] = 7.into();
    manifest["tabs"][0]["buffers"][0]["temp_id"] = "../outside".into();
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        SessionStore::new(directory.path().to_owned())
            .load()
            .is_err()
    );
    assert!(path.exists());
}
