use super::{
    Database,
    history::HistoryFingerprint,
    tests::{database, insert, request, tab},
};
use crate::app::domain::PersistedHistoryEntry;
use futures_lite::future::block_on;

#[test]
fn compact_history_cache_persists_complete_row_changes_and_survives_reopen() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("")];
        insert(&mut tabs[0], 0, &"large undo payload ".repeat(64 * 1024));
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        let original = db.history_cache[&id].clone();
        assert_eq!(original.len(), 1);
        assert!(std::mem::size_of::<HistoryFingerprint>() <= 40);
        let entries = tabs[0].active_buffer().document().exported_history();
        assert!(
            entries[0]
                .edits
                .iter()
                .map(|edit| edit.payload_bytes())
                .sum::<usize>()
                > 1024 * 1024
        );

        // Verify real writes, not just two calls to the hash function. These fields
        // can change while the stable row ID and document revision remain equal.
        let mutations: [fn(&mut PersistedHistoryEntry); 4] = [
            |entry| entry.global_seq += 1,
            |entry| entry.summary.push_str(" changed"),
            |entry| {
                entry.next_selection.primary_prefer_next_row =
                    !entry.next_selection.primary_prefer_next_row;
            },
            |entry| {
                entry.previous_selection.secondary_prefer_next_row =
                    !entry.previous_selection.secondary_prefer_next_row;
            },
        ];
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut capture = request(&tabs);
            let entries = &mut capture.tabs[0].session_tab.buffers[0].text_history;
            mutate(&mut entries[0]);
            let expected = entries.clone();
            db.persist(capture, index as u64 + 2).await.unwrap();
            assert_eq!(
                db.read_history(&id).await.unwrap(),
                expected,
                "mutation {index}"
            );
            assert_ne!(db.history_cache[&id], original);
        }
        db.persist(request(&tabs), 10).await.unwrap();
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        db.persist(request(&tabs), 1).await.unwrap();
        assert_eq!(db.history_cache[&id], original);
    });
}

#[test]
#[ignore = "manual performance measurement"]
fn unchanged_history_persistence_performance() {
    use crate::app::services::session_store::SessionPersistRequest;
    use std::time::Instant;
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("")];
        for _ in 0..4096 {
            insert(&mut tabs[0], 0, "history payload\n");
            tabs[0].buffers.buffer.document_mut().seal_history_group();
        }
        let capture = |tabs: &[crate::app::domain::WorkspaceTab]| {
            SessionPersistRequest::capture(tabs, 0, 16.0, false)
        };
        db.persist(capture(&tabs), 1).await.unwrap();
        let count = tabs[0].active_buffer().document().history_entries().len();
        let mut samples = Vec::new();
        for generation in 2..11 {
            let request = capture(&tabs);
            let start = Instant::now();
            let result = db.persist(request, generation).await.unwrap();
            assert!(!result.transaction_committed);
            samples.push(start.elapsed().as_nanos());
        }
        samples.sort_unstable();
        println!(
            "unchanged_history entries={count} median_ns={} range={}-{}",
            samples[4], samples[0], samples[8]
        );
    });
}

#[test]
fn shared_history_fingerprints_track_appends_and_undo_positions() {
    use super::{history::fingerprints, history_rows::HistoryRows};
    use crate::app::services::session_store::SessionPersistRequest;
    use std::sync::Arc;
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("")];
        for _ in 0..128 {
            insert(&mut tabs[0], 0, "x");
            tabs[0].buffers.buffer.document_mut().seal_history_group();
        }
        let capture = |tabs: &[crate::app::domain::WorkspaceTab]| {
            SessionPersistRequest::capture(tabs, 0, 16.0, false)
        };
        db.persist(capture(&tabs), 1).await.unwrap();
        let id = tabs[0].active_buffer().temp_id.clone();
        let old_keys = db.history_cache[&id].clone();
        let old_entries = db.history_inputs[&id].clone();
        let rows = HistoryRows(tabs[0].active_buffer().document().history_snapshot());
        assert!(Arc::ptr_eq(
            &old_keys,
            &fingerprints(&rows, Some((&old_entries, &old_keys)))
        ));
        assert!(
            !db.persist(capture(&tabs), 2)
                .await
                .unwrap()
                .transaction_committed
        );

        insert(&mut tabs[0], 0, "new");
        let rows = HistoryRows(tabs[0].active_buffer().document().history_snapshot());
        let incremental = fingerprints(&rows, Some((&old_entries, &old_keys)));
        assert_eq!(incremental, fingerprints(&rows, None));
        assert_ne!(incremental, old_keys);
        assert!(
            rows.0
                .reference_entries()
                .iter()
                .zip(old_entries.iter())
                .filter(|(new, old)| std::ptr::eq(*new, *old))
                .count()
                >= 64
        );
        db.persist(capture(&tabs), 3).await.unwrap();
        let keys = db.history_cache[&id].clone();
        tabs[0]
            .buffers
            .buffer
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(capture(&tabs), 4).await.unwrap();
        assert_eq!(keys, db.history_cache[&id]);
        assert_eq!(
            db.read_history(&id).await.unwrap(),
            tabs[0].active_buffer().document().exported_history()
        );
    });
}
