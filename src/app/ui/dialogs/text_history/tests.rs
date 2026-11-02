use super::{
    HistoryTab,
    model::{entry_counts, timeline_rows_from_entries},
    read_active_tab, read_follow_focus, write_active_tab, write_follow_focus,
};
use crate::app::{domain::PieceSource, text_history::TextHistoryEntryView};

#[test]
fn history_preferences_defer_changes_and_never_persist_picked_buffers() {
    let ctx = eframe::egui::Context::default();
    assert_eq!(read_active_tab(&ctx), HistoryTab::CurrentFile);
    assert!(read_follow_focus(&ctx));
    for (new, expected, follow) in [
        (HistoryTab::AllFiles, HistoryTab::AllFiles, false),
        (HistoryTab::File(999), HistoryTab::CurrentFile, true),
        (HistoryTab::CurrentFile, HistoryTab::CurrentFile, false),
    ] {
        let old = read_active_tab(&ctx);
        write_active_tab(&ctx, new);
        write_follow_focus(&ctx, follow);
        assert_eq!(read_active_tab(&ctx), old);
        assert_eq!(read_follow_focus(&ctx), !follow);
        ctx.run_ui(eframe::egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        assert_eq!(read_active_tab(&ctx), expected);
        assert_eq!(read_follow_focus(&ctx), follow);
    }
}

#[test]
fn timeline_and_picker_keep_file_identity_with_ties_undo_and_colliding_entry_ids() {
    let entries: Vec<_> = [(1, 10, 1), (1, 12, 2), (3, 12, 1), (2, 12, 1)]
        .into_iter()
        .map(|(id, global_seq, buffer_id)| TextHistoryEntryView {
            id,
            global_seq,
            buffer_id,
            label: "same.txt".into(),
            source: PieceSource::Edit,
            summary: format!("edit-{id}"),
            undone: buffer_id == 2,
            replayable: true,
            edit_count: 1,
            deleted_preview: String::new(),
            inserted_preview: "é".into(),
        })
        .collect();
    let rows = timeline_rows_from_entries(entries.iter());
    assert_eq!(
        rows.iter()
            .map(|r| (r.buffer_id, r.entry_id))
            .collect::<Vec<_>>(),
        [(1, 2), (1, 3), (2, 1), (1, 1)]
    );
    assert!(rows[2].undone);
    assert_eq!(rows[0].title, "edit-2");
    assert_eq!(rows[0].inserted, "é");
    let counts = entry_counts(&entries);
    assert_eq!(counts[&1], 3);
    assert_eq!(counts[&2], 1);
    assert!(!counts.contains_key(&99));
    assert!(entry_counts(&[]).is_empty());
    assert!(timeline_rows_from_entries([].iter()).is_empty());
}
