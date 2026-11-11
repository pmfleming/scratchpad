use super::*;
use crate::app::{
    app_state::test_support::test_app,
    domain::{BufferState, SplitAxis, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use eframe::App;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(root: &std::path::Path) -> Harness<'static, ScratchpadApp> {
    let mut app = test_app(root);
    app.tab_manager.set_tabs(
        vec![WorkspaceTab::new(BufferState::new(
            "note.txt".into(),
            "text".into(),
            None,
        ))],
        0,
    );
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
            ctx.options_mut(|options| options.zoom_with_keyboard = false);
        })
        .build_ui_state(
            |ui, app| App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            app,
        )
}

fn factor(h: &Harness<'_, ScratchpadApp>) -> f32 {
    h.state()
        .tab_manager
        .active_tab()
        .unwrap()
        .layout
        .active_view()
        .unwrap()
        .zoom_factor()
}

fn shortcut(h: &mut Harness<'_, ScratchpadApp>, key: egui::Key) {
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    });
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    });
    h.run_steps(3);
}

#[test]
fn status_click_opens_without_changing_zoom_and_shortcuts_open_even_at_limits() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(root.path());
    h.run_steps(3);
    h.get_by_label("100%").click();
    h.run_steps(3);
    assert_eq!(factor(&h), 1.0);
    assert!(h.query_by_label("Zoom percentage").is_some());
    assert!(!h.ctx.memory(|m| m.has_focus(focus_id())));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().state.dialogs.zoom_scrubber.is_none());
    shortcut(&mut h, egui::Key::Equals);
    assert!((factor(&h) - 1.1).abs() < 0.001);
    assert!(h.query_by_label("Zoom percentage").is_some());
    shortcut(&mut h, egui::Key::Minus);
    assert!((factor(&h) - 1.0).abs() < 0.001);
    h.get_by_label("Maximum zoom (End)").click();
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    shortcut(&mut h, egui::Key::Equals);
    assert_eq!(factor(&h), MAX_ZOOM_FACTOR);
    assert!(h.query_by_label("Zoom percentage").is_some());
}

#[test]
fn live_drag_keyboard_and_reset_preserve_text_and_global_settings() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(root.path());
    let settings = h.state().state.app_settings.clone();
    open(h.state_mut());
    h.run_steps(3);
    let track = h.get_by_label("Zoom percentage").rect();
    let start = egui::pos2(track.left() + 7.0, track.center().y);
    let end = egui::pos2(track.right() - 7.0, track.center().y);
    h.hover_at(end);
    h.run_steps(3);
    assert_eq!(factor(&h), 1.0);
    h.drag_at(start);
    h.run_steps(3);
    assert_eq!(factor(&h), MIN_ZOOM_FACTOR);
    h.hover_at(end);
    h.run_steps(3);
    assert_eq!(factor(&h), MAX_ZOOM_FACTOR);
    assert!(h.ctx.input(|i| i.pointer.any_down()));
    h.drop_at(end);
    h.run_steps(3);
    h.key_press(egui::Key::Home);
    h.run_steps(3);
    assert_eq!(factor(&h), MIN_ZOOM_FACTOR);
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert!((factor(&h) - 0.35).abs() < 0.001);
    h.get_by_label("Reset zoom (100%)").click();
    h.run_steps(3);
    assert_eq!(factor(&h), 1.0);
    assert_eq!(h.state().state.app_settings, settings);
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .text(),
        "text"
    );
    h.event(egui::Event::Text("X".into()));
    h.run_steps(3);
    assert!(h.state().state.dialogs.zoom_scrubber.is_none());
    let text = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .text();
    assert_eq!(text.chars().filter(|&c| c == 'X').count(), 1);
}

#[test]
fn pane_switch_cancels_drag_without_zooming_the_new_pane() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(root.path());
    let tab = h.state_mut().tab_manager.active_tab_mut().unwrap();
    let first = tab.layout.active_view_id();
    let second = tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.activate_view(first);
    open(h.state_mut());
    h.run_steps(3);
    let track = h.get_by_label("Zoom percentage").rect();
    let start = egui::pos2(track.left() + 7.0, track.center().y);
    let end = egui::pos2(track.right() - 7.0, track.center().y);
    h.drag_at(start);
    h.run_steps(3);
    h.state_mut()
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .activate_view(second);
    h.hover_at(end);
    h.run_steps(3);
    assert!(h.state().state.dialogs.zoom_scrubber.is_none());
    assert_eq!(factor(&h), 1.0);
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .layout
            .view(first)
            .unwrap()
            .zoom_factor(),
        MIN_ZOOM_FACTOR
    );
    h.drop_at(end);
    h.run_steps(3);
}

#[test]
fn accessibility_changes_are_live_and_bounded() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(root.path());
    open(h.state_mut());
    h.run_steps(3);
    h.get_by_label("Zoom percentage").set_value_accesskit(150.0);
    h.run_steps(3);
    assert_eq!(factor(&h), 1.5);
    h.get_by_label("Zoom percentage").increment_accesskit();
    h.run_steps(3);
    assert!((factor(&h) - 1.6).abs() < 0.001);
    h.get_by_label("Zoom percentage").decrement_accesskit();
    h.run_steps(3);
    assert_eq!(factor(&h), 1.5);
    h.get_by_label("Zoom percentage").set_value_accesskit(999.0);
    h.run_steps(3);
    assert_eq!(factor(&h), MAX_ZOOM_FACTOR);
}

#[test]
fn status_click_also_opens_zoom_while_history_is_visible() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(root.path());
    h.state_mut().state.dialogs.text_history.open();
    h.run_steps(3);
    assert!(h.state().state.dialogs.text_history.is_open());
    h.get_by_label("100%").click();
    h.run_steps(3);
    assert!(h.query_by_label("Zoom percentage").is_some());
    assert_eq!(factor(&h), 1.0);
}
