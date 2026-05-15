use scratchpad::app::domain::{BufferState, PaneNode, SplitAxis, WorkspaceTab};

fn buffer(name: &str, text: &str) -> BufferState {
    BufferState::new(name.to_owned(), text.to_owned(), None)
}

fn collect_leaf_area_fractions(node: &PaneNode, area_fraction: f32, output: &mut Vec<f32>) {
    match node {
        PaneNode::Leaf { .. } => output.push(area_fraction),
        PaneNode::Split {
            first,
            second,
            ratio,
            ..
        } => {
            collect_leaf_area_fractions(first, area_fraction * *ratio, output);
            collect_leaf_area_fractions(second, area_fraction * (1.0 - *ratio), output);
        }
    }
}

#[test]
fn splitting_and_closing_views_updates_pane_tree() {
    let mut tab = WorkspaceTab::new(buffer("one.txt", "one"));
    let original_view = tab.layout.active_view_id;
    let split_view = tab.split_active_view(SplitAxis::Vertical).unwrap();

    assert_eq!(tab.layout.views.len(), 2);
    assert!(tab.layout.root_pane.contains_view(original_view));
    assert!(tab.layout.root_pane.contains_view(split_view));

    assert!(tab.close_view(split_view));

    assert_eq!(tab.layout.views.len(), 1);
    assert!(tab.layout.root_pane.contains_view(original_view));
}

#[test]
fn open_buffer_as_split_tracks_distinct_file_group() {
    let mut tab = WorkspaceTab::new(buffer("one.txt", "one"));

    let view_id = tab
        .open_buffer_as_split(buffer("two.txt", "two"), SplitAxis::Horizontal, true, 0.4)
        .unwrap();

    assert_eq!(tab.file_group_count(), 2);
    assert_eq!(tab.layout.active_view_id, view_id);
    assert_eq!(tab.active_buffer().name, "two.txt");
    assert!(tab.display_name().contains("one.txt"));
    assert!(tab.display_name().contains("two.txt"));
}

#[test]
fn combining_tabs_merges_buffers_and_focuses_source_workspace() {
    let mut target = WorkspaceTab::new(buffer("target.txt", "target"));
    let source = WorkspaceTab::new(buffer("source.txt", "source"));
    let source_active = source.layout.active_view_id;

    let active = target
        .combine_with_tab(source, SplitAxis::Vertical, false, 0.5)
        .unwrap();

    assert_eq!(active, source_active);
    assert_eq!(target.layout.active_view_id, source_active);
    assert_eq!(target.file_group_count(), 2);
    assert_eq!(target.active_buffer().text(), "source");
}

#[test]
fn rebalancing_views_shares_space_equally() {
    let mut tab = WorkspaceTab::new(buffer("one.txt", "one"));
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.split_active_view(SplitAxis::Horizontal).unwrap();

    tab.rebalance_views_equally_for_axis(SplitAxis::Vertical);

    let mut fractions = Vec::new();
    collect_leaf_area_fractions(&tab.layout.root_pane, 1.0, &mut fractions);
    fractions.sort_by(f32::total_cmp);

    assert_eq!(fractions.len(), 3);
    for fraction in fractions {
        assert!((fraction - (1.0 / 3.0)).abs() < 0.001);
    }
}
