use super::{BufferFreshness, FileController, SavedBufferFixture, wait_for_background_io};
use crate::app::services::session_store::activity::ActivityKind;

fn observe(f: &mut SavedBufferFixture) {
    FileController::refresh_active_buffer_disk_state(&mut f.app);
    wait_for_background_io(&mut f.app);
}
fn rows(f: &SavedBufferFixture, kind: ActivityKind) -> usize {
    f.app
        .state
        .persistence
        .pending_activity
        .iter()
        .filter(|a| a.kind == kind)
        .count()
}

#[test]
fn clean_and_dirty_observations_deduplicate_versions_and_keep_previous_states() {
    for dirty in [false, true] {
        let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
        if dirty {
            let b = f.app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
            b.document_mut().replace_exact_with_history("local");
            b.is_dirty = true;
        }
        let now = f.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .document()
            .history_now();
        std::fs::write(&f.path, "external version").unwrap();
        observe(&mut f);
        let b = f.app.tab_manager.tabs.as_slice()[0].active_buffer();
        assert_eq!(b.text(), if dirty { "local" } else { "external version" });
        if dirty {
            assert_eq!(b.document().history_now(), now);
            assert_eq!(b.freshness, BufferFreshness::ConflictOnDisk);
            assert_eq!(rows(&f, ActivityKind::DiskConflict), 1);
        } else {
            assert_eq!(
                b.document().history_entries().last().unwrap().source,
                crate::app::domain::buffer::PieceSource::External
            );
            let mut d = b.document().clone();
            d.undo_last_operation().unwrap();
            assert_eq!(d.extract_text(), "saved");
        }
        observe(&mut f);
        assert_eq!(rows(&f, ActivityKind::DiskConflict), usize::from(dirty));
        // Same contents, new metadata, including a synthetic timestamp-only touch.
        f.app.tab_manager.tabs.as_mut_slice()[0]
            .active_buffer_mut()
            .disk_state = None;
        observe(&mut f);
        assert_eq!(rows(&f, ActivityKind::DiskConflict), usize::from(dirty));
        std::fs::write(&f.path, "saved").unwrap();
        observe(&mut f);
        assert_eq!(
            rows(&f, ActivityKind::DiskConflict),
            if dirty { 2 } else { 0 }
        );
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), "saved");
    }
}

#[test]
fn missing_once_then_returned_file_is_reconciled() {
    for dirty in [false, true] {
        let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
        f.app.tab_manager.tabs.as_mut_slice()[0]
            .active_buffer_mut()
            .is_dirty = dirty;
        std::fs::remove_file(&f.path).unwrap();
        observe(&mut f);
        observe(&mut f);
        assert_eq!(rows(&f, ActivityKind::Missing), 1);
        std::fs::write(&f.path, "returned").unwrap();
        observe(&mut f);
        let b = f.app.tab_manager.tabs.as_slice()[0].active_buffer();
        assert!(!b.observed_disk_missing);
        assert_eq!(b.text(), if dirty { "saved" } else { "returned" });
        assert_eq!(rows(&f, ActivityKind::DiskConflict), usize::from(dirty));
    }
}

#[test]
fn completed_own_save_uses_written_snapshot_even_when_local_work_continues() {
    let mut f = SavedBufferFixture::dirty("tracked.txt", "saved", "ours");
    assert!(FileController::save_file_at(&mut f.app, 0));
    let b = f.app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
    b.document_mut().replace_exact_with_history("later edit");
    b.is_dirty = true;
    wait_for_background_io(&mut f.app);
    let b = f.app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert_eq!(
        b.disk_content_digest,
        Some(blake3::hash(b"ours").to_hex().to_string())
    );
    assert!(b.is_dirty);
    observe(&mut f);
    assert_eq!(rows(&f, ActivityKind::DiskConflict), 0);
    assert_eq!(
        f.app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
        "later edit"
    );
}

#[test]
fn superseded_or_forgotten_observations_never_replace_local_text() {
    use crate::app::{
        app_state::{PendingReloadBufferAction, PendingReloadMode},
        services::{background_io::LoadedPathResult, file_service::FileService},
    };
    for forget in [false, true] {
        let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
        let action = PendingReloadBufferAction::new(
            f.app.tab_manager.tabs.as_slice()[0].active_buffer(),
            PendingReloadMode::ObserveDiskChange,
        )
        .unwrap();
        std::fs::write(&f.path, "external").unwrap();
        let observed = FileService::read_observed_file(&f.path).unwrap();
        let state = Some(observed.state.clone());
        let loaded = FileService::buffer_from_observed(&f.path, observed);
        if forget {
            f.app.state.persistence.session_store.invalidate_history();
        } else {
            f.app.tab_manager.tabs.as_mut_slice()[0]
                .active_buffer_mut()
                .observed_disk_digest = Some("newer version".into());
        }
        FileController::apply_async_reload_buffer_result(
            &mut f.app,
            action,
            vec![LoadedPathResult {
                path: f.path.clone(),
                retained_tab: None,
                disk_state: state,
                missing: false,
                result: Ok(loaded),
            }],
        );
        assert_eq!(
            f.app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
            "saved"
        );
        assert_eq!(rows(&f, ActivityKind::DiskConflict), 0);
    }
}

#[test]
fn startup_conflict_is_durable_and_not_recorded_again_on_restart() {
    use crate::app::{app_state::test_support::test_app, services::session_manager};
    let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
    let b = f.app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
    b.document_mut().replace_exact_with_history("local unsaved");
    b.is_dirty = true;
    session_manager::persist_session_now(&mut f.app).unwrap();
    std::fs::write(&f.path, "external").unwrap();
    let root = f._directory.path().to_path_buf();
    drop(f.app);
    let mut app = test_app(&root);
    wait_for_background_io(&mut app);
    assert_eq!(
        app.state
            .persistence
            .pending_activity
            .iter()
            .filter(|e| e.kind == ActivityKind::DiskConflict)
            .count(),
        1
    );
    session_manager::persist_session_now(&mut app).unwrap();
    drop(app);
    let mut app = test_app(&root);
    wait_for_background_io(&mut app);
    let b = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(b.text(), "local unsaved");
    assert_eq!(b.path.as_ref(), Some(&f.path));
    assert_eq!(b.freshness, BufferFreshness::ConflictOnDisk);
    assert!(app.state.persistence.pending_activity.is_empty());
}
