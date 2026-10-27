use super::{
    HistoryTab, TextHistoryFileGroup, file_groups_from_entries,
    model::{now_line_insert_index, row_from_entry, timeline_rows_from_entries},
    read_active_tab, read_follow_focus, write_active_tab, write_follow_focus,
};
use crate::app::{domain::PieceSource, text_history::TextHistoryEntryView};

#[test]
fn history_preferences_default_and_change_only_on_the_next_frame() {
    let ctx = eframe::egui::Context::default();
    assert_eq!(read_active_tab(&ctx), HistoryTab::ByFile);
    assert!(read_follow_focus(&ctx));
    for (old, new, follow) in [
        (HistoryTab::ByFile, HistoryTab::Timeline, false),
        (HistoryTab::Timeline, HistoryTab::ByFile, true),
    ] {
        write_active_tab(&ctx, new);
        write_follow_focus(&ctx, follow);
        assert_eq!(read_active_tab(&ctx), old);
        assert_eq!(read_follow_focus(&ctx), !follow);
        let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        assert_eq!(read_active_tab(&ctx), new);
        assert_eq!(read_follow_focus(&ctx), follow);
    }
}

#[test]
fn now_line_is_only_between_redoable_and_applied_rows() {
    for (states, boundary) in [
        (&[][..], None),
        (&[true, true][..], None),
        (&[false, false][..], None),
        (&[true, true, false, false][..], Some(2)),
    ] {
        let rows = states
            .iter()
            .map(|&undone| {
                let mut entry = entry(1, 1, 1, "change");
                entry.undone = undone;
                row_from_entry(&entry)
            })
            .collect::<Vec<_>>();
        assert_eq!(now_line_insert_index(&rows), boundary, "{states:?}");
    }
}

#[test]
fn per_file_groups_keep_rows_in_reverse_chronological_order() {
    let entries = [
        entry(1, 10, 1, "one"),
        entry(2, 11, 2, "other"),
        entry(3, 12, 1, "two"),
    ];
    assert_eq!(
        group_targets(&file_groups_from_entries(entries.iter()), 1),
        [(1, 3), (1, 1)]
    );
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
    assert_eq!(group_targets(&groups, 1), [(1, 2), (1, 3), (1, 1)]);
    assert_eq!(now_line_insert_index(&groups[0].rows), Some(1));
}

#[test]
fn per_file_groups_sort_by_latest_change_first() {
    let entries = [
        entry(1, 10, 1, "one"),
        entry(2, 12, 2, "other"),
        entry(3, 11, 1, "two"),
    ];
    let groups = file_groups_from_entries(entries.iter());
    assert_eq!(
        groups
            .iter()
            .map(|group| group.buffer_id)
            .collect::<Vec<_>>(),
        [2, 1]
    );
}

#[test]
fn per_file_rows_preserve_buffer_id_when_entry_ids_collide() {
    let entries = [entry(1, 10, 1, "one"), entry(1, 11, 2, "other")];
    let groups = file_groups_from_entries(entries.iter());
    assert_eq!(group_targets(&groups, 1), [(1, 1)]);
    assert_eq!(group_targets(&groups, 2), [(2, 1)]);
}

#[test]
fn timeline_aggregates_redoable_rows_before_one_global_boundary() {
    for (input, expected, boundary) in [
        (
            &[
                (3, 7, true),
                (3, 6, true),
                (2, 5, false),
                (2, 4, false),
                (1, 3, true),
                (1, 2, false),
                (1, 1, false),
            ][..],
            &[7, 6, 3, 5, 4, 2, 1][..],
            3,
        ),
        (
            &[
                (1, 2, true),
                (1, 1, false),
                (2, 5, true),
                (2, 4, false),
                (2, 3, false),
            ][..],
            &[5, 2, 4, 3, 1][..],
            2,
        ),
    ] {
        let entries = input
            .iter()
            .map(|&(buffer, id, undone)| {
                let mut entry = entry(id, id, buffer, "change");
                entry.undone = undone;
                entry
            })
            .collect::<Vec<_>>();
        let rows = timeline_rows_from_entries(entries.iter());
        assert_eq!(
            rows.iter().map(|row| row.entry_id).collect::<Vec<_>>(),
            expected,
            "{input:?}"
        );
        assert_eq!(now_line_insert_index(&rows), Some(boundary));
    }
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

fn group_targets(groups: &[TextHistoryFileGroup], buffer_id: u64) -> Vec<(u64, u64)> {
    groups
        .iter()
        .find(|group| group.buffer_id == buffer_id)
        .unwrap()
        .rows
        .iter()
        .map(|row| (row.buffer_id, row.entry_id))
        .collect()
}
