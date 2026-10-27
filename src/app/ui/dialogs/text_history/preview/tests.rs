use super::{open, show};
use crate::app::domain::{BufferState, CursorRange, WorkspaceTab};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;

fn history_buffer() -> BufferState {
    let mut buffer = BufferState::new("note".into(), "old".into(), None);
    buffer
        .document_mut()
        .replace_char_ranges_with_undo(
            &[(0..3, "current".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    buffer
}

#[test]
fn preview_replay_validates_document_and_history_before_applying_undo_or_redo() {
    for redo in [false, true] {
        for case in ["valid", "anchor-only", "edited", "replaced", "closed"] {
            let root = tempfile::tempdir().unwrap();
            let mut app = crate::app::app_state::test_support::test_app(root.path());
            let mut buffer = history_buffer();
            if redo {
                buffer.document_mut().undo_last_operation().unwrap();
            }
            let action = super::TextHistoryAction {
                buffer_id: buffer.id,
                entry_id: buffer.document().history_entries()[0].id,
            };
            app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
            let mut initialize = true;
            let mut harness = Harness::builder()
                .with_size(egui::vec2(900.0, 700.0))
                .build_ui_state(
                    move |ui, app| {
                        if initialize {
                            super::preview_edit(ui.ctx(), app, action);
                            initialize = false;
                        }
                        show(ui.ctx(), app);
                    },
                    app,
                );
            harness.run_steps(2);
            let app = harness.state_mut();
            match case {
                "anchor-only" => {
                    let buffer = app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
                    let snapshot = buffer.document_snapshot();
                    let tree = buffer.document_mut().piece_tree_mut();
                    tree.create_anchor(0, crate::app::domain::AnchorBias::Left);
                    assert_eq!(tree.generation(), snapshot.revision());
                }
                "edited" => app.tab_manager.tabs.as_mut_slice()[0]
                    .active_buffer_mut()
                    .document_mut()
                    .insert_direct(0, "edit"),
                "replaced" => {
                    let buffer = app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
                    // Same history sequence and entry ID in a different document.
                    *buffer.document_mut() = history_buffer().document().clone();
                    if redo {
                        buffer.document_mut().undo_last_operation().unwrap();
                    }
                }
                "closed" => app.tab_manager.set_tabs(Vec::new(), 0),
                _ => {}
            }
            let before = app.tab_manager.tabs.as_slice()[0].active_buffer().text();
            harness
                .get_by_role_and_label(egui::Role::Button, "Apply undo/redo")
                .click_accesskit();
            harness.run_steps(2);
            let text = harness.state().tab_manager.tabs.as_slice()[0]
                .active_buffer()
                .text();
            assert_eq!(
                text,
                if matches!(case, "valid" | "anchor-only") {
                    if redo { "current" } else { "old" }
                } else {
                    &before
                },
                "{redo}/{case}"
            );
            assert_eq!(
                harness
                    .ctx
                    .data(|data| data.get_temp::<super::Preview>(super::key()).is_some()),
                !matches!(case, "valid" | "anchor-only")
            );
        }
    }
}

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
