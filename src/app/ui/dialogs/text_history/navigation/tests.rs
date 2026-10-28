use super::{TextHistoryAction, apply, step_target};
use crate::app::{
    app_state::{ScratchpadApp, test_support::test_app},
    domain::{BufferState, CursorRange, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use eframe::{App, egui};
use egui_kittest::{Harness, kittest::Queryable};

fn app_with_history(root: &std::path::Path) -> ScratchpadApp {
    let path = root.join("note.txt");
    std::fs::write(&path, "old").unwrap();
    let mut buffer = BufferState::new("note.txt".into(), "old".into(), Some(path));
    for (end, text) in [(3, "middle"), (6, "current")] {
        buffer
            .replace_char_ranges_with_undo(
                &[(0..end, text.into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
    }
    assert_eq!(buffer.document().history_entries().len(), 2);
    let mut app = test_app(root);
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    app
}

fn text(app: &ScratchpadApp) -> String {
    app.tab_manager.tabs.as_slice()[0].active_buffer().text()
}

#[test]
fn live_navigation_moves_both_directions_without_replacement_edits_or_disk_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let ctx = egui::Context::default();
    for (undo, expected) in [
        (true, "middle"),
        (true, "old"),
        (false, "middle"),
        (false, "current"),
    ] {
        let action = step_target(&ctx, &mut app, undo).unwrap();
        assert!(apply(&ctx, &mut app, action));
        assert_eq!(text(&app), expected);
        let buffer = app.tab_manager.tabs.as_slice()[0].active_buffer();
        assert_eq!(buffer.document().history_entries().len(), 2);
        assert_eq!(
            std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
            "old"
        );
        if expected == "old" || expected == "current" {
            assert!(step_target(&ctx, &mut app, undo).is_none());
        }
    }
}

#[test]
fn selected_buffer_is_navigated_without_changing_other_files_or_stealing_focus() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let first = app.tab_manager.tabs.as_slice()[0].clone();
    let mut other = BufferState::new("other".into(), "other".into(), None);
    other
        .replace_char_ranges_with_undo(
            &[(0..5, "unrelated".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    app.tab_manager
        .set_tabs(vec![first, WorkspaceTab::new(other)], 1);
    let ctx = egui::Context::default();
    super::super::write_follow_focus(&ctx, false);
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .textures_delta
        .clear();
    let buffer = app.tab_manager.tabs.as_slice()[0].active_buffer();
    let action = TextHistoryAction {
        buffer_id: buffer.id,
        entry_id: buffer.document().history_entries()[0].id,
    };
    assert!(apply(&ctx, &mut app, action));
    assert_eq!(text(&app), "old");
    let redo = step_target(&ctx, &mut app, false).unwrap();
    assert_eq!(redo.buffer_id, action.buffer_id);
    assert!(apply(&ctx, &mut app, redo));
    assert_eq!(text(&app), "middle");
    assert_eq!(app.tab_manager.active_tab_index, 1);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1].active_buffer().text(),
        "unrelated"
    );
    assert!(!apply(
        &ctx,
        &mut app,
        TextHistoryAction {
            entry_id: u64::MAX,
            ..action
        }
    ));
    assert_eq!(text(&app), "middle");
}

#[test]
fn history_rows_buttons_and_arrow_keys_change_the_underlying_editor_without_a_preview() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.dialogs.text_history.open();
    let entries = app.tab_manager.tabs.as_slice()[0]
        .active_buffer()
        .document()
        .history_entries();
    let undo_all = format!("Undo: {}", entries[0].summary);
    let redo_all = format!("Redo: {}", entries[1].summary);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 850.0))
        .with_context_setup(|ctx| {
            apply_editor_fonts(
                ctx,
                &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
                4.0,
            )
            .unwrap();
            super::super::write_active_tab(ctx, super::super::HistoryTab::Timeline);
        })
        .build_ui_state(
            |ui, app| App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            app,
        );
    harness.run_steps(3);
    harness
        .get_all_by_role_and_label(egui::Role::Button, &undo_all)
        .last()
        .unwrap()
        .click_accesskit();
    harness.run_steps(3);
    assert_eq!(text(harness.state()), "old");
    harness
        .get_all_by_role_and_label(egui::Role::Button, &redo_all)
        .next()
        .unwrap()
        .click_accesskit();
    harness.run_steps(3);
    assert_eq!(text(harness.state()), "current");
    harness
        .get_by_role_and_label(egui::Role::Button, "↑ Undo")
        .click();
    harness.run_steps(3);
    assert_eq!(text(harness.state()), "middle");
    harness.key_press(egui::Key::ArrowUp);
    harness.run_steps(3);
    assert_eq!(text(harness.state()), "old");
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(3);
    assert_eq!(text(harness.state()), "middle");
    harness
        .get_by_role_and_label(egui::Role::Button, "↓ Redo")
        .click();
    harness.run_steps(3);
    assert_eq!(text(harness.state()), "current");
    assert!(harness.query_by_label("History preview").is_none());
    assert!(harness.query_by_label("Apply undo/redo").is_none());
    assert!(harness.state().state.dialogs.text_history.is_open());
    assert!(!harness.state().state.dialogs.any_modal_open());
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "old"
    );
}
