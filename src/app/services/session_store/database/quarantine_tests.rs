use super::{
    Database, ReadScope,
    tests::{database, insert, request, tab},
};
use futures_lite::future::block_on;
use std::{collections::HashMap, io};
use turso::params;

async fn damage(db: &Database, id: &str) {
    db.conn
        .execute("UPDATE blobs SET body=x'00' WHERE digest IN (SELECT digest FROM parts WHERE temp_id=?)", [id])
        .await
        .unwrap();
}
async fn archive(db: &Database) -> Vec<u8> {
    db.conn
        .query("SELECT payload FROM quarantine", ())
        .await
        .unwrap()
        .next()
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap()
}

#[test]
fn cold_damage_keeps_healthy_saves_and_retains_original_data_across_restart() {
    block_on(async {
        let (directory, mut db) = database().await;
        let mut tabs = vec![tab("healthy"), tab("precious")];
        let id = tabs[1].buffers.buffer.temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        db.load(ReadScope::Shells).await.unwrap();
        let original = request(&tabs).tabs.pop().unwrap().session_tab;
        let original_bytes = serde_json::to_vec(&original).unwrap();
        damage(&db, &id).await;
        let failure = db.hydrate(original).await.err().unwrap();
        assert!(failure.to_string().contains("quarantined"));
        assert!(
            failure
                .to_string()
                .contains("healthy buffers can still be saved")
        );
        assert!(db.blocked.is_none());
        assert_eq!(archive(&db).await, original_bytes);

        // Even a live snapshot and changed metadata must not replace this buffer.
        tabs[1]
            .buffers
            .buffer
            .replace_text("must not overwrite".into());
        tabs[1].buffers.buffer.name = "must not rename".into();
        tabs[0].buffers.buffer.replace_text("healthy edits".into());
        db.persist(request(&tabs), 2).await.unwrap();
        let shells = db.load(ReadScope::Shells).await.unwrap().unwrap();
        assert_eq!(shells.manifest.tabs[1].buffers[0].name, "note");
        assert_eq!(shells.contents.values().next().unwrap(), "healthy edits");
        assert_eq!(
            db.integer("SELECT count(*) FROM blobs WHERE body=x'00'")
                .await
                .unwrap(),
            1
        );
        assert_eq!(archive(&db).await, original_bytes);

        // Closing/reordering the tab can drop its live layout, not its archive or bytes.
        tabs.pop();
        db.persist(request(&tabs), 3).await.unwrap();
        drop(db);
        let mut db = Database::open(directory.path()).await.unwrap();
        tabs[0].buffers.buffer.replace_text("after restart".into());
        db.persist(request(&tabs), 1).await.unwrap();
        assert_eq!(archive(&db).await, original_bytes);
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            2
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM blobs WHERE body=x'00'")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db.load(ReadScope::Active)
                .await
                .unwrap()
                .unwrap()
                .contents
                .values()
                .next()
                .unwrap(),
            "after restart"
        );
        // A logical export/full restore must not silently omit the archived damage.
        assert!(db.load(ReadScope::All).await.is_err());
        assert!(db.blocked.is_none());
    });
}

#[test]
fn damaged_active_tab_stays_cold_while_other_tabs_restore_and_save() {
    use crate::app::services::session_store::{
        SessionActiveSurface, SessionPersistRequest, SessionStore,
    };
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_owned());
    let tabs = vec![tab("damaged active"), tab("healthy cold")];
    store.persist(&tabs, 0, 16.0, false).unwrap();
    let id = tabs[0].buffers.buffer.temp_id.clone();
    store
        .worker()
        .unwrap()
        .call(move |db| {
            block_on(damage(db, &id));
            Ok(())
        })
        .unwrap();
    let mut restored = Vec::new();
    let status = store
        .load_streaming(
            |_, _, _| true,
            |index, tab, cold| {
                restored.push((index, tab, cold));
                true
            },
        )
        .unwrap()
        .unwrap();
    assert!(
        status
            .restore_status
            .unwrap()
            .message
            .contains("healthy buffers still save")
    );
    assert_eq!(restored.len(), 2);
    let (_, bad_shell, bad_payload) = restored.remove(0);
    let bad_payload = bad_payload.unwrap();
    assert!(store.restore_cold_session_tab(bad_payload.clone()).is_err());
    let (_, _, healthy_payload) = restored.remove(0);
    let (mut healthy, _) = store
        .restore_cold_session_tab(healthy_payload.unwrap())
        .unwrap();
    assert_eq!(healthy.buffers.buffer.text(), "healthy cold");
    healthy
        .buffers
        .buffer
        .replace_text("protected new edit".into());
    store
        .persist_request(SessionPersistRequest::capture_with_cold_tabs(
            &[bad_shell, healthy],
            &HashMap::from([(0, bad_payload)]),
            1,
            SessionActiveSurface::Workspace,
            16.0,
            false,
        ))
        .unwrap();
    assert_eq!(
        store.load_startup_visible().unwrap().unwrap().tabs[0]
            .buffers
            .buffer
            .text(),
        "protected new edit"
    );
    assert!(
        store
            .export_recovery(&directory.path().join("incomplete-export"))
            .is_err()
    );
    assert!(!directory.path().join("incomplete-export").exists());
}

#[test]
fn stale_hydration_of_a_closed_tab_does_not_create_a_quarantine() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("healthy"), tab("closed")];
        db.persist(request(&tabs), 1).await.unwrap();
        let queued = request(&tabs).tabs.pop().unwrap().session_tab;
        db.persist(request(&tabs[..1]), 2).await.unwrap();
        assert_eq!(
            db.hydrate(queued).await.err().unwrap().kind(),
            io::ErrorKind::NotFound
        );
        assert!(db.blocked.is_none());
        assert!(db.quarantined.is_empty());
        db.persist(request(&tabs[..1]), 3).await.unwrap();
        assert!(db.load(ReadScope::All).await.is_ok());
    });
}

#[test]
fn quarantine_archives_committed_metadata_not_a_stale_hydration_payload() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("healthy"), tab("changed")];
        db.persist(request(&tabs), 1).await.unwrap();
        let queued = request(&tabs).tabs.pop().unwrap().session_tab;
        tabs[1].buffers.buffer.name = "latest committed name".into();
        db.persist(request(&tabs), 2).await.unwrap();
        let committed =
            serde_json::to_vec(&request(&tabs).tabs.pop().unwrap().session_tab).unwrap();
        damage(&db, &tabs[1].buffers.buffer.temp_id).await;
        assert!(db.hydrate(queued).await.is_err());
        assert_eq!(archive(&db).await, committed);
        assert!(db.blocked.is_none());
    });
}

#[test]
fn missing_cold_content_detected_by_a_save_is_quarantined_after_rollback() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("healthy"), tab("cold")];
        db.persist(request(&tabs), 1).await.unwrap();
        db.conn
            .execute(
                "DELETE FROM snapshots WHERE id=?",
                [tabs[1].buffers.buffer.temp_id.as_str()],
            )
            .await
            .unwrap();
        tabs[0]
            .buffers
            .buffer
            .replace_text("new protected edit".into());
        let captured = || {
            let mut capture = request(&tabs);
            capture.tabs[1].buffer_snapshots.clear();
            capture.tabs[1].preserve_existing = true;
            capture
        };
        assert!(db.persist(captured(), 2).await.is_err());
        assert!(db.blocked.is_none());
        assert_eq!(db.quarantined.len(), 1);
        db.persist(captured(), 3).await.unwrap();
        assert_eq!(
            db.load(ReadScope::Active)
                .await
                .unwrap()
                .unwrap()
                .contents
                .values()
                .next()
                .unwrap(),
            "new protected edit"
        );
        // Deleting a snapshot now cascades its parts, rather than leaving orphans.
        assert_eq!(db.integer("SELECT count(*) FROM parts").await.unwrap(), 1);
    });
}

#[test]
fn malformed_or_missing_buffer_rows_are_scoped_not_manifest_failures() {
    block_on(async {
        for sql in [
            "UPDATE snapshots SET bytes=-1 WHERE id=?",
            "UPDATE snapshots SET digest=x'00' WHERE id=?",
            "UPDATE parts SET part=9 WHERE temp_id=?",
            "DELETE FROM snapshots WHERE id=?",
            "UPDATE history SET seq=seq+1 WHERE temp_id=?",
            "UPDATE history SET global_seq=global_seq+1 WHERE temp_id=?",
            "UPDATE history SET entry=x'00' WHERE temp_id=?",
        ] {
            let (_root, mut db) = database().await;
            let mut tabs = vec![tab("healthy"), tab("damaged")];
            insert(&mut tabs[1], 0, "history");
            db.persist(request(&tabs), 1).await.unwrap();
            db.conn
                .execute(sql, [tabs[1].buffers.buffer.temp_id.as_str()])
                .await
                .unwrap();
            assert!(
                db.hydrate(request(&tabs).tabs.pop().unwrap().session_tab)
                    .await
                    .is_err()
            );
            assert!(db.blocked.is_none(), "{sql}");
            assert_eq!(db.quarantined.len(), 1);
            tabs[0]
                .buffers
                .buffer
                .replace_text("still protected".into());
            db.persist(request(&tabs), 2).await.unwrap();
            assert_eq!(
                db.load(ReadScope::Active)
                    .await
                    .unwrap()
                    .unwrap()
                    .contents
                    .values()
                    .next()
                    .unwrap(),
                "still protected"
            );
        }
    });
}

#[test]
fn archive_write_failure_blocks_every_save() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab("healthy"), tab("bad")];
        db.persist(request(&tabs), 1).await.unwrap();
        damage(&db, &tabs[1].buffers.buffer.temp_id).await;
        db.conn.execute_batch("CREATE TRIGGER refuse_archive BEFORE INSERT ON quarantine BEGIN SELECT RAISE(ABORT, 'archive unavailable'); END;").await.unwrap();
        assert!(
            db.hydrate(request(&tabs).tabs.pop().unwrap().session_tab)
                .await
                .is_err()
        );
        assert!(db.blocked.is_some());
        assert!(db.persist(request(&[]), 2).await.is_err());
        assert_eq!(db.integer("SELECT count(*) FROM tabs").await.unwrap(), 2);
    });
}

#[test]
fn full_parallel_read_attributes_damage_to_one_buffer() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs: Vec<_> = (0..600)
            .map(|i| tab(&format!("protected text {i}")))
            .collect();
        db.persist(request(&tabs), 1).await.unwrap();
        damage(&db, &tabs[421].buffers.buffer.temp_id).await;
        assert!(db.load(ReadScope::All).await.is_err());
        assert!(db.blocked.is_none());
        assert_eq!(db.quarantined.len(), 1);
        assert!(
            db.quarantined
                .contains_key(&tabs[421].buffers.buffer.temp_id)
        );
        db.persist(request(&tabs), 2).await.unwrap();
    });
}

#[test]
fn old_schema_is_rejected_and_schema_batch_rolls_back() {
    block_on(async {
        let (directory, mut db) = database().await;
        db.persist(request(&[tab("before upgrade")]), 1)
            .await
            .unwrap();
        db.conn
            .execute_batch("DROP TABLE quarantine; PRAGMA user_version=1;")
            .await
            .unwrap();
        drop(db);
        assert!(Database::open(directory.path()).await.is_err());
        let (_fresh, db) = database().await;
        assert!(
            db.create_schema("CREATE TABLE partial_schema(x); CREATE TABLE tabs(x);")
                .await
                .is_err()
        );
        assert!(db.conn.is_autocommit().unwrap());
        assert_eq!(
            db.integer("SELECT count(*) FROM sqlite_master WHERE name='partial_schema'")
                .await
                .unwrap(),
            0
        );
    });
}

#[test]
fn supplied_snapshots_are_retained_even_if_capture_policy_changes() {
    block_on(async {
        let (directory, mut db) = database().await;
        let tabs = vec![tab("explicit snapshot")];
        let captured = || {
            let mut captured = request(&tabs);
            let buffer = &mut captured.tabs[0].session_tab.buffers[0];
            buffer.path = Some(directory.path().join("saved.txt"));
            buffer.is_dirty = false;
            buffer.disk_len = Some(17);
            captured
        };
        db.persist(captured(), 1).await.unwrap();
        db.conn.execute_batch("CREATE TRIGGER no_rewrite BEFORE INSERT ON blobs BEGIN SELECT RAISE(ABORT, 'rewritten'); END;").await.unwrap();
        let second = db.persist(captured(), 2).await.unwrap();
        assert_eq!(second.manifest_write_ns, 0);
        assert_eq!(
            db.integer("SELECT count(*) FROM snapshots").await.unwrap(),
            1
        );
    });
}

#[test]
fn lengths_and_hashes_describe_the_bytes_emitted_including_empty_and_unicode() {
    block_on(async {
        let (_root, mut db) = database().await;
        let tabs = vec![tab(""), tab(&"aλ🦀\0\r\n".repeat(100_000))];
        db.persist(request(&tabs), 1).await.unwrap();
        assert!(db.integer("SELECT count(*) FROM parts").await.unwrap() > 1);
        let contents = db.load(ReadScope::All).await.unwrap().unwrap().contents;
        for tab in &tabs {
            let text = tab.buffers.buffer.text();
            let id = &tab.buffers.buffer.temp_id;
            assert_eq!(contents[id], text);
            let row = db.conn.query("SELECT bytes, digest, (SELECT coalesce(sum(length(b.body)),0) FROM parts p JOIN blobs b ON p.digest=b.digest WHERE p.temp_id=?) FROM snapshots WHERE id=?", params![id.as_str(), id.as_str()]).await.unwrap().next().await.unwrap().unwrap();
            assert_eq!(row.get::<i64>(0).unwrap(), row.get::<i64>(2).unwrap());
            assert_eq!(row.get::<i64>(0).unwrap() as usize, text.len());
            assert_eq!(
                row.get::<Vec<u8>>(1).unwrap(),
                blake3::hash(text.as_bytes()).as_bytes()
            );
        }
    });
}
