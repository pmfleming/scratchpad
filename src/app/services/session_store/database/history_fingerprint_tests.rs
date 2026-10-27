use super::{
    Database,
    history::{HistoryFingerprint, fingerprints},
    tests::{database, insert, request, tab},
};
use futures_lite::future::block_on;

#[test]
fn fingerprints_cover_payload_selection_coalescing_and_budget_stripping() {
    let mut tab = tab("before");
    insert(&mut tab, 0, "payload");
    let entries = tab.active_buffer().document().exported_history();
    let original = fingerprints(&entries).unwrap();
    for case in 0..5 {
        let mut changed = entries.clone();
        match case {
            0 => changed[0].global_seq += 1,
            1 => changed[0].summary.push_str(" changed"),
            2 => {
                changed[0].next_selection.primary_prefer_next_row =
                    !changed[0].next_selection.primary_prefer_next_row;
            }
            3 => changed[0].drop_payloads(),
            _ => changed[0].fingerprint ^= 1,
        }
        assert_ne!(fingerprints(&changed).unwrap(), original);
    }
}

#[test]
fn worker_cache_size_does_not_scale_with_undo_payload_and_reopen_uses_same_fingerprints() {
    block_on(async {
        let (root, mut db) = database().await;
        let mut tabs = vec![tab("")];
        insert(&mut tabs[0], 0, &"large undo payload ".repeat(64 * 1024));
        let id = tabs[0].active_buffer().temp_id.clone();
        db.persist(request(&tabs), 1).await.unwrap();
        let expected = db.history_cache[&id].clone();
        assert!(!expected.is_empty());
        assert!(std::mem::size_of::<HistoryFingerprint>() <= 40);
        let payload_bytes: usize = tabs[0]
            .active_buffer()
            .document()
            .exported_history()
            .iter()
            .map(|entry| {
                entry
                    .edits
                    .iter()
                    .map(|edit| edit.payload_bytes())
                    .sum::<usize>()
            })
            .sum();
        assert!(payload_bytes > 1024 * 1024);
        assert!(std::mem::size_of_val(expected.as_slice()) <= 80);
        drop(db);
        let mut db = Database::open(root.path()).await.unwrap();
        db.persist(request(&tabs), 1).await.unwrap();
        assert_eq!(db.history_cache[&id], expected);
    });
}
