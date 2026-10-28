use super::{open, show};
use crate::app::domain::{BufferState, WorkspaceTab};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;

#[test]
fn preview_actions_preserve_disk_and_keep_replacement_undoable() {
    for (action, follow_focus) in [
        ("Restore into this tab", true),
        ("Restore into this tab", false),
        ("Open as new tab", false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, "on disk").unwrap();
        let mut app = crate::app::app_state::test_support::test_app(root.path());
        app.tab_manager.set_tabs(
            vec![
                WorkspaceTab::new(BufferState::new(
                    "note".into(),
                    "current".into(),
                    Some(path.clone()),
                )),
                WorkspaceTab::new(BufferState::new("other".into(), "other".into(), None)),
            ],
            1,
        );
        let id = app.tab_manager.tabs.as_slice()[0].active_buffer().id;
        let recovered = Arc::new(format!("earlier{}", "é".repeat(33000)));
        let expected = recovered.clone();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .with_context_setup(move |ctx| {
                super::super::write_follow_focus(ctx, follow_focus);
                open(ctx, id, "Earlier note".into(), recovered.clone());
            })
            .build_ui_state(|ui, app| show(ui.ctx(), app), app);
        harness.run_steps(3);
        harness
            .get_by_role_and_label(egui::Role::Button, action)
            .click_accesskit();
        harness.run_steps(3);
        let app = harness.state_mut();
        let index = if action == "Restore into this tab" {
            0
        } else {
            app.tab_manager.active_tab_index
        };
        let buffer = app.tab_manager.tabs.as_mut_slice()[index].active_buffer_mut();
        assert_eq!(buffer.text(), expected.as_str(), "{action}");
        if action == "Restore into this tab" {
            assert!(buffer.is_dirty);
            assert_eq!(buffer.path.as_ref(), Some(&path));
            buffer.document_mut().undo_last_operation().unwrap();
            assert_eq!(buffer.text(), "current");
            assert_eq!(app.tab_manager.tabs.len(), 2);
            assert_eq!(app.tab_manager.active_tab_index, usize::from(!follow_focus));
        } else {
            assert!(buffer.path.is_none());
            assert_ne!(buffer.id, id);
            assert_eq!(app.tab_manager.tabs.len(), 3);
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
