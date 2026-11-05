use super::super::{SessionActiveSurface, SessionPersistRequest, SessionStore};
use super::RestoreSummary;
use crate::app::domain::{BufferFreshness, BufferState, WorkspaceTab};
use std::{collections::HashMap, io};

#[test]
fn restore_status_reports_real_freshness_and_aggregates_batches() {
    for (conflicts, missing, expected) in [
        (0, 0, None),
        (
            1,
            2,
            Some("Session restore found 1 disk conflict and 2 missing files."),
        ),
        (
            2,
            1,
            Some("Session restore found 2 disk conflicts and 1 missing file."),
        ),
    ] {
        let mut summary = RestoreSummary::default();
        for _ in 0..conflicts {
            summary.record(BufferFreshness::ConflictOnDisk);
        }
        let other = RestoreSummary {
            conflicted_buffers: 0,
            missing_buffers: missing,
        };
        summary.merge(other);
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
            |active, _| {
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
                |_, _| !cancel_at_start,
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

#[test]
fn reference_restore_preserves_committed_text_and_rejects_missing_documents() {
    use super::buffer_content::RestoredBufferContent;
    use crate::app::services::session_store::model::SessionBuffer;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.txt");
    let buffer = BufferState::new(
        "source.txt".into(),
        "saved λ\r\n".into(),
        Some(path.clone()),
    );
    for (missing, conflict, dirty) in [
        (false, false, false),
        (false, true, true),
        (true, false, false),
    ] {
        std::fs::write(&path, "external change").unwrap();
        if missing {
            std::fs::remove_file(&path).unwrap();
        }
        let mut metadata = SessionBuffer::capture(&buffer);
        metadata.is_dirty = dirty;
        metadata.disk_len = Some(10);
        metadata.disk_content_digest = Some("saved digest".into());
        metadata.observed_disk_digest = Some(
            if conflict {
                "changed digest"
            } else {
                "saved digest"
            }
            .into(),
        );
        metadata.history_document = Some(buffer.document().clone());
        let restored = RestoredBufferContent::from_references(&mut metadata).unwrap();
        assert_eq!(restored.document.extract_text(), "saved λ\r\n");
        assert_eq!(restored.is_dirty, dirty || missing);
        assert_eq!(
            restored.freshness,
            if missing {
                BufferFreshness::MissingOnDisk
            } else if conflict {
                BufferFreshness::ConflictOnDisk
            } else {
                BufferFreshness::InSync
            }
        );
        assert_eq!(metadata.pending_disk_observation, missing);
        assert_eq!(
            RestoredBufferContent::from_references(&mut metadata)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
