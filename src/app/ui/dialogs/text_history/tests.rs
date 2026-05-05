use super::{
    TextHistoryFileGroup, TextHistoryRow, file_groups_from_entries, newest_applied_index,
    per_file_now_line_insert_index, read_follow_focus, timeline_now_line_anchors,
    write_follow_focus,
};
use crate::app::domain::BufferId;
use crate::app::domain::PieceSource;
use crate::app::text_history::TextHistoryEntryView;

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
    let _ = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
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
fn timeline_now_line_anchors_one_per_buffer_in_mixed_state() {
    // Buffer 1: entries 3 (undone), 2 (applied), 1 (applied)
    // Buffer 2: entries 5 (applied), 4 (applied) — all applied, no anchor
    // Buffer 3: entries 7 (undone), 6 (undone) — all undone, no anchor
    let rows = vec![
        timeline_row(3, 7, true),
        timeline_row(3, 6, true),
        timeline_row(2, 5, false),
        timeline_row(2, 4, false),
        timeline_row(1, 3, true),
        timeline_row(1, 2, false),
        timeline_row(1, 1, false),
    ];

    let anchors = timeline_now_line_anchors(&rows);

    assert_eq!(anchors.len(), 1);
    assert!(anchors.contains(&(1, 2)));
}

#[test]
fn timeline_now_line_anchors_multiple_buffers() {
    // Two buffers, both in mixed state — expect one anchor each.
    let rows = vec![
        timeline_row(1, 2, true),
        timeline_row(1, 1, false),
        timeline_row(2, 5, true),
        timeline_row(2, 4, false),
        timeline_row(2, 3, false),
    ];

    let anchors = timeline_now_line_anchors(&rows);

    assert_eq!(anchors.len(), 2);
    assert!(anchors.contains(&(1, 1)));
    assert!(anchors.contains(&(2, 4)));
}

#[test]
fn timeline_now_line_anchors_empty_when_no_history() {
    let rows: Vec<TextHistoryRow> = Vec::new();

    assert!(timeline_now_line_anchors(&rows).is_empty());
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
