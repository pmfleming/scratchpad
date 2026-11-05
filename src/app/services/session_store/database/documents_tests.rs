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
