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
        let mutations: [fn(&mut PersistedHistoryEntry); 5] = [
            |entry| entry.global_seq += 1,
            |entry| entry.summary.push_str(" changed"),
            |entry| {
                entry.next_selection.primary_prefer_next_row =
                    !entry.next_selection.primary_prefer_next_row;
            },
            PersistedHistoryEntry::drop_payloads,
            |entry| entry.fingerprint ^= 1,
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
