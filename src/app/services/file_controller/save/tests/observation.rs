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

#[test]
fn timestamp_only_touch_is_ignored_but_same_size_new_contents_are_observed() {
    use std::time::{Duration, UNIX_EPOCH};
    let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
    let b = f
        .app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    b.document_mut().replace_exact_with_history("local");
    b.is_dirty = true;
    let touch = |path: &std::path::Path, secs| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(secs)),
            )
            .unwrap()
    };
    touch(&f.path, 1_700_000_000);
    observe(&mut f);
    assert_eq!(rows(&f, ActivityKind::DiskConflict), 0);
    std::fs::write(&f.path, "other").unwrap();
    touch(&f.path, 1_700_000_002);
    observe(&mut f);
    assert_eq!(rows(&f, ActivityKind::DiskConflict), 1);
    assert_eq!(
        f.app
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .text(),
        "local"
    );
}

#[test]
fn explicit_reload_of_saved_text_discards_local_work_without_an_external_label() {
    let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
    let b = f
        .app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    b.document_mut().replace_exact_with_history("local");
    b.is_dirty = true;
    assert!(FileController::reload_buffer_from_disk(&mut f.app, 0));
    wait_for_background_io(&mut f.app);
    let b = f.app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(b.text(), "saved");
    assert_eq!(
        b.document().history_entries().last().unwrap().source,
        crate::app::domain::buffer::PieceSource::Load
    );
    let mut doc = b.document().clone();
    doc.undo_last_operation().unwrap();
    assert_eq!(doc.extract_text(), "local");
}

#[test]
fn deferred_save_survives_superseded_observations_or_reports_failure() {
    use crate::app::{
        app_state::{DeferredSave, PendingReloadBufferAction, PendingReloadMode},
        services::{background_io::LoadedPathResult, file_service::FileService},
    };
    for case in ["epoch", "digest", "baseline", "renamed", "error", "empty"] {
        let mut f = SavedBufferFixture::new("tracked.txt", "saved", "saved");
        let buffer = f.app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
        buffer.document_mut().replace_exact_with_history("local");
        buffer.is_dirty = true;
        let mut action = PendingReloadBufferAction::new(
            f.app.tab_manager.tabs.as_slice()[0].active_buffer(),
            PendingReloadMode::ObserveDiskChange,
        )
        .unwrap();
        action.expected_epoch = f.app.state.persistence.session_store.history_epoch();
        action.resume_save = Some(DeferredSave {
            encoding: Some("UTF-8".into()),
        });
        let observed = FileService::read_observed_file(&f.path).unwrap();
        let state = Some(observed.state.clone());
        let loaded = FileService::buffer_from_observed(&f.path, observed);
        match case {
            "epoch" => {
                f.app.state.persistence.session_store.invalidate_history();
            }
            "digest" => action.expected_observed_digest = Some("obsolete observation".into()),
            "baseline" => {
                f.app.tab_manager.tabs.as_mut_slice()[0]
                    .active_buffer_mut()
                    .disk_state = None
            }
            "renamed" => f.app.tab_manager.tabs.as_mut_slice()[0]
                .active_buffer_mut()
                .set_path(Some(f.path.with_extension("renamed"))),
            _ => {}
        }
        let results = if case == "empty" {
            vec![]
        } else {
            vec![LoadedPathResult {
                path: f.path.clone(),
                retained_tab: None,
                disk_state: state,
                missing: false,
                result: if case == "error" {
                    Err("temporary read failure".into())
                } else {
                    Ok(loaded)
                },
            }]
        };
        FileController::apply_async_reload_buffer_result(&mut f.app, action, results);
        wait_for_background_io(&mut f.app);
        if matches!(case, "renamed" | "error") {
            assert_eq!(std::fs::read_to_string(&f.path).unwrap(), "saved", "{case}");
            assert!(
                f.app
                    .state
                    .status
                    .current
                    .as_ref()
                    .unwrap()
                    .text
                    .contains("did not complete"),
                "{case}"
            );
        } else {
            assert_eq!(std::fs::read_to_string(&f.path).unwrap(), "local", "{case}");
            assert!(
                !f.app.tab_manager.tabs.as_slice()[0]
                    .active_buffer()
                    .is_dirty,
                "{case}"
            );
        }
    }
}
