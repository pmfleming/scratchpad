use super::{
    Database, ReadScope,
    test_fixture::{database, exec, request, scalar, tab, texts},
};
use futures_lite::future::block_on;
use std::io;

#[test]
fn failed_rollback_blocks_subsequent_reads_and_writes() {
    block_on(async {
        let (_root, mut db) = database().await;
        db.persist(request(&[tab("committed")]), 1).await.unwrap();
        // Inject a failed rollback result at the connection boundary.
        db.record_rollback_result(Err(io::Error::other("injected rollback failure")));
        assert!(db.load(ReadScope::All).await.is_err());
        assert!(
            db.persist(request(&[tab("must not replace")]), 2)
                .await
                .is_err()
        );
    });
}

#[test]
fn parallel_scan_detects_orphan_chunks_outside_all_content_ranges() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs: Vec<_> = (0..1000).map(|_| tab("unique identity")).collect();
        db.persist(request(&tabs), 1).await.unwrap();
        // Simulate external corruption bypassing the enabled FK constraints.
        exec(&db, "PRAGMA foreign_keys=OFF").await;
        exec(
            &db,
            "INSERT INTO document_tail VALUES('zzzz_orphan',0,0,x'6162',zeroblob(32))",
        )
        .await;
        assert!(db.load(ReadScope::All).await.is_err());
        assert!(db.persist(request(&tabs), 2).await.is_err());
    });
}

#[test]
fn document_replacement_with_reused_generation_is_not_skipped() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("old")];
        let revision = tabs[0].active_buffer().document_snapshot().revision();
        db.persist(request(&tabs), 1).await.unwrap();
        tabs[0].buffers.buffer.replace_text("replacement".into());
        assert_eq!(
            tabs[0].active_buffer().document_snapshot().revision(),
            revision
        );
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(
            texts(&mut db, ReadScope::All)
                .await
                .into_values()
                .next()
                .unwrap(),
            "replacement"
        );
    });
}

#[test]
fn disk_full_rolls_back_content_metadata_and_cleanup() {
    block_on(async {
        let (_root, mut db) = database().await;
        db.persist(request(&[tab("original")]), 1).await.unwrap();
        db.checkpoint().await.unwrap();
        let pages = scalar(&db, "PRAGMA page_count").await;
        scalar(&db, &format!("PRAGMA max_page_count={pages}")).await;
        let changed = vec![tab(&"λ".repeat(1024 * 1024))];
        assert!(db.persist(request(&changed), 2).await.is_err());
        let session = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(session.text_contents().len(), 1);
        assert_eq!(
            session.text_contents().into_values().next().unwrap(),
            "original"
        );
        scalar(&db, "PRAGMA max_page_count=1000000").await;
        db.persist(request(&changed), 3).await.unwrap();
        assert_eq!(
            texts(&mut db, ReadScope::All).await[&changed[0].active_buffer().temp_id].len(),
            2 * 1024 * 1024
        );
    });
}

#[test]
fn missing_or_corrupt_content_is_quarantined_not_replaced_with_empty_text() {
    block_on(async {
        for (damage, retained) in [
            ("DELETE FROM document_pieces", 0),
            ("UPDATE document_pieces SET body=x'00'", 1),
        ] {
            for shells_first in [true, false] {
                let (_root, mut db) = database().await;
                db.persist(request(&[tab("important")]), 1).await.unwrap();
                db.conn.execute(damage, ()).await.unwrap();
                if shells_first {
                    let stored = db.load(ReadScope::Shells).await.unwrap().unwrap();
                    assert_eq!(stored.quarantined.len(), 1);
                    assert!(stored.text_contents().is_empty());
                } else {
                    assert!(db.load(ReadScope::All).await.is_err());
                }
                assert!(db.load(ReadScope::Active).await.is_err());
                assert!(db.blocked.is_none());
                db.persist(request(&[]), 2).await.unwrap();
                assert_eq!(scalar(&db, "SELECT count(*) FROM quarantine").await, 1);
                assert_eq!(
                    scalar(&db, "SELECT count(*) FROM document_pieces").await,
                    retained
                );
                assert_eq!(
                    scalar(&db, "SELECT count(*) FROM document_pieces").await,
                    retained
                );
            }
        }
    });
}

#[test]
fn corrupt_metadata_cannot_be_overwritten_without_first_loading() {
    block_on(async {
        let (directory, mut db) = database().await;
        db.persist(request(&[tab("important")]), 1).await.unwrap();
        exec(&db, "UPDATE layouts SET root_pane='bad'").await;
        drop(db);
        let mut reopened = Database::open(directory.path()).await.unwrap();
        assert!(reopened.persist(request(&[]), 1).await.is_err());
        assert_eq!(
            reopened
                .integer("SELECT count(*) FROM document_pieces")
                .await
                .unwrap(),
            1
        );
    });
}

#[cfg(unix)]
#[test]
fn existing_session_directory_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let _db = block_on(Database::open(directory.path())).unwrap();
    for path in [
        directory.path().to_owned(),
        directory.path().join("session.db"),
        directory.path().join("session.db.lock"),
    ] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}
