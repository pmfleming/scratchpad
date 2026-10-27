use super::{open, show};
use crate::app::domain::{BufferState, WorkspaceTab};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;

#[test]
fn preview_actions_preserve_disk_and_keep_replacement_undoable() {
    for action in ["Restore into this tab", "Open as new tab"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, "on disk").unwrap();
        let mut app = crate::app::app_state::test_support::test_app(root.path());
        app.tab_manager.set_tabs(
            vec![WorkspaceTab::new(BufferState::new(
                "note".into(),
                "current".into(),
                Some(path.clone()),
            ))],
            0,
        );
        let id = app.tab_manager.active_tab().unwrap().active_buffer().id;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .with_context_setup(move |ctx| {
                open(ctx, id, "Earlier note".into(), Arc::new("earlier".into()));
            })
            .build_ui_state(|ui, app| show(ui.ctx(), app), app);
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::Role::Button, action)
            .click_accesskit();
        harness.run_steps(3);
        let app = harness.state_mut();
        let buffer = app
            .tab_manager
            .active_tab_mut()
            .unwrap()
            .active_buffer_mut();
        assert_eq!(buffer.text(), "earlier", "{action}");
        if action == "Restore into this tab" {
            assert!(buffer.is_dirty);
            assert_eq!(buffer.path.as_ref(), Some(&path));
            buffer.document_mut().undo_last_operation().unwrap();
            assert_eq!(buffer.text(), "current");
            assert_eq!(app.tab_manager.tabs.len(), 1);
        } else {
            assert!(buffer.path.is_none());
            assert_ne!(buffer.id, id);
            assert_eq!(app.tab_manager.tabs.len(), 2);
            assert_eq!(
                app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
                "current"
            );
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "on disk");
        assert!(
            !harness
                .ctx
                .data(|data| data.get_temp::<super::Preview>(super::key()).is_some())
        );
    }
}
