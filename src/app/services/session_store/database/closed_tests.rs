use super::{
    closed::ClosedRow,
    tests::{database, request, tab},
};
use crate::app::services::session_store::CapturedClosedTab;
use futures_lite::future::block_on;

#[test]
fn archive_diff_preserves_survivors_and_requested_order() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut save = request(&[]);
        for text in ["one", "two", "three"] {
            save.closed_tabs
                .push(CapturedClosedTab::capture(&tab(text), None));
        }
        db.persist(save, 1).await.unwrap();
        let old = db.closed_rows().await.unwrap();
        let token = old[1].token.clone();
        // The middle archive must not be rewritten when the oldest is removed.
        db.conn.execute_batch(&format!(
            "CREATE TRIGGER keep_archive BEFORE DELETE ON closed_tabs WHEN old.token='{token}' BEGIN SELECT RAISE(ABORT, 'retained archive deleted'); END;
             CREATE TRIGGER keep_payload BEFORE UPDATE ON closed_tabs WHEN old.token='{token}' BEGIN SELECT RAISE(ABORT, 'retained archive updated'); END;"
        )).await.unwrap();
        db.write_closed(&old[1..], &old).await.unwrap();
        assert!(db.closed_rows().await.unwrap() == old[1..]);
        db.conn
            .execute_batch("DROP TRIGGER keep_archive; DROP TRIGGER keep_payload;")
            .await
            .unwrap();
        let mut rows = db.closed_rows().await.unwrap();
        rows.reverse();
        let previous = db.closed_rows().await.unwrap();
        db.write_closed(&rows, &previous).await.unwrap();
        assert!(db.closed_rows().await.unwrap() == rows);
        let previous = db.closed_rows().await.unwrap();
        rows[0].closed_at += 1;
        db.write_closed(&rows, &previous).await.unwrap();
        assert!(db.closed_rows().await.unwrap() == rows);
        db.write_closed(&[], &rows).await.unwrap();
        assert!(db.closed_rows().await.unwrap().is_empty());
    });
}

#[test]
fn token_lookup_does_not_decode_unrelated_archives() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut save = request(&[]);
        save.closed_tabs
            .push(CapturedClosedTab::capture(&tab("wanted"), None));
        db.persist(save, 1).await.unwrap();
        let token = db.closed_rows().await.unwrap()[0].token.clone();
        db.conn
            .execute(
                "INSERT INTO closed_tabs(token,closed_at,payload) VALUES('broken',0,x'00')",
                (),
            )
            .await
            .unwrap();
        let (_, contents) = db.read_closed_tab(&token).await.unwrap();
        assert_eq!(contents.into_values().collect::<Vec<_>>(), ["wanted"]);
        assert_eq!(
            db.read_closed_tab("missing").await.err().unwrap().kind(),
            std::io::ErrorKind::NotFound
        );
        assert!(db.blocked.is_none());
        assert!(db.read_closed_tab("broken").await.is_err());
        assert!(db.blocked.is_some());
    });
}

#[test]
fn archive_diff_failure_rolls_back_removed_and_changed_rows() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut save = request(&[]);
        for text in ["one", "two"] {
            save.closed_tabs
                .push(CapturedClosedTab::capture(&tab(text), None));
        }
        db.persist(save, 1).await.unwrap();
        let old = db.closed_rows().await.unwrap();
        let changed = ClosedRow::new(
            old[1].token.clone(),
            old[1].closed_at + 1,
            old[1].payload.clone(),
        )
        .unwrap();
        db.conn.execute_batch("CREATE TRIGGER fail_update BEFORE UPDATE ON closed_tabs BEGIN SELECT RAISE(ABORT, 'injected'); END; BEGIN IMMEDIATE;").await.unwrap();
        let result = db.write_closed(&[changed], &old).await;
        assert!(db.finish_transaction(result).await.is_err());
        assert!(db.closed_rows().await.unwrap() == old);
    });
}
