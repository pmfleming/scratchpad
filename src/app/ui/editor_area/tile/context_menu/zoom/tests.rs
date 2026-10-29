use crate::app::{
    app_state::test_support::test_app,
    domain::{BufferState, SplitAxis, WorkspaceTab},
    ui::editor_area::tile::TileRenderRequest,
};
use eframe::egui;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

#[test]
fn zoom_menu_targets_clicked_pane_and_stays_open_for_repeated_adjustments() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let mut tab = WorkspaceTab::new(BufferState::new("note".into(), "text".into(), None));
    let first = tab.layout.active_view_id();
    let second = tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.activate_view(first);
    app.tab_manager.set_tabs(vec![tab], 0);
    let baseline = app.state.app_settings.font_size();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 900.0))
        .with_context_setup(|ctx| {
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            ctx.set_fonts(fonts);
        })
        .build_ui_state(
            move |ui, app| {
                let response =
                    ui.add_sized(egui::vec2(300.0, 100.0), egui::Button::new("Second pane"));
                let request = TileRenderRequest {
                    tab_index: 0,
                    view_id: second,
                    pane_path: Vec::new(),
                    rect: response.rect,
                    is_active: app
                        .tab_manager
                        .active_tab()
                        .unwrap()
                        .layout
                        .active_view_id()
                        == second,
                    multiple_tiles_visible: true,
                };
                super::super::attach_editor_context_menu(
                    &response,
                    ui,
                    app,
                    &request,
                    &mut Vec::new(),
                );
            },
            app,
        );
    harness.run();
    harness.get_by_label("Second pane").click_secondary();
    harness.run();
    harness.get_by_label("Zoom · 100%");
    for expected in ["Zoom · 110%", "Zoom · 120%"] {
        harness
            .get_by_role_and_label(egui::Role::Button, "Zoom in")
            .click();
        harness.run();
        harness.get_by_label(expected);
    }
    let out = harness
        .get_by_role_and_label(egui::Role::Button, "Zoom out")
        .rect();
    let plus = harness
        .get_by_role_and_label(egui::Role::Button, "Zoom in")
        .rect();
    let reset = harness
        .get_by_role_and_label(egui::Role::Button, "Reset zoom")
        .rect();
    assert_eq!(out.center().y, plus.center().y);
    assert_eq!(plus.center().y, reset.center().y);
    assert!(out.right() <= plus.left() && plus.right() <= reset.left());
    harness
        .get_by_role_and_label(egui::Role::Button, "Zoom out")
        .click();
    harness.run();
    harness.get_by_label("Zoom · 110%");
    harness
        .get_by_role_and_label(egui::Role::Button, "Reset zoom")
        .click();
    harness.run();
    harness.get_by_label("Zoom · 100%");
    assert!(
        harness
            .get_by_role_and_label(egui::Role::Button, "Reset zoom")
            .accesskit_node()
            .is_disabled()
    );
    let tab = harness.state().tab_manager.active_tab().unwrap();
    assert_eq!(tab.layout.active_view_id(), second);
    assert_eq!(tab.layout.view(first).unwrap().zoom_factor(), 1.0);
    assert_eq!(tab.layout.view(second).unwrap().zoom_factor(), 1.0);
    assert_eq!(harness.state().state.app_settings.font_size(), baseline);
}
