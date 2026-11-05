use super::{
    Database, ReadScope,
    tests::{database, insert, request, tab},
};
use crate::app::{domain::SplitAxis, services::session_store::CapturedClosedTab};
use futures_lite::future::block_on;

#[test]
fn documents_and_branches_survive_loss_of_every_workspace_reference() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut live = tab("baseline");
        insert(&mut live, 0, "one");
        live.active_buffer_mut().navigate_history(1).unwrap();
        insert(&mut live, 0, "branch");
        let id = live.active_buffer().temp_id.clone();
        let expected = live.active_buffer().document().exported_history();
        db.persist(request(&[live]), 1).await.unwrap();
        db.persist(request(&[]), 2).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap(), expected);
        assert_eq!(db.read_document(&id).await.unwrap().name, "note");
        assert_eq!(db.read_all_contents().await.unwrap()[&id], "branchbaseline");
        assert!(
            db.load(ReadScope::Shells)
                .await
                .unwrap()
                .unwrap()
                .manifest
                .tabs
                .is_empty()
        );
    });
}

#[test]
fn consuming_a_closed_split_layout_retains_every_other_document() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut split = tab("first");
        split.split_active_view(SplitAxis::Vertical).unwrap();
        // A split using two distinct documents.
        let other = tab("second").active_buffer().clone();
        split.buffers.push_extra(other.clone());
        let view = split.layout.active_view_id();
        split
            .layout
            .views_mut()
            .iter_mut()
            .find(|v| v.id == view)
            .unwrap()
            .buffer_id = other.id;
        let mut close = request(&[]);
        close
            .closed_tabs
            .push(CapturedClosedTab::capture(&split, None));
        db.persist(close, 1).await.unwrap();
        let ids: Vec<_> = split.buffers().map(|b| b.temp_id.clone()).collect();
        db.persist(request(&[crate::app::domain::WorkspaceTab::new(other)]), 2)
            .await
            .unwrap();
        assert!(db.closed_rows().await.unwrap().is_empty());
        drop(db);
        let db = Database::open(root.path()).await.unwrap();
        for id in ids {
            assert!(!db.read_history(&id).await.unwrap().is_empty());
            db.read_document(&id).await.unwrap();
        }
    });
}

#[test]
fn unchanged_closed_documents_are_never_rewritten_by_later_saves() {
    block_on(async {
        let (_root, mut db) = database().await;
        let closed = tab("closed");
        let id = closed.active_buffer().temp_id.clone();
        let mut close = request(&[]);
        close
            .closed_tabs
            .push(CapturedClosedTab::capture(&closed, None));
        db.persist(close, 1).await.unwrap();
        db.conn.execute_batch(&format!("CREATE TRIGGER keep_document BEFORE UPDATE ON documents WHEN old.id='{id}' BEGIN SELECT RAISE(ABORT,'closed document rewritten'); END;
            CREATE TRIGGER keep_layout BEFORE UPDATE ON closed_tabs BEGIN SELECT RAISE(ABORT,'closed layout rewritten'); END;")).await.unwrap();
        db.persist(request(&[tab("live")]), 2).await.unwrap();
        assert_eq!(db.read_history(&id).await.unwrap().len(), 1);
    });
}

#[test]
fn warm_saves_do_not_read_unrelated_closed_layout_payloads() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut close = request(&[]);
        close
            .closed_tabs
            .push(CapturedClosedTab::capture(&tab("closed"), None));
        db.persist(close, 1).await.unwrap();
        db.conn
            .execute("UPDATE layouts SET root_pane='bad'", ())
            .await
            .unwrap();
        // An unrelated corrupt archive is preserved; selecting it reports its error.
        db.persist(request(&[tab("live")]), 2).await.unwrap();
        assert!(db.closed_rows().await.is_err());
    });
}

#[test]
fn root_and_nonroot_references_are_constrained_and_layout_indexes_are_validated() {
    block_on(async {
        let (_root, mut db) = database().await;
        let live = tab("baseline");
        let id = live.active_buffer().temp_id.clone();
        db.persist(request(&[live]), 1).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM history_nodes WHERE node_id=0")
                .await
                .unwrap(),
            1
        );
        assert!(
            db.conn
                .execute("UPDATE document_history SET now_id=999", ())
                .await
                .is_err()
        );
        assert!(
            db.conn
                .execute("UPDATE history SET parent_id=998,seq=999", ())
                .await
                .is_err()
        );
        db.conn
            .execute(
                "DELETE FROM layout_documents WHERE document_id=?",
                [id.as_str()],
            )
            .await
            .unwrap();
        assert!(db.load(ReadScope::Shells).await.is_err());
    });
}

#[test]
fn save_as_over_a_retained_path_transfers_lookup_without_reusing_the_replaced_tree() {
    block_on(async {
        let (root, mut db) = database().await;
        let path = root.path().join("target.txt");
        std::fs::write(&path, "").unwrap();
        let mut source = tab("source");
        let source_id = source.active_buffer().temp_id.clone();
        let mut target = tab("");
        target.active_buffer_mut().set_path(Some(path.clone()));
        let old_id = target.active_buffer().temp_id.clone();
        db.persist(request(&[source.clone(), target]), 1)
            .await
            .unwrap();
        db.persist(request(&[source.clone()]), 2).await.unwrap();
        source.active_buffer_mut().set_path(Some(path.clone()));
        db.persist(request(&[source]), 3).await.unwrap();
        let key = crate::app::CanonicalPathKey::from_path(&path);
        assert_eq!(
            db.retained_tab_by_path(key.as_str())
                .await
                .unwrap()
                .unwrap()
                .buffers[0]
                .temp_id,
            source_id
        );
        let old = db.read_document(&old_id).await.unwrap();
        assert!(old.path.is_none());
        assert!(old.is_dirty);
        let (tab, _) = db.load_retained_document(&old_id).await.unwrap();
        assert_eq!(
            tab.buffers[0]
                .history_document
                .as_ref()
                .unwrap()
                .try_extract_text()
                .unwrap(),
            ""
        );
    });
}

#[test]
fn a_cursor_move_updates_one_view_and_closing_the_first_tab_preserves_layouts() {
    use crate::app::domain::{CharCursor, CursorRange};
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("one"), tab("two"), tab("three")];
        db.persist(request(&tabs), 1).await.unwrap();
        let ids: Vec<_> = tabs.iter().map(|t| t.layout_id).collect();
        db.conn.execute_batch("CREATE TABLE metadata_writes(kind TEXT) STRICT;
            CREATE TRIGGER layout_update AFTER UPDATE ON layouts BEGIN INSERT INTO metadata_writes VALUES('layout'); END;
            CREATE TRIGGER view_update AFTER UPDATE ON layout_views BEGIN INSERT INTO metadata_writes VALUES('view'); END;
            CREATE TRIGGER document_update AFTER UPDATE ON documents BEGIN INSERT INTO metadata_writes VALUES('document'); END;
            CREATE TRIGGER state_update AFTER UPDATE ON session_state BEGIN INSERT INTO metadata_writes VALUES('state'); END;").await.unwrap();
        tabs[1].layout.views_mut()[0].cursor_range = Some(CursorRange::one(CharCursor::new(2)));
        db.persist(request(&tabs), 2).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM metadata_writes")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db.integer("SELECT count(*) FROM metadata_writes WHERE kind='view'")
                .await
                .unwrap(),
            1
        );
        db.conn
            .execute("DELETE FROM metadata_writes", ())
            .await
            .unwrap();
        let closed = tabs.remove(0);
        let mut save = request(&tabs);
        save.closed_tabs
            .push(CapturedClosedTab::capture(&closed, None));
        db.persist(save, 3).await.unwrap();
        assert_eq!(
            db.integer("SELECT count(*) FROM metadata_writes WHERE kind<>'state'")
                .await
                .unwrap(),
            0
        );
        let restored = db.load(ReadScope::All).await.unwrap().unwrap();
        assert_eq!(
            restored
                .manifest
                .tabs
                .iter()
                .map(|t| t.layout_id)
                .collect::<Vec<_>>(),
            ids[1..]
        );
        assert_eq!(db.closed_rows().await.unwrap()[0].tab.layout_id, ids[0]);
    });
}

#[test]
#[ignore = "manual 10,000-tab typed metadata and startup reservation measurement"]
fn measure_real_typed_metadata_at_ten_thousand_tabs() {
    use std::time::Instant;
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs: Vec<_> = (0..10_000).map(|_| tab("")).collect();
        let begin = Instant::now();
        let first_profile = db
            .persist(
                crate::app::services::session_store::SessionPersistRequest::capture(
                    &tabs, 0, 16.0, false,
                ),
                1,
            )
            .await
            .unwrap();
        eprintln!("first_profile={first_profile:?}");
        let first = begin.elapsed();
        let begin = Instant::now();
        let steady_profile = db
            .persist(
                crate::app::services::session_store::SessionPersistRequest::capture(
                    &tabs, 0, 16.0, false,
                ),
                2,
            )
            .await
            .unwrap();
        eprintln!("steady_profile={steady_profile:?}");
        let steady = begin.elapsed();
        let begin = Instant::now();
        let loaded = db.load(ReadScope::Shells).await.unwrap().unwrap();
        let restore = begin.elapsed();
        assert_eq!(loaded.manifest.tabs.len(), 10_000);
        let max = tabs.iter().map(|t| t.layout_id).max().unwrap();
        drop(db);
        let begin = Instant::now();
        let mut db = Database::open(root.path()).await.unwrap();
        let open = begin.elapsed();
        assert!(crate::app::domain::tab::next_layout_id() > max);
        tabs.remove(0);
        let begin = Instant::now();
        db.persist(
            crate::app::services::session_store::SessionPersistRequest::capture(
                &tabs, 0, 16.0, false,
            ),
            1,
        )
        .await
        .unwrap();
        let close = begin.elapsed();
        eprintln!(
            "typed_metadata tabs=10000 first_ms={} steady_ms={} restore_ms={} startup_reserve_ms={} close_first_ms={}",
            first.as_millis(),
            steady.as_millis(),
            restore.as_millis(),
            open.as_millis(),
            close.as_millis()
        );
    });
}
