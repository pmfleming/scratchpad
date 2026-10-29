use crate::app::{
    app_state::test_support::test_app,
    domain::{BufferState, SplitAxis, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use eframe::{App, egui};
use egui_kittest::Harness;

#[test]
fn editor_renders_each_views_font_size_and_gestures_only_zoom_the_hovered_pane() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let mut tab = WorkspaceTab::new(BufferState::new(
        "note".into(),
        "one\ntwo\nthree".into(),
        None,
    ));
    let first = tab.layout.active_view_id();
    let second = tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.layout.view_mut(second).unwrap().set_zoom_factor(2.0);
    tab.activate_view(first);
    app.tab_manager.set_tabs(vec![tab], 0);
    let baseline = app.state.app_settings.font_size();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
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
        );
    harness.run_steps(3);
    let tab = harness.state().tab_manager.active_tab().unwrap();
    let first_height = tab.layout.view(first).unwrap().scroll.metrics().row_height;
    let second_view = tab.layout.view(second).unwrap();
    assert!(first_height > 0.0);
    assert!(second_view.scroll.metrics().row_height > first_height * 1.8);
    let target = second_view.scroll.metrics().viewport_rect.center();
    harness.event(egui::Event::PointerMoved(target));
    harness.event(egui::Event::Zoom(1.5));
    harness.run_steps(2);
    let tab = harness.state().tab_manager.active_tab().unwrap();
    assert_eq!(tab.layout.view(first).unwrap().zoom_factor(), 1.0);
    assert_eq!(tab.layout.view(second).unwrap().zoom_factor(), 3.0);
    assert_eq!(
        tab.layout.view(first).unwrap().scroll.metrics().row_height,
        first_height
    );
    assert_eq!(harness.state().state.app_settings.font_size(), baseline);
    harness.event(egui::Event::PointerMoved(egui::pos2(1.0, 1.0)));
    harness.event(egui::Event::Zoom(2.0));
    harness.run_steps(2);
    let tab = harness.state().tab_manager.active_tab().unwrap();
    assert_eq!(tab.layout.view(first).unwrap().zoom_factor(), 1.0);
    assert_eq!(tab.layout.view(second).unwrap().zoom_factor(), 3.0);
}
