#![forbid(unsafe_code)]

use scratchpad::app::domain::BufferState;
use scratchpad::app::domain::{EditorViewState, PaneBranch, PaneNode, SplitAxis, WorkspaceTab};

fn collect_leaf_area_fractions(node: &PaneNode, area_fraction: f32, output: &mut Vec<f32>) {
    match node {
        PaneNode::Leaf { .. } => output.push(area_fraction),
        PaneNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            collect_leaf_area_fractions(first, area_fraction * ratio, output);
            collect_leaf_area_fractions(second, area_fraction * (1.0 - ratio), output);
        }
    }
}

#[test]
fn untitled_workspace_tab_wraps_untitled_buffer() {
    let tab = WorkspaceTab::untitled();

    assert_eq!(tab.buffer.name, "Untitled");
    assert!(tab.buffer.path.is_none());
    assert_eq!(tab.views.len(), 1);
    assert!(matches!(tab.root_pane, PaneNode::Leaf { .. }));
}

#[test]
fn splitting_and_closing_views_updates_pane_tree() {
    let mut tab = WorkspaceTab::untitled();
    let original_view_id = tab.active_view_id;
    let new_view_id = tab.split_active_view(SplitAxis::Vertical).unwrap();

    assert_eq!(tab.views.len(), 2);
    assert!(matches!(tab.root_pane, PaneNode::Split { .. }));
    assert_eq!(tab.active_view_id, new_view_id);

    assert!(tab.close_view(new_view_id));
    assert_eq!(tab.views.len(), 1);
    assert!(matches!(tab.root_pane, PaneNode::Leaf { .. }));
    assert_eq!(tab.active_view_id, original_view_id);
}

#[test]
fn resizing_split_updates_target_ratio() {
    let mut tab = WorkspaceTab::untitled();
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    let first_view_id = match &tab.root_pane {
        PaneNode::Split { first, .. } => match first.as_ref() {
            PaneNode::Leaf { view_id } => *view_id,
            PaneNode::Split { .. } => panic!("expected first child leaf"),
        },
        PaneNode::Leaf { .. } => panic!("expected split root"),
    };

    tab.active_view_id = first_view_id;
    tab.split_active_view(SplitAxis::Horizontal).unwrap();

    assert!(tab.resize_split(vec![], 0.68));
    assert!(tab.resize_split(vec![PaneBranch::First], 0.34));

    match &tab.root_pane {
        PaneNode::Split { ratio, first, .. } => {
            assert_eq!(*ratio, 0.68);
            match first.as_ref() {
                PaneNode::Split { ratio, .. } => assert_eq!(*ratio, 0.34),
                PaneNode::Leaf { .. } => panic!("expected nested split"),
            }
        }
        PaneNode::Leaf { .. } => panic!("expected split root"),
    }
}

#[test]
fn split_preview_placement_can_create_new_first_child() {
    let mut tab = WorkspaceTab::untitled();
    let original_view_id = tab.active_view_id;
    let new_view_id = tab
        .split_active_view_with_placement(SplitAxis::Vertical, true, 0.35)
        .unwrap();

    match &tab.root_pane {
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            assert_eq!(*axis, SplitAxis::Vertical);
            assert_eq!(*ratio, 0.35);
            assert!(
                matches!(first.as_ref(), PaneNode::Leaf { view_id } if *view_id == new_view_id)
            );
            assert!(
                matches!(second.as_ref(), PaneNode::Leaf { view_id } if *view_id == original_view_id)
            );
        }
        PaneNode::Leaf { .. } => panic!("expected split root"),
    }
}

#[test]
fn restored_tab_repairs_missing_pane_views() {
    let buffer = BufferState::new("Untitled".to_owned(), String::new(), None);
    let existing_view = EditorViewState::new(buffer.id, false);
    let existing_view_id = existing_view.id;
    let missing_view_id = existing_view_id + 10_000;

    let tab = WorkspaceTab::restored(
        buffer,
        vec![existing_view],
        PaneNode::Split {
            axis: SplitAxis::Vertical,
            ratio: 0.5,
            first: Box::new(PaneNode::Leaf {
                view_id: missing_view_id,
            }),
            second: Box::new(PaneNode::Leaf {
                view_id: existing_view_id,
            }),
        },
        missing_view_id,
    );

    assert_eq!(tab.views.len(), 1);
    assert!(matches!(tab.root_pane, PaneNode::Leaf { view_id } if view_id == existing_view_id));
    assert_eq!(tab.active_view_id, existing_view_id);
}

#[test]
fn combining_tabs_merges_buffers_and_focuses_source_workspace() {
    let mut target = WorkspaceTab::new(BufferState::new(
        "left.txt".to_owned(),
        "left".to_owned(),
        None,
    ));
    let source = WorkspaceTab::new(BufferState::new(
        "right.txt".to_owned(),
        "right".to_owned(),
        None,
    ));
    let source_active_view_id = source.active_view_id;
    let source_buffer_id = source.active_buffer().id;

    let combined_view_id = target
        .combine_with_tab(source, SplitAxis::Vertical, false, 0.5)
        .expect("combine should succeed");

    assert_eq!(combined_view_id, source_active_view_id);
    assert_eq!(target.views.len(), 2);
    assert_eq!(target.active_view_id, source_active_view_id);
    assert_eq!(target.active_buffer().id, source_buffer_id);
    assert_eq!(target.active_buffer().text(), "right");
    assert!(matches!(target.root_pane, PaneNode::Split { .. }));
}

#[test]
fn rebalancing_views_shares_space_equally() {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "one.txt".to_owned(),
        "one".to_owned(),
        None,
    ));

    for (name, content) in [
        ("two.txt", "two"),
        ("three.txt", "three"),
        ("four.txt", "four"),
    ] {
        tab.open_buffer_with_balanced_layout(BufferState::new(
            name.to_owned(),
            content.to_owned(),
            None,
        ))
        .expect("balanced open should succeed");
    }

    assert!(tab.rebalance_views_equally());

    let mut areas = Vec::new();
    collect_leaf_area_fractions(&tab.root_pane, 1.0, &mut areas);

    assert_eq!(tab.views.len(), 4);
    assert!(areas.iter().all(|area| (area - 0.25).abs() < f32::EPSILON));
}

#[test]
fn promoting_a_file_extracts_all_of_its_views_to_a_new_tab() {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "alpha.txt".to_owned(),
        "alpha".to_owned(),
        None,
    ));
    let original_view_id = tab.active_view_id;
    let second_alpha_view_id = tab
        .split_active_view(SplitAxis::Vertical)
        .expect("split should succeed");
    tab.activate_view(original_view_id);
    tab.open_buffer_as_split(
        BufferState::new("beta.txt".to_owned(), "beta".to_owned(), None),
        SplitAxis::Horizontal,
        false,
        0.5,
    )
    .expect("open buffer split should succeed");

    let promoted_tab = tab
        .promote_view_to_new_tab(second_alpha_view_id)
        .expect("promotion should succeed");

    assert_eq!(tab.views.len(), 1);
    assert_eq!(tab.active_buffer().name, "beta.txt");
    assert!(matches!(tab.root_pane, PaneNode::Leaf { .. }));

    assert_eq!(promoted_tab.views.len(), 2);
    assert_eq!(promoted_tab.active_view_id, second_alpha_view_id);
    assert_eq!(promoted_tab.active_buffer().name, "alpha.txt");
    assert!(matches!(promoted_tab.root_pane, PaneNode::Split { .. }));
}
