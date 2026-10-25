use super::*;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::session_store::SessionPersistRequest;
use futures_lite::future::block_on;

fn tab(text: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new("note".into(), text.into(), None))
}
fn request(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    SessionPersistRequest::capture(tabs, 0, 16.0, false)
}

#[test]
fn worker_round_trip_and_stale_capture_barrier() {
    let directory = tempfile::tempdir().unwrap();
    let worker = DatabaseWorker::open(directory.path().to_owned(), None).unwrap();
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
    let reopened = DatabaseWorker::open(directory.path().to_owned(), None).unwrap();
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
fn metadata_only_commit_does_not_rewrite_content() {
    block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(directory.path()).await.unwrap();
        let tabs = vec![tab("unchanged")];
        database.persist(request(&tabs), 1).await.unwrap();
        database.conn.execute_batch("CREATE TRIGGER no_content_write BEFORE INSERT ON chunks BEGIN SELECT RAISE(ABORT, 'content rewritten'); END;").await.unwrap();
        let mut captured = request(&tabs);
        captured.font_size = 18.0;
        database.persist(captured, 2).await.unwrap();
        let session = database.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(session.manifest.font_size, 18.0);
        assert_eq!(session.contents.values().next().unwrap(), "unchanged");
    });
}

#[test]
fn failed_transaction_keeps_previous_session_and_can_retry() {
    block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(directory.path()).await.unwrap();
        database
            .persist(request(&[tab("original")]), 1)
            .await
            .unwrap();
        database.conn.execute_batch("CREATE TRIGGER fail_write BEFORE INSERT ON tabs BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        let changed = vec![tab("changed")];
        assert!(database.persist(request(&changed), 2).await.is_err());
        let restored = database.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(restored.contents.len(), 1);
        assert_eq!(restored.contents.values().next().unwrap(), "original");
        database
            .conn
            .execute("DROP TRIGGER fail_write", ())
            .await
            .unwrap();
        database.persist(request(&changed), 3).await.unwrap();
        assert_eq!(
            database
                .load(ReadScope::All)
                .await
                .unwrap()
                .unwrap()
                .contents
                .values()
                .next()
                .unwrap(),
            "changed"
        );
    });
}

#[test]
fn corrupt_chunk_blocks_subsequent_overwrite() {
    block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(directory.path()).await.unwrap();
        database
            .persist(request(&[tab("precious")]), 1)
            .await
            .unwrap();
        database
            .conn
            .execute("UPDATE chunks SET body=x'00'", ())
            .await
            .unwrap();
        assert!(database.load(ReadScope::All).await.is_err());
        assert!(database.persist(request(&[]), 2).await.is_err());
    });
}

#[test]
fn unicode_content_spans_multiple_bounded_chunks() {
    block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(directory.path()).await.unwrap();
        let text = "aλ🦀\0\r\n".repeat(100_000);
        database.persist(request(&[tab(&text)]), 1).await.unwrap();
        assert!(
            database
                .integer("SELECT count(*) FROM chunks")
                .await
                .unwrap()
                > 1
        );
        assert_eq!(
            database
                .load(ReadScope::All)
                .await
                .unwrap()
                .unwrap()
                .contents
                .into_values()
                .next()
                .unwrap(),
            text
        );
    });
}

#[test]
fn second_owner_and_future_schema_are_rejected() {
    block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).await.unwrap();
        assert!(Database::open(directory.path()).await.is_err());
        database
            .conn
            .execute("PRAGMA user_version=999", ())
            .await
            .unwrap();
        drop(database);
        assert!(Database::open(directory.path()).await.is_err());
    });
}
