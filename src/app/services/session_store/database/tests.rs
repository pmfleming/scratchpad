use super::{Database, DatabaseWorker, ReadScope};
use crate::app::domain::{BufferState, CharCursor, CursorRange, SplitAxis, WorkspaceTab};
use crate::app::services::session_store::{SessionPersistRequest, model::SessionPaneNode};
use futures_lite::future::block_on;

pub(super) fn tab(text: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new("note".into(), text.into(), None))
}
pub(super) fn request(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    let mut request = SessionPersistRequest::capture(tabs, 0, 16.0, false);
    // Most low-level tests isolate live snapshot GC; version retention has its own tests.
    request.version_history = false;
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
    assert_eq!(session.contents.values().next().unwrap(), "new\0\r\nλ");
    worker.call(|db| block_on(db.checkpoint())).unwrap();
    drop(worker);
    let reopened = DatabaseWorker::open(directory.path().to_owned()).unwrap();
    assert_eq!(
        reopened
            .call(|db| block_on(db.load(ReadScope::Active)))
            .unwrap()
            .unwrap()
            .contents,
        session.contents
    );
}

#[test]
fn unchanged_and_metadata_only_commits_never_rewrite_content_or_history() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("unchanged")];
        insert(&mut tabs[0], 0, "history");
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn.execute_batch("CREATE TRIGGER no_content_write BEFORE INSERT ON blobs BEGIN SELECT RAISE(ABORT, 'content rewritten'); END;
            CREATE TRIGGER no_history_write BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT, 'history rewritten'); END;
            CREATE TRIGGER no_tab_write BEFORE INSERT ON tabs BEGIN SELECT RAISE(ABORT, 'tab rewritten'); END;").await.unwrap();
        let unchanged = db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(unchanged.manifest_write_ns, 0);
        let mut captured = request(&tabs);
        captured.font_size = 18.0;
        db.persist(captured, 3).await.unwrap();
        let session = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(session.manifest.font_size, 18.0);
        assert_eq!(
            session.contents.values().next().unwrap(),
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
            assert_eq!(stored.contents, original.contents, "{name}");
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
fn second_owner_and_future_schema_are_rejected() {
    block_on(async {
        let (root, db) = database().await;
        assert!(Database::open(root.path()).await.is_err());
        db.conn
            .execute("PRAGMA user_version=999", ())
            .await
            .unwrap();
        drop(db);
        assert!(Database::open(root.path()).await.is_err());
    });
}
