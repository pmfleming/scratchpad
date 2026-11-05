use super::{
    error,
    tests::{database, insert, request, tab},
};
use futures_lite::future::block_on;

#[test]
fn pages_use_total_sequence_order_skip_baselines_and_never_read_edit_payloads() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut tabs = vec![tab("one"), tab("two")];
        for i in 0..130 {
            let index = i % 2;
            insert(&mut tabs[index], 0, &format!("{i}"));
        }
        db.persist(request(&tabs), 1).await.unwrap();
        // Deliberately corrupt payloads: drawing the timeline must only use columns.
        db.conn
            .execute(
                "UPDATE history SET entry=x'00',global_seq=42 WHERE edited_at<>0",
                (),
            )
            .await
            .unwrap();
        let mut ids = std::collections::HashSet::new();
        let mut before = None;
        loop {
            let page = db
                .timeline_page(before, vec![], String::new())
                .await
                .unwrap();
            assert!(page.rows.len() <= 100);
            for pair in page.rows.windows(2) {
                assert!(pair[0].cursor > pair[1].cursor);
            }
            for row in page.rows {
                assert!(row.timestamp > 0);
                assert!(ids.insert(row.id()));
            }
            before = page.older;
            if before.is_none() {
                break;
            }
        }
        assert_eq!(ids.len(), 130);
        let page = db
            .timeline_page(
                None,
                vec![tabs[0].active_buffer().temp_id.clone()],
                String::new(),
            )
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 65);
        assert!(page.older.is_none());
    });
}

#[test]
fn characterize_union_pagination_plan_before_adding_activity() {
    block_on(async {
        let (_root, db) = database().await;
        let plan = db.query_collect("EXPLAIN QUERY PLAN SELECT global_seq FROM history UNION ALL SELECT global_seq FROM activity ORDER BY global_seq DESC LIMIT 101",(), |row| row.get::<String>(3).map_err(error)).await.unwrap();
        // Keep engine evidence visible when this probe is run with --nocapture.
        eprintln!("Turso 0.8.3 timeline UNION plan: {plan:?}");
        assert!(!plan.is_empty());
    });
}

#[test]
fn zero_edit_timestamps_and_clock_changes_do_not_hide_or_reorder_activity() {
    block_on(async {
        let (_root, mut db) = database().await;
        let mut live = tab("initial");
        insert(&mut live, 0, "first");
        insert(&mut live, 0, "second");
        db.persist(request(&[live]), 1).await.unwrap();
        db.conn
            .execute(
                "UPDATE history SET edited_at=0,global_seq=9223372036854775807 WHERE seq=3",
                (),
            )
            .await
            .unwrap();
        let page = db.timeline_page(None, vec![], String::new()).await.unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].cursor.node, 3);
        assert_eq!(page.rows[0].timestamp, 0);
        assert!(page.rows[0].cursor > page.rows[1].cursor);
    });
}
