//! Shared fixtures for the session database tests.
use super::{Database, ReadScope};
use crate::app::domain::{BufferState, CharCursor, CursorRange, WorkspaceTab};
use crate::app::services::session_store::{SessionPersistRequest, SessionStore};
use futures_lite::future::block_on;
use std::collections::HashMap;

pub(super) fn tab(text: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new("note".into(), text.into(), None))
}
/// A capture whose history is materialized as rows, so malformed-row tests can alter it.
pub(super) fn request(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    let mut request = SessionPersistRequest::capture(tabs, 0, 16.0, false);
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
/// A production capture that keeps history as shared references.
pub(super) fn capture(tabs: &[WorkspaceTab]) -> SessionPersistRequest {
    SessionPersistRequest::capture(tabs, 0, 14.0, false)
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
/// A database holding one committed tab with `text`, plus that tab and its document id.
pub(super) async fn committed(
    text: &str,
) -> (tempfile::TempDir, Database, Vec<WorkspaceTab>, String) {
    let (root, mut db) = database().await;
    let tabs = vec![tab(text)];
    db.persist(request(&tabs), 1).await.unwrap();
    let id = tabs[0].active_buffer().temp_id.clone();
    (root, db, tabs, id)
}
/// Runs SQL that alters or audits stored rows behind the database's back.
pub(super) async fn exec(db: &Database, sql: &str) {
    db.conn.execute_batch(sql).await.unwrap();
}
pub(super) async fn scalar(db: &Database, sql: &str) -> i64 {
    db.integer(sql).await.unwrap()
}
pub(super) async fn blobs(db: &Database, sql: &str) -> Vec<Vec<u8>> {
    db.query_collect(sql, (), |r| r.get::<Vec<u8>>(0).map_err(super::error))
        .await
        .unwrap()
}
/// Loads a committed session and returns each document's text by id.
pub(super) async fn texts(db: &mut Database, scope: ReadScope) -> HashMap<String, String> {
    db.load(scope).await.unwrap().unwrap().text_contents()
}
/// Each batch query must seek its table's index rather than scan the table.
pub(super) async fn assert_queries_seek_indexes(db: &Database, queries: &[(&str, &str)]) {
    for (table, query) in queries {
        let mut rows = db
            .conn
            .query(&format!("EXPLAIN QUERY PLAN {query}"), ())
            .await
            .unwrap();
        let mut details = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            details.push(row.get::<String>(3).unwrap());
        }
        let seeks = format!("SEARCH {table} USING INDEX");
        let scans = format!("SCAN {table}");
        assert!(
            details.iter().any(|line| line.starts_with(&seeks)),
            "{details:?}"
        );
        assert!(
            !details.iter().any(|line| line.starts_with(&scans)),
            "{details:?}"
        );
    }
}

/// Store-level helpers for tests that drive the app against a real session store.
impl SessionStore {
    /// Runs SQL on the store's database worker, behind the store's back.
    pub(crate) fn exec_for_test(&self, sql: &str) {
        let sql = sql.to_owned();
        self.worker()
            .unwrap()
            .call(move |db| block_on(db.conn.execute_batch(&sql)).map_err(super::error))
            .unwrap();
    }
    pub(crate) fn integer_for_test(&self, sql: &str) -> i64 {
        let sql = sql.to_owned();
        self.worker()
            .unwrap()
            .call(move |db| block_on(db.integer(&sql)))
            .unwrap()
    }
    /// The active text of the most recently closed tab, read without consuming it.
    pub(crate) fn newest_closed_text(&self) -> String {
        let archive = self.closed_tabs().unwrap().remove(0);
        let tab = self.restore_closed_tab(&archive.token).unwrap();
        tab.active_buffer().text()
    }
}
