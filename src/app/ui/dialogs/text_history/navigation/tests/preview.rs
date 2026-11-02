use super::super::super::preview;
use super::{app_with_history, displayed_text, harness, text};
use crate::app::{
    app_state::test_support::test_app,
    domain::{CursorRange, WorkspaceTab},
};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn previewing_a_split_file_uses_distinct_views_and_preserves_original_cursors() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let tab = app.tab_manager.active_tab_mut().unwrap();
    tab.split_active_view(crate::app::domain::SplitAxis::Vertical)
        .unwrap();
    let id = tab.active_buffer().id;
    let views: Vec<_> = tab.layout.views.iter().map(|view| view.id).collect();
    for (index, id) in views.into_iter().enumerate() {
        let (buffer, view) = tab.buffer_and_view_mut(id).unwrap();
        view.set_cursor_range_anchored(
            buffer,
            CursorRange::one(crate::app::domain::CharCursor::new(index + 1)),
        );
    }
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    let cursors: Vec<_> = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .layout
        .views
        .iter()
        .map(|view| (view.id, view.cursor_range))
        .collect();
    h.hover_at(
        h.get_by_label("State: Oldest retained state")
            .rect()
            .center(),
    );
    h.run_steps(4);
    assert_eq!(displayed_text(&h, id), "old");
    let buffer = h.state().tab_manager.active_tab().unwrap().active_buffer();
    let projection = preview::history_preview(&h.ctx, buffer).unwrap();
    {
        let projection = projection.lock().unwrap();
        assert_eq!(projection.tab.layout.views.len(), 2);
        assert_ne!(
            projection.tab.layout.views[0].id,
            projection.tab.layout.views[1].id
        );
        assert!(
            projection
                .tab
                .layout
                .views
                .iter()
                .all(|view| view.latest_display_snapshot.is_some())
        );
    }
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .layout
            .views
            .iter()
            .map(|view| (view.id, view.cursor_range))
            .collect::<Vec<_>>(),
        cursors
    );
}

#[test]
fn hover_changes_the_editor_projection_and_escape_preserves_text_history_and_session() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer.undo_last_text_operation().unwrap();
    let id = buffer.id;
    let dirty = buffer.is_dirty;
    let entries: Vec<_> = buffer
        .document()
        .history_entries()
        .iter()
        .map(|e| (e.id, e.is_undone()))
        .collect();
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "middle");
    h.hover_at(
        h.get_by_label("State: Oldest retained state")
            .rect()
            .center(),
    );
    h.run_steps(4);
    assert_eq!(displayed_text(&h, id), "old");
    assert_eq!(text(h.state()), "middle");
    let buffer = h.state().tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(buffer.is_dirty, dirty);
    let projection = preview::history_preview(&h.ctx, buffer).unwrap();
    assert!(
        projection
            .lock()
            .unwrap()
            .tab
            .layout
            .active_view()
            .unwrap()
            .latest_display_snapshot
            .is_some()
    );
    crate::app::services::session_manager::persist_session_now(h.state_mut()).unwrap();
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "middle");
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_entries()
            .iter()
            .map(|e| (e.id, e.is_undone()))
            .collect::<Vec<_>>(),
        entries
    );
    drop(h);
    let restored = test_app(root.path());
    assert_eq!(text(&restored), "middle");
    assert_eq!(
        restored
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_entries()
            .iter()
            .map(|e| e.is_undone())
            .collect::<Vec<_>>(),
        vec![false, true]
    );
}

#[test]
fn click_moves_history_position_and_clicking_outside_discards_the_next_preview() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let id = app.tab_manager.active_tab().unwrap().active_buffer().id;
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    h.hover_at(
        h.get_by_label("State: Oldest retained state")
            .rect()
            .center(),
    );
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "old");
    h.get_by_label("State: Oldest retained state").click();
    h.run_steps(4);
    assert_eq!(text(h.state()), "old");
    let buffer = h
        .state_mut()
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    assert_eq!(buffer.document().history_entries().len(), 2);
    assert!(
        buffer
            .document()
            .history_entries()
            .iter()
            .all(|entry| entry.is_undone())
    );
    buffer.redo_last_text_operation().unwrap();
    assert_eq!(buffer.text(), "middle");
    buffer.undo_last_text_operation().unwrap();
    assert_eq!(buffer.text(), "old");
    h.run_steps(3);
    let ctx = h.ctx.clone();
    let after = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .document()
        .history_entries()[1]
        .id;
    assert!(preview::select(&ctx, h.state_mut(), id, Some(after)));
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "current");
    let outside = egui::pos2(25.0, 700.0);
    h.hover_at(outside);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
    assert!(!h.state().state.dialogs.text_history.is_open());
    assert_eq!(displayed_text(&h, id), "old");
    assert_eq!(text(h.state()), "old");
}

#[test]
fn opening_reveals_the_current_state_and_hover_scrolls_both_edges_until_leaving() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    for i in 0..80 {
        let len = buffer.current_file_length().chars;
        buffer
            .replace_char_ranges_with_undo(
                &[(0..len, format!("state-{i}"))],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
    }
    for _ in 0..40 {
        buffer.undo_last_text_operation().unwrap();
    }
    let id = buffer.id;
    let summary = buffer
        .document()
        .history_entries()
        .iter()
        .rev()
        .find(|e| !e.is_undone())
        .unwrap()
        .summary
        .clone();
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(4);
    assert!(
        h.get_all_by_role_and_label(egui::Role::Button, &format!("State: {summary}"))
            .any(|node| node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True))
    );
    assert_eq!(displayed_text(&h, id), "state-39");
    let rect = h.get_by_label("History states").rect();
    h.hover_at(egui::pos2(rect.center().x, rect.bottom() - 5.0));
    h.run_steps(4);
    let before = displayed_text(&h, id);
    h.run_steps(40);
    let older = displayed_text(&h, id);
    assert_ne!(before, older, "bottom edge should keep scrolling");
    assert_eq!(text(h.state()), "state-39");
    h.hover_at(egui::pos2(rect.center().x, rect.top() + 5.0));
    h.run_steps(40);
    assert_ne!(displayed_text(&h, id), older, "top edge should scroll back");
    h.hover_at(egui::pos2(5.0, 700.0));
    h.run_steps(3);
    let stopped = displayed_text(&h, id);
    h.run_steps(30);
    assert_eq!(displayed_text(&h, id), stopped);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "state-39");
    h.state_mut().state.dialogs.text_history.open();
    h.run_steps(4);
    assert!(
        h.get_all_by_role_and_label(egui::Role::Button, &format!("State: {summary}"))
            .any(|node| node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True))
    );
}

#[test]
fn a_stale_preview_cannot_replace_an_edit_or_a_closed_file() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let ctx = egui::Context::default();
    let id = app.tab_manager.active_tab().unwrap().active_buffer().id;
    assert!(preview::select(&ctx, &mut app, id, None));
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer
        .replace_char_ranges_with_undo(
            &[(0..7, "new edit".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    assert!(
        preview::history_preview(&ctx, app.tab_manager.active_tab().unwrap().active_buffer())
            .is_none()
    );
    preview::close(&ctx, &mut app);
    assert_eq!(text(&app), "new edit");
    app.tab_manager.set_tabs(vec![WorkspaceTab::untitled()], 0);
    assert!(!preview::select(&ctx, &mut app, id, None));
}
