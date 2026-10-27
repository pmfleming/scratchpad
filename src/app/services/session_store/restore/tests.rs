use super::super::{
    SessionActiveSurface, SessionPersistRequest, SessionStore, model::SessionBuffer,
};
use super::{RestoreSummary, Restorer};
use crate::app::domain::{BufferFreshness, BufferState, WorkspaceTab};
use crate::app::services::file_service::FileService;
use std::{collections::HashMap, fs, io};

#[test]
fn disk_reconciliation_preserves_dirty_edits_but_reloads_clean_files() {
    use BufferFreshness::{ConflictOnDisk, InSync, MissingOnDisk};
    for (session, disk, dirty, known_disk, expected_text, expected_dirty, freshness) in [
        ("same", Some("same"), true, false, "same", false, InSync),
        ("session", Some("disk"), true, true, "session", true, InSync),
        (
            "session",
            Some("disk"),
            true,
            false,
            "session",
            true,
            ConflictOnDisk,
        ),
        ("session", Some("disk"), false, false, "disk", false, InSync),
        ("session", None, true, false, "session", true, MissingOnDisk),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        if let Some(text) = disk {
            fs::write(&path, text).unwrap();
        }
        let mut live = BufferState::new("note.txt".into(), session.into(), Some(path.clone()));
        live.is_dirty = dirty;
        if known_disk {
            live.sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
        }
        let mut buffer = SessionBuffer::from(&live);
        buffer.is_dirty = dirty;
        let mut restorer = Restorer::new(HashMap::from([(buffer.temp_id.clone(), session.into())]));
        let restored = restorer.restore_buffer_content(&mut buffer);
        assert_eq!(
            restored.document.extract_text(),
            expected_text,
            "{session}/{disk:?}"
        );
        assert_eq!(
            (restored.is_dirty, restored.freshness),
            (expected_dirty, freshness),
            "{session}/{disk:?}"
        );
    }
}

#[test]
fn restore_status_reports_counts_and_prioritizes_warnings() {
    for (reloaded, conflicts, missing, expected) in [
        (0, 0, 0, None),
        (
            0,
            1,
            2,
            Some("Session restore found 1 disk conflict and 2 missing files."),
        ),
        (
            3,
            2,
            1,
            Some("Session restore found 2 disk conflicts and 1 missing file."),
        ),
        (
            1,
            0,
            0,
            Some("Reloaded 1 clean file from disk during session restore."),
        ),
        (
            2,
            0,
            0,
            Some("Reloaded 2 clean files from disk during session restore."),
        ),
    ] {
        let summary = RestoreSummary {
            reloaded_clean_buffers: reloaded,
            conflicted_buffers: conflicts,
            missing_buffers: missing,
        };
        assert_eq!(
            summary.into_status().map(|status| status.message),
            expected.map(str::to_owned)
        );
    }
}

#[test]
fn streaming_restores_active_first_preserves_cold_payloads_and_honors_cancellation() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path().to_owned());
    let tabs: Vec<_> = [
        ("first.txt", "first original"),
        ("second.txt", "second original"),
    ]
    .into_iter()
    .map(|(name, text)| WorkspaceTab::new(BufferState::new(name.into(), text.into(), None)))
    .collect();
    store.persist(&tabs, 1, 16.0, false).unwrap();
    let visible = store.load_startup_visible().unwrap().unwrap();
    assert_eq!(visible.active_tab_index, 0);
    assert_eq!(visible.tabs.len(), 1);
    assert_eq!(visible.tabs[0].active_buffer().name, "second.txt");

    let mut started = false;
    let mut streamed = Vec::new();
    let mut cold = HashMap::new();
    let metadata = store
        .load_streaming(
            |active, _, _| {
                started = active == 1;
                true
            },
            |index, tab, payload| {
                if let Some(payload) = payload {
                    cold.insert(index, payload);
                }
                streamed.push((index, tab));
                true
            },
        )
        .unwrap()
        .unwrap();
    assert!(started);
    assert_eq!(streamed.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [1, 0]);
    assert_eq!(cold.keys().copied().collect::<Vec<_>>(), [0]);
    assert_eq!(metadata.active_tab_index, 1);
    assert!(metadata.tabs.is_empty());
    assert_eq!(streamed[0].1.active_buffer().name, "second.txt");
    assert_eq!(streamed[1].1.active_buffer().name, "first.txt");
    streamed.sort_by_key(|(index, _)| *index);
    let streamed: Vec<_> = streamed.into_iter().map(|(_, tab)| tab).collect();
    store
        .persist_request(SessionPersistRequest::capture_with_cold_tabs(
            &streamed,
            &cold,
            1,
            SessionActiveSurface::Workspace,
            16.0,
            false,
        ))
        .unwrap();
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), 2);
    for (restored, original) in restored.tabs.iter().zip(&tabs) {
        assert_eq!(restored.active_buffer().name, original.active_buffer().name);
        assert_eq!(
            restored.active_buffer().text(),
            original.active_buffer().text()
        );
    }

    for cancel_at_start in [true, false] {
        let mut callbacks = 0;
        let error = store
            .load_streaming(
                |_, _, _| !cancel_at_start,
                |_, _, _| {
                    callbacks += 1;
                    false
                },
            )
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(callbacks, usize::from(!cancel_at_start));
    }
    assert_eq!(store.load().unwrap().unwrap().tabs.len(), 2);
}
