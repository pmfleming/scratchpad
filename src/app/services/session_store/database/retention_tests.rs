use super::tests::{database, insert, request, tab};
use crate::app::services::session_store::CapturedClosedTab;
use futures_lite::future::block_on;

#[test]
fn closed_retention_is_budgeted_retry_safe_and_shares_blob_bodies() {
    block_on(async {
        let (_root, mut db) = database().await;
        let closed = CapturedClosedTab::capture(&tab("shared"), None);
        let mut first = request(&[]);
        first.closed_tabs.push(closed.clone());
        db.persist(first, 1).await.unwrap();
        let mut retry = request(&[]);
        retry.closed_tabs.push(closed);
        db.persist(retry, 2).await.unwrap();
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
        for generation in 3..38 {
            let mut next = request(&[]);
            next.closed_tabs
                .push(CapturedClosedTab::capture(&tab("shared"), None));
            db.persist(next, generation).await.unwrap();
        }
        assert_eq!(
            db.closed_rows().await.unwrap().len(),
            36,
            "no hidden 32-tab limit"
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            36
        );
        assert_eq!(db.integer("SELECT count(*) FROM blobs").await.unwrap(), 1);
    });
}

#[test]
fn damaged_closed_history_is_quarantined_before_retention_expiry() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut precious = tab("precious");
        insert(&mut precious, 0, "undoable ");
        let closed = CapturedClosedTab::capture(&precious, None);
        let token = closed.token.clone();
        let mut capture = request(&[]);
        capture.closed_tabs.push(closed);
        db.persist(capture, 1).await.unwrap();
        db.conn
            .execute("UPDATE history SET entry=x'00'", ())
            .await
            .unwrap();
        assert!(db.read_closed_tab(&token).await.is_err());
        assert_eq!(db.quarantined.len(), 1);
        for generation in 2..35 {
            let mut capture = request(&[]);
            capture
                .closed_tabs
                .push(CapturedClosedTab::capture(&tab("healthy"), None));
            db.persist(capture, generation).await.unwrap();
        }
        assert!(
            db.closed_rows()
                .await
                .unwrap()
                .iter()
                .any(|row| row.token == token)
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            34
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM history WHERE entry=x'00'")
                .await
                .unwrap(),
            1
        );
        db.conn
            .execute("UPDATE closed_tabs SET payload=x'00'", ())
            .await
            .unwrap();
        assert!(db.persist(request(&[]), 100).await.is_err());
        assert!(db.blocked.is_some());
    });
}

#[test]
fn failed_archive_transaction_leaves_live_session_and_retries_without_duplicate_archive() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("keep me")];
        db.persist(request(&tabs), 1).await.unwrap();
        let closed = CapturedClosedTab::capture(&tabs[0], None);
        db.conn.execute_batch("CREATE TRIGGER fail_closed BEFORE INSERT ON closed_tabs BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        let mut close = request(&[]);
        close.closed_tabs.push(closed.clone());
        assert!(db.persist(close, 2).await.is_err());
        assert!(db.closed_rows().await.unwrap().is_empty());
        assert_eq!(db.integer("SELECT count(*) FROM tabs").await.unwrap(), 1);
        db.conn
            .execute("DROP TRIGGER fail_closed", ())
            .await
            .unwrap();
        let mut close = request(&[]);
        close.closed_tabs.push(closed);
        db.persist(close, 3).await.unwrap();
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
        assert_eq!(db.integer("SELECT count(*) FROM tabs").await.unwrap(), 0);
    });
}
