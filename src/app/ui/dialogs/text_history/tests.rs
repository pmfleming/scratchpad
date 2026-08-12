use super::model::{
    newest_applied_index, per_file_now_line_insert_index, timeline_now_line_insert_index,
    timeline_rows_from_entries,
};
use super::{
    HistoryTab, TextHistoryFileGroup, TextHistoryRow, file_groups_from_entries, read_active_tab,
    read_follow_focus, write_active_tab, write_follow_focus,
};
use crate::app::domain::BufferId;
use crate::app::domain::PieceSource;
use crate::app::text_history::TextHistoryEntryView;

#[test]
fn history_dialog_defaults_to_by_file_tab() {
    let ctx = eframe::egui::Context::default();

    assert_eq!(read_active_tab(&ctx), HistoryTab::ByFile);
}

#[test]
fn history_tab_persistence_round_trips() {
    let ctx = eframe::egui::Context::default();

    write_active_tab(&ctx, HistoryTab::Timeline);
    assert_eq!(read_active_tab(&ctx), HistoryTab::ByFile);
    advance_egui_frame(&ctx);
    assert_eq!(read_active_tab(&ctx), HistoryTab::Timeline);

    write_active_tab(&ctx, HistoryTab::ByFile);
    assert_eq!(read_active_tab(&ctx), HistoryTab::Timeline);
    advance_egui_frame(&ctx);
    assert_eq!(read_active_tab(&ctx), HistoryTab::ByFile);
}

#[test]
fn follow_focus_defaults_to_enabled() {
    let ctx = eframe::egui::Context::default();

    assert!(read_follow_focus(&ctx));
}

#[test]
fn follow_focus_persistence_round_trips() {
    let ctx = eframe::egui::Context::default();

    write_follow_focus(&ctx, false);
    assert!(read_follow_focus(&ctx));
    advance_egui_frame(&ctx);
    assert!(!read_follow_focus(&ctx));

    write_follow_focus(&ctx, true);
    assert!(!read_follow_focus(&ctx));
    advance_egui_frame(&ctx);
    assert!(read_follow_focus(&ctx));
}

fn advance_egui_frame(ctx: &eframe::egui::Context) {
    let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
}

#[test]
fn per_file_now_line_sits_between_applied_and_undone_rows() {
    let rows = vec![row(true), row(true), row(false), row(false)];

    let newest_applied = newest_applied_index(&rows);

    assert_eq!(newest_applied, Some(2));
    assert_eq!(per_file_now_line_insert_index(&rows), Some(2));
}

#[test]
fn per_file_now_line_is_suppressed_when_it_would_sit_at_top() {
    let rows = vec![row(true), row(true)];

    let newest_applied = newest_applied_index(&rows);

    assert_eq!(newest_applied, None);
    assert_eq!(per_file_now_line_insert_index(&rows), None);
}

#[test]
fn per_file_now_line_is_suppressed_when_everything_is_applied() {
    let rows = vec![row(false), row(false)];

    assert_eq!(newest_applied_index(&rows), Some(0));
    assert_eq!(per_file_now_line_insert_index(&rows), None);
}

#[test]
fn per_file_now_line_is_absent_for_empty_file_history() {
    assert_eq!(per_file_now_line_insert_index(&[]), None);
}

#[test]
fn per_file_groups_keep_rows_in_reverse_chronological_order() {
    let entries = [
        entry(1, 10, 1, "one"),
        entry(2, 11, 2, "other"),
        entry(3, 12, 1, "two"),
    ];

    let groups = file_groups_from_entries(entries.iter());

    assert_eq!(group_entry_ids(&groups, 1), vec![3, 1]);
}

#[test]
fn per_file_groups_put_redoable_rows_above_undoable_rows() {
    let mut entries = [
        entry(1, 10, 1, "one"),
        entry(2, 11, 1, "two"),
        entry(3, 12, 1, "three"),
    ];
    entries[1].undone = true;

    let groups = file_groups_from_entries(entries.iter());

    assert_eq!(group_entry_ids(&groups, 1), vec![2, 3, 1]);
    assert_eq!(per_file_now_line_insert_index(&groups[0].rows), Some(1));
}

#[test]
fn per_file_groups_sort_by_latest_change_first() {
    let entries = [
        entry(1, 10, 1, "one"),
        entry(2, 12, 2, "other"),
        entry(3, 11, 1, "two"),
    ];

    let groups = file_groups_from_entries(entries.iter());

    assert_eq!(group_buffer_ids(&groups), vec![2, 1]);
}

#[test]
fn per_file_rows_preserve_buffer_id_when_entry_ids_collide() {
    let entries = [entry(1, 10, 1, "one"), entry(1, 11, 2, "other")];

    let groups = file_groups_from_entries(entries.iter());

    assert_eq!(group_targets(&groups, 1), vec![(1, 1)]);
    assert_eq!(group_targets(&groups, 2), vec![(2, 1)]);
}

fn row(undone: bool) -> TextHistoryRow {
    TextHistoryRow {
        buffer_id: 0,
        entry_id: 0,
        global_seq: 0,
        title: String::new(),
        detail: String::new(),
        icon: "",
        undone,
    }
}

fn timeline_row(buffer_id: BufferId, entry_id: u64, undone: bool) -> TextHistoryRow {
    TextHistoryRow {
        buffer_id,
        entry_id,
        global_seq: entry_id,
        title: String::new(),
        detail: String::new(),
        icon: "",
        undone,
    }
}

#[test]
fn timeline_rows_put_redoable_aggregate_above_undoable_aggregate() {
    let rows = [
        timeline_row(3, 7, true),
        timeline_row(3, 6, true),
        timeline_row(2, 5, false),
        timeline_row(2, 4, false),
        timeline_row(1, 3, true),
        timeline_row(1, 2, false),
        timeline_row(1, 1, false),
    ];

    let mut entries = rows
        .iter()
        .map(|row| entry(row.entry_id, row.global_seq, row.buffer_id, "change"))
        .collect::<Vec<_>>();
    for (entry, row) in entries.iter_mut().zip(rows.iter()) {
        entry.undone = row.undone;
    }
    let timeline_rows = timeline_rows_from_entries(entries.iter());

    assert_eq!(entry_ids(&timeline_rows), vec![7, 6, 3, 5, 4, 2, 1]);
    assert_eq!(timeline_now_line_insert_index(&timeline_rows), Some(3));
}

#[test]
fn timeline_now_line_has_one_global_boundary_for_multiple_buffers() {
    let rows = [
        timeline_row(1, 2, true),
        timeline_row(1, 1, false),
        timeline_row(2, 5, true),
        timeline_row(2, 4, false),
        timeline_row(2, 3, false),
    ];

    let mut entries = rows
        .iter()
        .map(|row| entry(row.entry_id, row.global_seq, row.buffer_id, "change"))
        .collect::<Vec<_>>();
    for (entry, row) in entries.iter_mut().zip(rows.iter()) {
        entry.undone = row.undone;
    }
    let timeline_rows = timeline_rows_from_entries(entries.iter());

    assert_eq!(entry_ids(&timeline_rows), vec![5, 2, 4, 3, 1]);
    assert_eq!(timeline_now_line_insert_index(&timeline_rows), Some(2));
}

#[test]
fn timeline_now_line_is_absent_when_no_history() {
    let rows: Vec<TextHistoryRow> = Vec::new();

    assert_eq!(timeline_now_line_insert_index(&rows), None);
}

#[test]
fn timeline_now_line_is_absent_when_everything_is_applied() {
    let rows = vec![timeline_row(1, 2, false), timeline_row(1, 1, false)];

    assert_eq!(timeline_now_line_insert_index(&rows), None);
}

#[test]
fn timeline_now_line_is_absent_when_everything_is_undone() {
    let rows = vec![timeline_row(1, 2, true), timeline_row(1, 1, true)];

    assert_eq!(timeline_now_line_insert_index(&rows), None);
}

fn entry(id: u64, global_seq: u64, buffer_id: u64, summary: &str) -> TextHistoryEntryView {
    TextHistoryEntryView {
        id,
        global_seq,
        buffer_id,
        label: format!("file-{buffer_id}"),
        source: PieceSource::Edit,
        summary: summary.to_owned(),
        undone: false,
        replayable: true,
        edit_count: 1,
        first_deleted_text: String::new(),
        first_inserted_text: summary.to_owned(),
    }
}

fn group_entry_ids(groups: &[TextHistoryFileGroup], buffer_id: u64) -> Vec<u64> {
    groups
        .iter()
        .find(|group| group.buffer_id == buffer_id)
        .map(|group| group.rows.iter().map(|row| row.entry_id).collect())
        .unwrap_or_default()
}

fn group_buffer_ids(groups: &[TextHistoryFileGroup]) -> Vec<u64> {
    groups.iter().map(|group| group.buffer_id).collect()
}

fn group_targets(groups: &[TextHistoryFileGroup], buffer_id: u64) -> Vec<(u64, u64)> {
    groups
        .iter()
        .find(|group| group.buffer_id == buffer_id)
        .map(|group| {
            group
                .rows
                .iter()
                .map(|row| (row.buffer_id, row.entry_id))
                .collect()
        })
        .unwrap_or_default()
}

fn entry_ids(rows: &[TextHistoryRow]) -> Vec<u64> {
    rows.iter().map(|row| row.entry_id).collect()
}
