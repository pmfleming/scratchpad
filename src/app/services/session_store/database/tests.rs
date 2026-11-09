use super::{Database, DatabaseWorker, ReadScope};
use crate::app::domain::{BufferState, CharCursor, CursorRange, SplitAxis, WorkspaceTab};
use crate::app::services::session_store::{SessionPersistRequest, model::SessionPaneNode};
use futures_lite::future::block_on;

pub(super) fn tab(text: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new("note".into(), text.into(), None))
}
pub(super) fn request(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    let mut request = SessionPersistRequest::capture(tabs, 0, 16.0, false);
    // Materialize test fixtures explicitly so malformed-row tests can alter them.
    for buffer in request
        .tabs
        .iter_mut()
        .flat_map(|tab| &mut tab.session_tab.buffers)
    {
        if let Some(capture) = buffer.history_capture.take() {
            buffer.text_history = (0..capture.len())
                .map(|i| capture.entry(i).unwrap())
                .collect();
        }
    }
    request
}
pub(super) fn insert(tab: &mut WorkspaceTab, at: usize, text: &str) {
    tab.buffers
        .buffer
        .document_mut()
        .replace_char_ranges_with_undo(
            &[(at..at, text.into())],
            CursorRange::one(CharCursor::new(at)),
            CursorRange::one(CharCursor::new(at + text.chars().count())),
        )
        .unwrap();
}
pub(super) async fn database() -> (tempfile::TempDir, Database) {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).await.unwrap();
    (root, db)
}

#[test]
fn writer_retries_a_busy_transaction_until_the_other_connection_releases_it() {
    block_on(async {
        let (_root, db) = database().await;
        let other = db.database.connect().unwrap();
        other.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let mut begin = std::pin::pin!(db.conn.execute("BEGIN IMMEDIATE", ()));
        assert!(futures_lite::future::poll_once(&mut begin).await.is_none());
        other.execute("ROLLBACK", ()).await.unwrap();
        begin.await.unwrap();
        db.conn.execute("ROLLBACK", ()).await.unwrap();
        assert!(db.conn.is_autocommit().unwrap());
    });
}

#[test]
fn writer_busy_wait_is_bounded_and_can_be_retried_after_timeout() {
    block_on(async {
        let (_root, db) = database().await;
        let other = db.database.connect().unwrap();
        other.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let started = std::time::Instant::now();
        let result = db.conn.execute("BEGIN IMMEDIATE", ()).await;
        let elapsed = started.elapsed();
        assert!(matches!(result, Err(turso::Error::Busy(_))), "{result:?}");
        assert!(
            elapsed
                >= super::WRITER_BUSY_TIMEOUT.saturating_sub(std::time::Duration::from_millis(10)),
            "busy wait returned too soon: {elapsed:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "busy wait exceeded its bound: {elapsed:?}"
        );
        assert!(db.conn.is_autocommit().unwrap());
        other.execute("ROLLBACK", ()).await.unwrap();
        db.conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        db.conn.execute("ROLLBACK", ()).await.unwrap();
    });
}

#[test]
fn profiled_session_subdivisions_fit_their_parent_intervals() {
    use crate::app::services::session_store::SessionStore;
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_owned());
    let mut tabs = vec![tab("Unicode λ\n"), tab("second")];
    insert(&mut tabs[0], 0, "edited ");
    let saved = store.persist_profiled(&tabs, 0, 16.0, false).unwrap();
    assert!(saved.transaction_committed);
    assert!(saved.history_prepare_ns + saved.content_prepare_ns <= saved.manifest_serialize_ns);
    assert!(saved.history_write_ns > 0 && saved.content_write_ns > 0);
    assert!(saved.history_write_ns + saved.content_write_ns <= saved.snapshot_write_ns);
    assert_eq!(
        saved.layout_write_ns + saved.commit_ns,
        saved.manifest_write_ns
    );
    let loaded = store.load_profiled().unwrap();
    let profile = loaded.profile;
    assert!(profile.orphan_validation_ns > 0 && profile.history_read_ns > 0);
    assert!(
        profile.orphan_validation_ns
            + profile.layout_read_ns
            + profile.content_validation_ns
            + profile.history_read_ns
            <= profile.manifest_read_parse_ns
    );
    assert_eq!(loaded.restored.unwrap().tabs.len(), 2);
    let unchanged = store.persist_profiled(&tabs, 0, 16.0, false).unwrap();
    assert!(!unchanged.transaction_committed);
    assert_eq!(
        unchanged.history_write_ns
            + unchanged.content_write_ns
            + unchanged.layout_write_ns
            + unchanged.commit_ns,
        0
    );
}

#[test]
fn worker_round_trip_and_stale_capture_barrier() {
    let directory = tempfile::tempdir().unwrap();
    let worker = DatabaseWorker::open(directory.path().to_owned()).unwrap();
    let old = request(&[tab("old")]);
    let new = request(&[tab("new\0\r\nλ")]);
    worker.call(move |db| block_on(db.persist(new, 2))).unwrap();
    worker.call(move |db| block_on(db.persist(old, 1))).unwrap();
    let session = worker
        .call(|db| block_on(db.load(ReadScope::All)))
        .unwrap()
        .unwrap();
    assert_eq!(
        session.text_contents().values().next().unwrap(),
        "new\0\r\nλ"
    );
    worker.call(|db| block_on(db.checkpoint())).unwrap();
    drop(worker);
    let reopened = DatabaseWorker::open(directory.path().to_owned()).unwrap();
    assert_eq!(
        reopened
            .call(|db| block_on(db.load(ReadScope::Active)))
            .unwrap()
            .unwrap()
            .text_contents(),
        session.text_contents()
    );
}

#[test]
fn unchanged_and_metadata_only_commits_never_rewrite_content_or_history() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("unchanged")];
        insert(&mut tabs[0], 0, "history");
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn.execute_batch("CREATE TRIGGER no_content_write BEFORE INSERT ON chunk_bodies BEGIN SELECT RAISE(ABORT, 'content rewritten'); END;
            CREATE TRIGGER no_history_write BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT, 'history rewritten'); END;
            CREATE TRIGGER no_tab_write BEFORE INSERT ON tabs BEGIN SELECT RAISE(ABORT, 'tab rewritten'); END;").await.unwrap();
        let unchanged = db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(unchanged.manifest_write_ns, 0);
        let mut captured = request(&tabs);
        captured.active_surface = super::super::SessionActiveSurface::Settings;
        db.persist(captured, 3).await.unwrap();
        let session = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(
            session.manifest.active_surface,
            super::super::SessionActiveSurface::Settings
        );
        assert_eq!(
            session.text_contents().values().next().unwrap(),
            "historyunchanged"
        );
    });
}

#[test]
fn invalid_captures_leave_the_committed_session_intact() {
    type Mutation = (&'static str, fn(&mut SessionPersistRequest));
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("original"), tab("other")];
        tabs[0].split_active_view(SplitAxis::Vertical);
        insert(&mut tabs[0], 0, "edit");
        db.persist(request(&tabs), 1).await.unwrap();
        let original = db.load(ReadScope::All).await.unwrap().unwrap();
        let history = db
            .read_history(&tabs[0].buffers.buffer.temp_id)
            .await
            .unwrap();
        let cases: &[Mutation] = &[
            ("duplicate buffer identity across tabs", |r| {
                r.tabs[1].session_tab.buffers[0].temp_id =
                    r.tabs[0].session_tab.buffers[0].temp_id.clone();
            }),
            ("duplicate local buffer", |r| {
                let b = r.tabs[0].session_tab.buffers[0].clone();
                r.tabs[0].session_tab.buffers.push(b);
            }),
            ("unsafe snapshot ID", |r| {
                r.tabs[0].session_tab.buffers[0].temp_id = "../escape".into();
            }),
            ("no buffers", |r| r.tabs[0].session_tab.buffers.clear()),
            ("duplicate view", |r| {
                let v = r.tabs[0].session_tab.views[0].clone();
                r.tabs[0].session_tab.views.push(v);
            }),
            ("unknown active view", |r| {
                r.tabs[0].session_tab.active_view_id = u64::MAX;
            }),
            ("unknown buffer reference", |r| {
                r.tabs[0].session_tab.views[0].buffer_id = u64::MAX;
            }),
            ("unknown pane view", |r| {
                r.tabs[0].session_tab.root_pane = SessionPaneNode::Leaf { view_id: u64::MAX }
            }),
            ("incomplete pane coverage", |r| {
                r.tabs[0].session_tab.root_pane = SessionPaneNode::Leaf {
                    view_id: r.tabs[0].session_tab.active_view_id,
                }
            }),
            ("duplicate pane leaf", |r| {
                if let SessionPaneNode::Split { first, second, .. } =
                    &mut r.tabs[0].session_tab.root_pane
                {
                    *second = first.clone();
                }
            }),
            ("invalid pane ratio", |r| {
                if let SessionPaneNode::Split { ratio, .. } = &mut r.tabs[0].session_tab.root_pane {
                    *ratio = f32::NAN;
                }
            }),
            ("orphan supplied snapshot", |r| {
                r.tabs[0].buffer_snapshots[0].temp_id = "unknown".into();
            }),
            ("zero history ID", |r| {
                r.tabs[0].session_tab.buffers[0].text_history[0].id = 0;
            }),
            ("duplicate history ID", |r| {
                let e = r.tabs[0].session_tab.buffers[0].text_history[0].clone();
                r.tabs[0].session_tab.buffers[0].text_history.push(e);
            }),
            ("overflowing history ID", |r| {
                r.tabs[0].session_tab.buffers[0].text_history[0].id = u64::MAX;
            }),
            ("zero global sequence", |r| {
                r.tabs[0].session_tab.buffers[0].text_history[0].global_seq = 0;
            }),
            ("overflowing global sequence", |r| {
                r.tabs[0].session_tab.buffers[0].text_history[0].global_seq = u64::MAX;
            }),
        ];
        for (name, mutate) in cases {
            let mut captured = request(&tabs);
            mutate(&mut captured);
            assert!(db.persist(captured, 2).await.is_err(), "{name}");
            assert!(db.conn.is_autocommit().unwrap(), "{name}");
            assert!(db.blocked.is_none(), "{name}");
            let stored = db.load(ReadScope::All).await.unwrap().unwrap();
            assert_eq!(stored.text_contents(), original.text_contents(), "{name}");
            assert_eq!(stored.manifest.tabs.len(), 2);
            assert_eq!(
                db.read_history(&tabs[0].buffers.buffer.temp_id)
                    .await
                    .unwrap(),
                history
            );
        }
        // Rejections must not advance the stale-capture barrier.
        tabs[0].buffers.buffer.replace_text("retry".into());
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(
            db.read_all_contents().await.unwrap()[&tabs[0].buffers.buffer.temp_id],
            "retry"
        );
    });
}

#[test]
fn second_owner_and_unsupported_schemas_are_rejected_without_discarding_data() {
    block_on(async {
        for pragma in [
            "PRAGMA user_version=9",
            "PRAGMA user_version=1",
            "PRAGMA user_version=999",
        ] {
            let (root, mut db) = database().await;
            db.persist(request(&[tab("preserved")]), 1).await.unwrap();
            assert!(Database::open(root.path()).await.is_err());
            db.conn.execute_batch(pragma).await.unwrap();
            db.checkpoint().await.unwrap();
            drop(db);
            let before = std::fs::read(root.path().join("session.db")).unwrap();
            assert!(Database::open(root.path()).await.is_err());
            assert_eq!(
                std::fs::read(root.path().join("session.db")).unwrap(),
                before
            );
        }
    });
}

#[test]
fn existing_page_formats_preserve_exact_reopen() {
    block_on(async {
        for seeded_page_size in [None, Some(4096), Some(16384)] {
            let root = tempfile::tempdir().unwrap();
            if let Some(page_size) = seeded_page_size {
                let engine =
                    turso::Builder::new_local(root.path().join("session.db").to_str().unwrap())
                        .with_io(turso::IoBackend::Syscall)
                        .build()
                        .await
                        .unwrap();
                let conn = engine.connect().unwrap();
                conn.execute_batch(&format!(
                    "PRAGMA page_size={page_size}; CREATE TABLE seed(x); DROP TABLE seed;"
                ))
                .await
                .unwrap();
                let mut checkpoint = conn
                    .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
                    .await
                    .unwrap();
                while checkpoint.next().await.unwrap().is_some() {}
            }
            let mut db = Database::open(root.path()).await.unwrap();
            let expected = seeded_page_size.unwrap_or(4096);
            assert_eq!(db.integer("PRAGMA page_size").await.unwrap(), expected);
            let text = "café 東京\n".repeat(50_000);
            let mut tabs = vec![tab(&text)];
            insert(&mut tabs[0], 0, "recorded edit\n");
            db.persist(request(&tabs), 1).await.unwrap();
            let history = db
                .read_history(&tabs[0].buffers.buffer.temp_id)
                .await
                .unwrap();
            let contents = db
                .load(ReadScope::All)
                .await
                .unwrap()
                .unwrap()
                .text_contents();
            assert_eq!(
                contents.values().next().unwrap(),
                &format!("recorded edit\n{text}")
            );
            db.checkpoint().await.unwrap();
            drop(db);
            let mut reopened = Database::open(root.path()).await.unwrap();
            assert_eq!(
                reopened.integer("PRAGMA page_size").await.unwrap(),
                expected
            );
            assert_eq!(
                reopened
                    .load(ReadScope::All)
                    .await
                    .unwrap()
                    .unwrap()
                    .text_contents(),
                contents
            );
            assert_eq!(
                reopened
                    .read_history(&tabs[0].buffers.buffer.temp_id)
                    .await
                    .unwrap(),
                history
            );
        }
    });
}
