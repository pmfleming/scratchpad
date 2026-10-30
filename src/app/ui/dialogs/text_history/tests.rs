use super::{
    HistoryTab, TextHistoryFileGroup, file_groups_from_entries, model::timeline_rows_from_entries,
    read_active_tab, read_follow_focus, write_active_tab, write_follow_focus,
};
use crate::app::{domain::PieceSource, text_history::TextHistoryEntryView};

#[test]
fn history_preferences_default_to_current_file_and_defer_changes() {
    let ctx = eframe::egui::Context::default();
    assert_eq!(read_active_tab(&ctx), HistoryTab::CurrentFile);
    assert!(read_follow_focus(&ctx));
    for (old, new, follow) in [
        (HistoryTab::CurrentFile, HistoryTab::AllFiles, false),
        (HistoryTab::AllFiles, HistoryTab::CurrentFile, true),
    ] {
        write_active_tab(&ctx, new);
        write_follow_focus(&ctx, follow);
        assert_eq!(read_active_tab(&ctx), old);
        assert_eq!(read_follow_focus(&ctx), !follow);
        ctx.run_ui(eframe::egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        assert_eq!(read_active_tab(&ctx), new);
        assert_eq!(read_follow_focus(&ctx), follow);
    }
}

#[test]
fn picked_buffer_ids_are_not_persisted() {
    let ctx = eframe::egui::Context::default();
    write_active_tab(&ctx, HistoryTab::File(999));
    ctx.run_ui(eframe::egui::RawInput::default(), |_| {})
        .textures_delta
        .clear();
    assert_eq!(read_active_tab(&ctx), HistoryTab::CurrentFile);
}

#[test]
fn timeline_keeps_global_order_even_when_files_have_different_undo_positions() {
    let mut entries = [
        entry(1, 10, 1, "one"),
        entry(2, 11, 2, "other"),
        entry(3, 12, 1, "two"),
    ];
    entries[1].undone = true;
    let rows = timeline_rows_from_entries(entries.iter());
    assert_eq!(
        rows.iter()
            .map(|r| (r.buffer_id, r.entry_id))
            .collect::<Vec<_>>(),
        [(1, 3), (2, 2), (1, 1)]
    );
    assert_eq!(
        group_targets(&file_groups_from_entries(entries.iter()), 1),
        [(1, 3), (1, 1)]
    );
}

#[test]
fn file_groups_sort_by_latest_change_and_preserve_colliding_ids() {
    let entries = [
        entry(1, 10, 1, "one"),
        entry(1, 12, 2, "other"),
        entry(3, 11, 1, "two"),
    ];
    let groups = file_groups_from_entries(entries.iter());
    assert_eq!(
        groups.iter().map(|g| g.buffer_id).collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(group_targets(&groups, 1), [(1, 3), (1, 1)]);
    assert_eq!(group_targets(&groups, 2), [(2, 1)]);
}

fn entry(id: u64, global_seq: u64, buffer_id: u64, summary: &str) -> TextHistoryEntryView {
    TextHistoryEntryView {
        id,
        global_seq,
        buffer_id,
        label: format!("file-{buffer_id}"),
        source: PieceSource::Edit,
        summary: summary.into(),
        undone: false,
        replayable: true,
        edit_count: 1,
        first_deleted_text: String::new(),
        first_inserted_text: summary.into(),
    }
}
fn group_targets(groups: &[TextHistoryFileGroup], buffer_id: u64) -> Vec<(u64, u64)> {
    groups
        .iter()
        .find(|g| g.buffer_id == buffer_id)
        .unwrap()
        .rows
        .iter()
        .map(|r| (r.buffer_id, r.entry_id))
        .collect()
}
