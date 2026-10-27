use super::{
    Database, ReadScope,
    tests::{database, insert, request, tab},
};
use crate::app::{
    domain::{BufferState, SplitAxis, TextDocument, TextHistoryBudget},
    services::{
        session_store::{CapturedClosedTab, capture::CapturedRecoveryPoint},
        settings_store::RecoverySettings,
    },
};
use futures_lite::future::block_on;

#[test]
fn corrupt_archive_compaction_quarantines_without_blocking_healthy_saves_or_restart() {
    block_on(async {
        for damage_history in [false, true] {
            let (root, mut db) = database().await;
            let mut archived = tab("archive base");
            insert(&mut archived, 0, &"x".repeat(20_000));
            let id = archived.active_buffer().temp_id.clone();
            let closed = CapturedClosedTab::capture(&archived, None);
            let token = closed.token.clone();
            let mut live = vec![tab("healthy")];
            let mut initial = request(&live);
            initial.closed_tabs.push(closed);
            db.persist(initial, 1).await.unwrap();
            if damage_history {
                db.conn
                    .execute(
                        "UPDATE history SET entry=x'00' WHERE temp_id=?",
                        [id.as_str()],
                    )
                    .await
                    .unwrap();
            } else {
                db.conn.execute("UPDATE blobs SET body=x'00' WHERE digest IN (SELECT digest FROM parts WHERE temp_id=?)", [id.as_str()]).await.unwrap();
            }
            let policy = RecoverySettings {
                archive_byte_budget: 4000,
                ..Default::default()
            };
            live[0]
                .active_buffer_mut()
                .replace_text("healthy edit".into());
            let mut save = request(&live);
            save.recovery_policy = policy.clone();
            db.persist(save, 2).await.unwrap();
            assert!(db.blocked.is_none());
            assert!(db.quarantined.contains_key(&id));
            assert_eq!(db.closed_rows().await.unwrap()[0].token, token);
            assert_eq!(db.integer("SELECT count(*) FROM history").await.unwrap(), 1);
            assert!(db.read_closed_tab(&token).await.is_err());
            assert!(db.blocked.is_none());
            drop(db);
            let mut db = Database::open(root.path()).await.unwrap();
            live[0]
                .active_buffer_mut()
                .replace_text("after restart".into());
            let mut save = request(&live);
            save.recovery_policy = policy;
            db.persist(save, 1).await.unwrap();
            assert!(db.quarantined.contains_key(&id));
            assert_eq!(db.closed_rows().await.unwrap()[0].token, token);
            let session = db.load(ReadScope::Active).await.unwrap().unwrap();
            assert_eq!(
                session.contents[&live[0].active_buffer().temp_id],
                "after restart"
            );
        }
    });
}

#[test]
fn partially_compacted_split_keeps_all_history_when_a_later_buffer_is_damaged() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut archived = tab("first");
        insert(&mut archived, 0, &"a".repeat(20_000));
        let first = archived.active_buffer().temp_id.clone();
        archived
            .open_buffer_as_split(
                BufferState::new("second".into(), "second".into(), None),
                SplitAxis::Vertical,
                true,
                0.5,
            )
            .unwrap();
        insert(&mut archived, 0, &"b".repeat(20_000));
        let second = archived.active_buffer().temp_id.clone();
        let mut initial = request(&[]);
        initial
            .closed_tabs
            .push(CapturedClosedTab::capture(&archived, None));
        db.persist(initial, 1).await.unwrap();
        db.conn
            .execute(
                "UPDATE history SET entry=x'00' WHERE temp_id=?",
                [second.as_str()],
            )
            .await
            .unwrap();
        let mut save = request(&[]);
        save.recovery_policy.archive_byte_budget = 1;
        db.persist(save, 2).await.unwrap();
        assert_eq!(db.read_history(&first).await.unwrap().len(), 1);
        assert!(db.quarantined.contains_key(&second));
        assert_eq!(db.integer("SELECT count(*) FROM history").await.unwrap(), 2);
        assert_eq!(
            db.integer("SELECT count(*) FROM buffer_versions")
                .await
                .unwrap(),
            0
        );
    });
}

#[test]
fn failed_quarantine_still_blocks_destructive_replacement() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut archived = tab("precious");
        insert(&mut archived, 0, &"x".repeat(20_000));
        let mut initial = request(&[]);
        initial
            .closed_tabs
            .push(CapturedClosedTab::capture(&archived, None));
        db.persist(initial, 1).await.unwrap();
        db.conn.execute_batch("UPDATE history SET entry=x'00'; CREATE TRIGGER no_quarantine BEFORE INSERT ON quarantine BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        let mut save = request(&[tab("healthy")]);
        save.recovery_policy.archive_byte_budget = 1;
        assert!(db.persist(save, 2).await.is_err());
        assert!(db.blocked.is_some());
        assert_eq!(db.closed_rows().await.unwrap().len(), 1);
        assert_eq!(db.integer("SELECT count(*) FROM history").await.unwrap(), 1);
    });
}

#[test]
fn protected_archives_do_not_evict_new_archives_or_live_undo_boundaries() {
    block_on(async {
        for quarantine in [false, true] {
            let (_root, mut db) = database().await;
            let protected_tab = tab(&"p".repeat(60_000));
            let id = protected_tab.active_buffer().temp_id.clone();
            let protected = CapturedClosedTab::capture(&protected_tab, None);
            let token = protected.token.clone();
            let mut initial = request(&[]);
            initial.version_history = true;
            initial.closed_tabs.push(protected);
            db.persist(initial, 1).await.unwrap();
            if quarantine {
                let tab = db.closed_rows().await.unwrap().remove(0).tab;
                db.protect_damaged(&super::quarantine::damaged(&id, "injected damage"), &[tab])
                    .await
                    .unwrap();
            }
            let live = vec![tab("live")];
            let ordinary = CapturedClosedTab::capture(&tab("ordinary archive"), None);
            let ordinary_token = ordinary.token.clone();
            let mut save = request(&live);
            save.recovery_policy = RecoverySettings {
                archive_byte_budget: 4000,
                snapshot_byte_budget: 4000,
                pinned_archives: if quarantine { vec![] } else { vec![token] },
                ..Default::default()
            };
            save.closed_tabs.push(ordinary);
            save.tabs[0].recovery_points.push(CapturedRecoveryPoint {
                temp_id: live[0].active_buffer().temp_id.clone(),
                sequence: 123,
                snapshot: TextDocument::new("before undo pruning".into()).snapshot(),
            });
            db.persist(save, 2).await.unwrap();
            assert!(
                db.closed_rows()
                    .await
                    .unwrap()
                    .iter()
                    .any(|row| row.token == ordinary_token)
            );
            assert_eq!(db.closed_rows().await.unwrap().len(), 2);
            assert_eq!(db.version_history(&id).await.unwrap().len(), 1);
            let points = db
                .version_history(&live[0].active_buffer().temp_id)
                .await
                .unwrap();
            assert_eq!(points.len(), 1);
            assert!(points[0].undo_boundary);
        }
    });
}

#[test]
fn committed_undo_tokens_make_idle_saves_noops_even_before_ui_acknowledgement() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = vec![tab("base")];
        live[0]
            .active_buffer_mut()
            .document_mut()
            .set_history_budget(TextHistoryBudget {
                per_file_entry_limit: 4096,
                per_file_byte_budget: 64 << 20,
                aggregate_byte_budget: 64 << 20,
                persisted_payload_budget: 1 << 20,
                derived_from_memory: false,
            });
        insert(&mut live[0], 0, &"x".repeat((1 << 20) + 1));
        assert_eq!(request(&live).recovery_tokens().len(), 1);
        db.conn.execute_batch("CREATE TRIGGER fail_point BEFORE INSERT ON buffer_versions BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        assert!(db.persist(request(&live), 1).await.is_err());
        assert_eq!(
            request(&live).recovery_tokens().len(),
            1,
            "failed save remains retryable"
        );
        db.conn
            .execute("DROP TRIGGER fail_point", ())
            .await
            .unwrap();
        db.persist(request(&live), 2).await.unwrap();
        let changes = db.integer("SELECT total_changes()").await.unwrap();
        let profile = db.persist(request(&live), 3).await.unwrap();
        assert_eq!(profile.manifest_write_ns, 0);
        assert_eq!(profile.stale_cleanup_ns, 0);
        assert_eq!(db.integer("SELECT total_changes()").await.unwrap(), changes);
        let tokens = request(&live).recovery_tokens();
        live[0]
            .active_buffer_mut()
            .document_mut()
            .acknowledge_recovery_snapshot(tokens[0].1);
        assert!(request(&live).recovery_tokens().is_empty());
        assert_eq!(
            db.persist(request(&live), 4)
                .await
                .unwrap()
                .manifest_write_ns,
            0
        );
    });
}
