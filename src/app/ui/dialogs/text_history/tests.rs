use super::*;
use crate::app::{
    app_state::test_support::test_app,
    domain::{BufferState, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use eframe::App;
use egui_kittest::{Harness, kittest::Queryable};

fn app(root: &std::path::Path) -> ScratchpadApp {
    let mut app = test_app(root);
    let mut buffer = BufferState::new("note.txt".into(), "base".into(), None);
    for text in ["one", "two"] {
        let len = buffer.document().piece_tree().len_chars();
        buffer
            .replace_char_ranges_with_undo(
                &[(0..len, text.into())],
                Default::default(),
                Default::default(),
            )
            .unwrap();
    }
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    app.state.dialogs.text_history.open();
    app
}
fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    Harness::builder()
        .allow_missing_glyphs()
        .with_size(egui::vec2(1200.0, 850.0))
        .with_context_setup(|ctx| {
            apply_editor_fonts(
                ctx,
                &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
                4.0,
            )
            .unwrap();
        })
        .build_ui_state(
            |ui, app| App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            app,
        )
}
fn text(app: &ScratchpadApp) -> String {
    app.tab_manager.active_tab().unwrap().active_buffer().text()
}

#[test]
fn now_is_visible_hover_is_inert_and_click_selects_a_state() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(app(root.path()));
    h.run_steps(3);
    assert!(h.query_by_label("Now · Replace match").is_some());
    let root_row = h.get_by_label("Empty document");
    h.hover_at(root_row.rect().center());
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    h.get_by_label("Empty document").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
    assert!(h.query_by_label("Now · Empty document").is_some());
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(text(h.state()), "base");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.dialogs.text_history.is_open());
    assert_eq!(text(h.state()), "base");
}

#[test]
fn branches_remain_selectable_and_recent_order_ignores_navigation() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    let id = buffer.id;
    let before = buffer.document().last_edit();
    buffer.navigate_history(1).unwrap();
    assert_eq!(before, buffer.document().last_edit());
    buffer
        .replace_char_ranges_with_undo(
            &[(0..4, "alternate".into())],
            Default::default(),
            Default::default(),
        )
        .unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label("Branch 2").is_some());
    assert!(h.query_by_label("Branch 4").is_some());
    let ctx = h.ctx.clone();
    assert!(navigate(&ctx, h.state_mut(), id, 3));
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    assert!(navigate(&ctx, h.state_mut(), id, 4));
    h.run_steps(3);
    assert_eq!(text(h.state()), "alternate");
}

#[test]
fn history_typing_delivers_first_event_to_editor() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(app(root.path()));
    h.run_steps(3);
    h.event(egui::Event::Text("X".into()));
    h.run_steps(3);
    assert!(text(h.state()).contains('X'));
}

#[test]
fn scrubber_shows_only_the_selected_route_and_opens_without_moving_now() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    app.state.dialogs.text_history.close();
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer.navigate_history(1).unwrap();
    buffer
        .replace_char_ranges_with_undo(
            &[(0..4, "branch".into())],
            Default::default(),
            Default::default(),
        )
        .unwrap();
    buffer.undo_last_text_operation().unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(text(h.state()), "base");
    assert!(h.query_by_label("Now · 1 of 2 changes").is_some());
    assert!(h.state().state.dialogs.history_scrubber.is_open());
    let before = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .document()
        .history_now();
    h.run_steps(3);
    assert_eq!(
        before,
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_now()
    );
}
