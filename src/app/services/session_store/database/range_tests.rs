use super::{
    chunking::{ChunkCheckpoint, IncrementalChunks},
    tests::{database, insert, request, tab},
};
use crate::app::domain::{DocumentSnapshot, PieceSource};
use futures_lite::future::block_on;

fn random_text(bytes: usize) -> String {
    let mut state = 7u64;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(b' ' + (state % 95) as u8)
        })
        .collect()
}
fn compare(snapshot: &DocumentSnapshot, old: Option<&ChunkCheckpoint>) -> (ChunkCheckpoint, u64) {
    let mut incremental = IncrementalChunks::new(snapshot, old);
    let parts = incremental
        .by_ref()
        .map(|chunk| chunk.unwrap().meta)
        .collect::<Vec<_>>();
    let full = IncrementalChunks::new(snapshot, None)
        .map(|chunk| chunk.unwrap().meta)
        .collect::<Vec<_>>();
    assert_eq!(parts.len(), full.len());
    for (actual, expected) in parts.iter().zip(full) {
        assert_eq!(actual.len, expected.len);
        assert_eq!(actual.digest, expected.digest);
    }
    (
        ChunkCheckpoint {
            token: snapshot.piece_tree().save_change_token(),
            parts,
        },
        incremental.bytes_read(),
    )
}

#[test]
fn incremental_boundaries_match_full_cdc_after_multiple_unicode_edits_and_eof_changes() {
    let mut tab = tab(&random_text(2 * 1024 * 1024));
    let (mut checkpoint, _) = compare(&tab.active_buffer().document_snapshot(), None);
    for round in 0..32 {
        let tree = tab.buffers.buffer.document_mut().piece_tree_mut();
        let at = match round % 4 {
            0 => 0,
            1 => tree.len_chars() / 2,
            2 => tree.len_chars(),
            _ => 7919,
        };
        tree.insert_with_source(at, "🦀λ\0\r\n", PieceSource::Edit);
        if round % 3 == 0 {
            let end = (at + 73).min(tree.len_chars());
            tree.remove_char_range(at..end);
        }
        if round % 5 == 0 {
            tree.insert_with_source(tree.len_chars(), "end", PieceSource::Edit);
        }
        let (next, read) = compare(&tab.active_buffer().document_snapshot(), Some(&checkpoint));
        assert!(read < 1024 * 1024, "local edits read {read} bytes");
        checkpoint = next;
    }
}

#[test]
fn missing_or_branched_edit_journal_falls_back_without_stale_hashes() {
    let mut tab = tab(&random_text(1024 * 1024));
    let (checkpoint, _) = compare(&tab.active_buffer().document_snapshot(), None);
    let mut branch = tab.active_buffer().document().clone();
    insert(&mut tab, 1, "left");
    let (left, _) = compare(&tab.active_buffer().document_snapshot(), Some(&checkpoint));
    branch
        .piece_tree_mut()
        .insert_with_source(1, "right", PieceSource::Edit);
    let snapshot = branch.snapshot();
    let (_, read) = compare(&snapshot, Some(&left));
    assert_eq!(read as usize, snapshot.len_bytes());
    // More edits than the bounded journal can prove: safely rescan everything.
    for _ in 0..140 {
        tab.buffers
            .buffer
            .document_mut()
            .piece_tree_mut()
            .insert_with_source(0, "x", PieceSource::Edit);
    }
    let snapshot = tab.active_buffer().document_snapshot();
    let (_, read) = compare(&snapshot, Some(&checkpoint));
    assert_eq!(read as usize, snapshot.len_bytes());
    tab.buffers
        .buffer
        .replace_text("entirely replaced 🦀".into());
    let snapshot = tab.active_buffer().document_snapshot();
    let (_, read) = compare(&snapshot, Some(&left));
    assert_eq!(read as usize, snapshot.len_bytes());
}

#[test]
#[ignore = "release CPU/byte-work probe; run explicitly on an idle host"]
fn profile_local_edit_saves_against_forced_full_scan() {
    block_on(async {
        let (_root, mut incremental) = database().await;
        let (_baseline_root, mut full_scan) = database().await;
        let mut tabs = vec![tab(&random_text(16 * 1024 * 1024))];
        incremental.persist(request(&tabs), 1).await.unwrap();
        full_scan.persist(request(&tabs), 1).await.unwrap();
        for generation in 2..=10 {
            let length = tabs[0].active_buffer().document_snapshot().len_chars();
            let at = match generation % 3 {
                0 => 0,
                1 => length / 2,
                _ => length,
            };
            insert(&mut tabs[0], at, "small edit λ");
            full_scan.chunk_cache.clear();
            let (full, delta) = if generation % 2 == 0 {
                let full = full_scan.persist(request(&tabs), generation).await.unwrap();
                (
                    full,
                    incremental
                        .persist(request(&tabs), generation)
                        .await
                        .unwrap(),
                )
            } else {
                let delta = incremental
                    .persist(request(&tabs), generation)
                    .await
                    .unwrap();
                (
                    full_scan.persist(request(&tabs), generation).await.unwrap(),
                    delta,
                )
            };
            println!(
                "round={generation} position={at} full_ms={:.3} incremental_ms={:.3} full_read={} incremental_read={} full_hashed={} incremental_hashed={}",
                full.total_ns as f64 / 1e6,
                delta.total_ns as f64 / 1e6,
                full.snapshot_bytes_read,
                delta.snapshot_bytes_read,
                full.snapshot_bytes_hashed,
                delta.snapshot_bytes_hashed
            );
            assert!(delta.snapshot_bytes_hashed < 1024 * 1024);
        }
    });
}

#[test]
fn failed_incremental_write_and_undo_remain_retryable() {
    block_on(async {
        let (_root, mut db) = database().await;
        let original = random_text(4 * 1024 * 1024);
        let mut tabs = vec![tab(&original)];
        db.persist(request(&tabs), 1).await.unwrap();
        insert(&mut tabs[0], original.len() / 2, "local edit");
        db.conn.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON session_state BEGIN SELECT RAISE(ABORT, 'injected'); END;").await.unwrap();
        assert!(db.persist(request(&tabs), 2).await.is_err());
        assert!(db.chunk_cache.is_empty());
        db.conn.execute("DROP TRIGGER fail_save", ()).await.unwrap();
        let retry = db.persist(request(&tabs), 3).await.unwrap();
        assert_eq!(
            retry.snapshot_bytes_hashed as usize,
            tabs[0].active_buffer().document_snapshot().len_bytes()
        );
        tabs[0]
            .buffers
            .buffer
            .document_mut()
            .undo_last_operation()
            .unwrap();
        db.persist(request(&tabs), 4).await.unwrap();
        assert_eq!(
            db.read_all_contents()
                .await
                .unwrap()
                .into_values()
                .next()
                .unwrap(),
            original
        );
        insert(&mut tabs[0], 0, "small front edit");
        let incremental = db.persist(request(&tabs), 5).await.unwrap();
        assert!(incremental.snapshot_bytes_hashed < 512 * 1024);
        assert!(incremental.snapshot_bytes_read < 1024 * 1024);
        assert_eq!(
            db.read_all_contents()
                .await
                .unwrap()
                .into_values()
                .next()
                .unwrap(),
            tabs[0].active_buffer().text()
        );
    });
}
