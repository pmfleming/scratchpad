use super::*;
use crate::app::services::session_store::ops::BUFFER_FILE_EXTENSION;
use std::fs;

#[test]
fn stale_dirty_flag_clears_when_session_text_matches_disk() {
    let (store, buffer, _path) = restore_fixture("same", "same", true, None);

    let restored = store.restore_buffer_content(&buffer);

    assert!(!restored.is_dirty);
    assert_eq!(restored.freshness, BufferFreshness::InSync);
}

#[test]
fn dirty_session_text_stays_dirty_when_disk_has_not_changed() {
    let disk_text = "disk";
    let (store, mut buffer, _path) = restore_fixture("session", disk_text, true, None);
    buffer.disk_len = Some(disk_text.len() as u64);
    buffer.disk_modified_millis = restored_disk_state(&store, &buffer).modified_millis;

    let restored = store.restore_buffer_content(&buffer);

    assert!(restored.is_dirty);
    assert_eq!(restored.freshness, BufferFreshness::InSync);
}

#[test]
fn dirty_session_text_conflicts_when_disk_changed() {
    let (store, buffer, _path) = restore_fixture("session", "disk", true, None);

    let restored = store.restore_buffer_content(&buffer);

    assert!(restored.is_dirty);
    assert_eq!(restored.freshness, BufferFreshness::ConflictOnDisk);
}

fn restore_fixture(
    session_text: &str,
    disk_text: &str,
    is_dirty: bool,
    session_disk_len: Option<u64>,
) -> (SessionStore, SessionBuffer, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().expect("create temp session root");
    let root = temp_dir.keep();
    fs::create_dir_all(&root).expect("create session root");
    let path = root.join("note.txt");
    fs::write(&path, disk_text).expect("write disk file");

    let temp_id = "buffer-test".to_owned();
    fs::write(
        root.join(format!("{temp_id}.{BUFFER_FILE_EXTENSION}")),
        session_text,
    )
    .expect("write session snapshot");

    let buffer = SessionBuffer {
        id: 1,
        name: "note.txt".to_owned(),
        path: Some(path.clone()),
        is_dirty,
        is_settings_file: false,
        show_control_chars: false,
        right_to_left_reading_order: false,
        temp_id,
        format: None,
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_modified_millis: None,
        disk_len: session_disk_len,
        text_history: Vec::new(),
    };

    (SessionStore::new(root), buffer, path)
}

fn restored_disk_state(store: &SessionStore, buffer: &SessionBuffer) -> DiskFileState {
    FileService::read_disk_state(buffer.path.as_ref().expect("fixture path")).unwrap_or_else(|_| {
        panic!(
            "read disk state from {}",
            store.root().join("note.txt").display()
        )
    })
}
