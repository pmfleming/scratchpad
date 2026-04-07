use scratchpad::app::domain::BufferState;
use scratchpad::app::domain::{EditorViewState, PaneBranch, PaneNode, SplitAxis, WorkspaceTab};

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
    let existing_view = EditorViewState::new(false);
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
