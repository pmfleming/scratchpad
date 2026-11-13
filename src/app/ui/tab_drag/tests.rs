use super::state::TabDropIntent;
use super::{
    TabDragCommit, TabDropAxis, TabDropZone, TabRectEntry, commit_for_drop,
    drag_sources_allow_combine,
};
use eframe::egui;

fn reorder(drop_slot: usize) -> TabDropIntent {
    TabDropIntent::Reorder {
        zone_index: 0,
        drop_slot,
    }
}

fn combine(target_index: usize) -> TabDropIntent {
    TabDropIntent::Combine {
        zone_index: 0,
        target_index,
    }
}

#[test]
fn released_drags_commit_only_changes_that_move_or_merge_tabs() {
    let single = |intent| commit_for_drop(intent, 1, vec![1], 4);
    assert!(matches!(
        single(reorder(4)),
        Some(TabDragCommit::Reorder {
            from_index: 1,
            to_index: 3
        })
    ));
    assert!(single(reorder(1)).is_none());
    assert!(single(reorder(2)).is_none());
    assert!(matches!(
        single(combine(3)),
        Some(TabDragCommit::Combine {
            source_index: 1,
            target_index: 3
        })
    ));
    assert!(single(combine(1)).is_none());

    let group = |intent| commit_for_drop(intent, 1, vec![1, 2], 4);
    assert!(matches!(
        group(reorder(9)),
        Some(TabDragCommit::ReorderGroup { from_indices, to_index: 4 }) if from_indices == [1, 2]
    ));
    assert!(matches!(
        group(combine(0)),
        Some(TabDragCommit::CombineGroup { source_indices, target_index: 0 }) if source_indices == [1, 2]
    ));
    assert!(group(combine(2)).is_none());
}

#[test]
fn combining_needs_a_combinable_source_or_group_member() {
    let entry = |index, combine_enabled| TabRectEntry {
        index,
        rect: egui::Rect::NOTHING,
        combine_enabled,
    };
    let zones = [TabDropZone {
        axis: TabDropAxis::Horizontal,
        entries: vec![entry(0, true), entry(1, false), entry(2, true)],
    }];
    for (dragged, source, expected) in [
        (&[0][..], 0, true),
        (&[1], 1, false),
        (&[1, 2], 1, true),
        (&[1, 3], 1, false),
        (&[3], 3, false),
    ] {
        assert_eq!(
            drag_sources_allow_combine(&zones, dragged, source),
            expected,
            "{dragged:?}"
        );
    }
}
