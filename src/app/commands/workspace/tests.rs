use super::super::{AppCommand, WorkspaceCommand, handle_command};
use crate::app::app_state::test_support::test_app;
use crate::app::domain::{BufferState, SplitAxis, TabManager, TileDirection, WorkspaceTab};

fn run(app: &mut crate::app::app_state::ScratchpadApp, command: WorkspaceCommand) -> bool {
    handle_command(app, AppCommand::Workspace(command))
}

#[test]
fn tile_commands_report_whether_the_layout_changed() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    app.tab_manager = TabManager::for_test_tabs(vec![WorkspaceTab::new(BufferState::new(
        "note".into(),
        "text".into(),
        None,
    ))]);
    let active_view = |app: &crate::app::app_state::ScratchpadApp| {
        app.tab_manager
            .active_tab()
            .unwrap()
            .layout
            .active_view_id()
    };
    let leaf_count = |app: &crate::app::app_state::ScratchpadApp| {
        app.tab_manager.active_tab().unwrap().layout.leaf_count()
    };
    let first = active_view(&app);

    // A lone view has nowhere to move, nothing to resize and must stay open.
    let left = TileDirection::Left;
    assert!(!run(
        &mut app,
        WorkspaceCommand::MoveActiveTile { direction: left }
    ));
    assert!(!run(
        &mut app,
        WorkspaceCommand::ResizeActiveTile { direction: left }
    ));
    assert!(!run(
        &mut app,
        WorkspaceCommand::CloseView { view_id: first }
    ));

    assert!(run(
        &mut app,
        WorkspaceCommand::SplitActiveView {
            axis: SplitAxis::Vertical,
            new_view_first: false,
            ratio: 0.5,
        }
    ));
    let second = active_view(&app);
    assert_ne!(second, first);
    assert_eq!(leaf_count(&app), 2);
    assert!(run(
        &mut app,
        WorkspaceCommand::ResizeActiveTile { direction: left }
    ));
    assert!(run(
        &mut app,
        WorkspaceCommand::MoveActiveTile { direction: left }
    ));
    assert_eq!(active_view(&app), second);

    assert!(run(
        &mut app,
        WorkspaceCommand::CloseView { view_id: second }
    ));
    // The close commits once the session store has persisted it.
    super::perform_close_view_committed(&mut app, second);
    assert_eq!(leaf_count(&app), 1);
    assert_eq!(active_view(&app), first);

    for out_of_range in [
        WorkspaceCommand::ActivateTab { index: 5 },
        WorkspaceCommand::RequestCloseTab { index: 5 },
        WorkspaceCommand::ReorderTab {
            from_index: 0,
            to_index: 5,
        },
    ] {
        assert!(!run(&mut app, out_of_range));
    }
}
