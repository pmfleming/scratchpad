use super::{
    Database, error,
    tests::{database, request, tab},
};
use crate::app::{
    app_state::{test_support, workspace_controller},
    services::{
        file_controller::FileController,
        session_manager,
        session_store::{
            activity::{ActivityEvent, ActivityKind},
            timeline::TimelineKind,
        },
    },
};
use futures_lite::future::block_on;

#[test]
fn activity_alone_commits_and_repeated_captures_are_idempotent() {
    block_on(async {
        let (root, mut db) = database().await;
        let tabs = vec![tab("")];
        db.persist(request(&tabs), 1).await.unwrap();
        let recency = db
            .integer("SELECT edit_seq FROM document_history")
            .await
            .unwrap();
        let event = ActivityEvent::new(ActivityKind::Opened, tabs[0].active_buffer());
        for generation in [2, 3] {
            let mut save = request(&tabs);
            save.activity.push(event.clone());
            let profile = db.persist(save, generation).await.unwrap();
            assert!(profile.manifest_write_ns > 0);
            assert_eq!(profile.snapshot_bytes_read, 0);
            assert_eq!(profile.snapshot_bytes_hashed, 0);
            assert_eq!(
                db.integer("SELECT count(*) FROM activity").await.unwrap(),
                1
            );
            assert_eq!(
                db.integer("SELECT edit_seq FROM document_history")
                    .await
                    .unwrap(),
                recency
            );
        }
        let sequence = event.sequence;
        drop(db);
        let db = Database::open(root.path()).await.unwrap();
        assert!(crate::app::domain::buffer::next_text_history_global_seq() > sequence);
        assert_eq!(
            db.timeline_page(None, vec![], String::new())
                .await
                .unwrap()
                .rows
                .len(),
            1
        );
    });
}

#[test]
fn activity_and_document_changes_roll_back_together_when_a_reference_is_missing() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("before")];
        db.persist(request(&tabs), 1).await.unwrap();
        tabs[0].active_buffer_mut().replace_text("after".into());
        let mut event = ActivityEvent::new(ActivityKind::Opened, tabs[0].active_buffer());
        event.document = "missing".into();
        let mut save = request(&tabs);
        save.activity.push(event);
        assert!(db.persist(save, 2).await.is_err());
        assert_eq!(
            db.integer("SELECT count(*) FROM activity").await.unwrap(),
            0
        );
        assert_eq!(
            db.read_all_contents().await.unwrap()[&tabs[0].active_buffer().temp_id],
            "before"
        );
    });
}

#[test]
fn open_edit_close_reopen_and_restart_keep_one_ordered_feed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, "one").unwrap();
    let mut app = test_support::test_app(root.path());
    FileController::open_paths(&mut app, vec![path.clone()]);
    let document = app
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .temp_id
        .clone();
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer.document_mut().replace_exact_with_history("local");
    buffer.mark_dirty_after_local_edit();
    let index = app.tab_manager.active_tab_index;
    workspace_controller::perform_close_tab_no_persist(&mut app, index);
    let store = app.state.persistence.session_store.clone();
    let rows = store
        .timeline_page(None, vec![], String::new())
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 3);
    assert!(matches!(
        rows[0].kind,
        TimelineKind::Activity(ActivityKind::Closed)
    ));
    assert!(rows[1].node().is_some());
    assert!(matches!(
        rows[2].kind,
        TimelineKind::Activity(ActivityKind::Opened)
    ));
    FileController::open_paths(&mut app, vec![path.clone()]);
    session_manager::persist_session_now(&mut app).unwrap();
    assert_eq!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .temp_id,
        document
    );
    let before = store
        .timeline_page(None, vec![], String::new())
        .unwrap()
        .rows
        .len();
    assert_eq!(before, 4);
    app.set_session_persist_on_drop(true);
    drop(app);
    drop(store);
    let app = test_support::test_app(root.path());
    assert!(app.state.persistence.pending_activity.is_empty());
    assert_eq!(
        app.state
            .persistence
            .session_store
            .timeline_page(None, vec![], String::new())
            .unwrap()
            .rows
            .len(),
        before
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "one");
}

#[test]
fn an_empty_disk_file_has_open_and_close_rows_but_a_blank_untitled_document_leaves_none() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("empty.txt");
    std::fs::write(&path, "").unwrap();
    let mut app = test_support::test_app(root.path());
    FileController::open_paths(&mut app, vec![path]);
    let index = app.tab_manager.active_tab_index;
    workspace_controller::perform_close_tab_no_persist(&mut app, index);
    let rows = app
        .state
        .persistence
        .session_store
        .timeline_page(None, vec![], String::new())
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.node().is_none()));
    workspace_controller::new_tab(&mut app);
    let document = app
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .temp_id
        .clone();
    assert!(
        app.state
            .persistence
            .pending_activity
            .iter()
            .any(|e| e.document == document && e.kind == ActivityKind::Created)
    );
    let index = app.tab_manager.active_tab_index;
    workspace_controller::perform_close_tab_no_persist(&mut app, index);
    assert!(
        !app.state
            .persistence
            .pending_activity
            .iter()
            .any(|e| e.document == document)
    );
    app.state
        .persistence
        .session_store
        .worker()
        .unwrap()
        .call(move |db| {
            block_on(async {
                assert!(
                    db.query_collect("SELECT id FROM documents WHERE id=?", [document], |r| r
                        .get::<String>(0)
                        .map_err(error))
                        .await?
                        .is_empty()
                );
                Ok(())
            })
        })
        .unwrap();
}

#[test]
fn failed_close_activity_commit_keeps_the_document_open_and_pending_work_retryable() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, "one").unwrap();
    let mut app = test_support::test_app(root.path());
    FileController::open_paths(&mut app, vec![path]);
    let store = app.state.persistence.session_store.clone();
    store.worker().unwrap().call(|db|block_on(async {
        db.conn.execute_batch("CREATE TRIGGER refuse_closed BEFORE INSERT ON activity WHEN new.kind='closed' BEGIN SELECT RAISE(ABORT,'injected close failure'); END;").await.map_err(error)
    })).unwrap();
    let count = app.tab_manager.tabs.len();
    let index = app.tab_manager.active_tab_index;
    workspace_controller::perform_close_tab_no_persist(&mut app, index);
    assert_eq!(app.tab_manager.tabs.len(), count);
    assert_eq!(app.state.persistence.pending_activity.len(), 1);
    store
        .worker()
        .unwrap()
        .call(|db| {
            block_on(async {
                db.conn
                    .execute("DROP TRIGGER refuse_closed", ())
                    .await
                    .map_err(error)?;
                Ok(())
            })
        })
        .unwrap();
    workspace_controller::perform_close_tab_no_persist(&mut app, index);
    assert_eq!(app.tab_manager.tabs.len(), count - 1);
    assert!(app.state.persistence.pending_activity.is_empty());
    assert_eq!(
        store
            .timeline_page(None, vec![], String::new())
            .unwrap()
            .rows
            .len(),
        2
    );
}

#[test]
fn closing_a_second_view_does_not_close_the_document() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, "one").unwrap();
    let mut app = test_support::test_app(root.path());
    FileController::open_paths(&mut app, vec![path]);
    let view = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .split_active_view(crate::app::domain::SplitAxis::Vertical)
        .unwrap();
    session_manager::persist_session_now(&mut app).unwrap();
    crate::app::commands::perform_close_view(&mut app, view);
    assert_eq!(app.tab_manager.active_tab().unwrap().layout.leaf_count(), 1);
    let rows = app
        .state
        .persistence
        .session_store
        .timeline_page(None, vec![], String::new())
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0].kind,
        TimelineKind::Activity(ActivityKind::Opened)
    ));
}

#[test]
fn closing_a_cold_blank_untitled_document_removes_its_creation_without_hydrating() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_support::test_app(root.path());
    workspace_controller::new_tab(&mut app);
    session_manager::persist_session_now(&mut app).unwrap();
    let index = app.tab_manager.active_tab_index;
    let cold = crate::app::services::session_store::cold_tab_from_workspace_tab(
        &app.tab_manager.tabs.as_slice()[index],
    );
    app.tab_manager.set_cold_session_tab(index, cold);
    workspace_controller::perform_close_tab_no_persist(&mut app, index);
    assert!(
        app.state
            .persistence
            .session_store
            .timeline_page(None, vec![], String::new())
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(!app.state.background_io.has_pending_session_hydration());
}
