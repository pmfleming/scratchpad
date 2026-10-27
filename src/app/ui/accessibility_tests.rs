use crate::app::app_state::{ScratchpadApp, settings_controller};
use crate::app::fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts};
use crate::app::services::{session_store::SessionStore, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use eframe::{App, egui};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn setup(ctx: &egui::Context) {
    crate::app::diagnostics::take_test_egui_warnings();
    apply_editor_fonts(
        ctx,
        &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
        4.0,
    )
    .unwrap();
    ctx.options_mut(|options| options.warn_on_id_clash = true);
    #[cfg(debug_assertions)]
    ctx.global_style_mut(|style| style.debug.warn_if_rect_changes_id = true);
}

#[test]
fn settings_inputs_have_names_and_custom_switches_are_actionable() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(directory.path().join("session")),
        SettingsStore::new(directory.path().join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    settings_controller::open_settings(&mut app);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 900.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
            app,
        );
    harness.run_steps(3);
    harness.get_by_role_and_label(egui::Role::Tab, "Settings");
    let switch = harness.get_all_by_role(egui::Role::Switch).next().unwrap();
    let id = switch.accesskit_node().id();
    let toggled = switch.accesskit_node().toggled();
    switch.click_accesskit();
    harness.run_steps(3);
    let switch = harness
        .get_all_by_role(egui::Role::Switch)
        .find(|node| node.accesskit_node().id() == id)
        .unwrap();
    assert_ne!(switch.accesskit_node().toggled(), toggled);
    // Kittest validates accessible names for all visible inputs on every step.
    let warnings = crate::app::diagnostics::take_test_egui_warnings();
    assert!(warnings.is_empty(), "{warnings:#?}");
}

#[test]
fn named_dialog_disappears_from_accessibility_tree_when_closed() {
    let mut harness = Harness::builder().with_context_setup(setup).build_ui_state(
        |ui, open| {
            if *open {
                super::callout::show_floating(
                    ui.ctx(),
                    "accessible-dialog",
                    "Test dialog",
                    egui::pos2(10.0, 10.0),
                    300.0,
                    |ui| {
                        let response = crate::app::chrome::phosphor_button(
                            ui,
                            "test-close",
                            egui_phosphor::regular::X,
                            egui::vec2(28.0, 28.0),
                            egui::Color32::BLACK,
                            egui::Color32::GRAY,
                            "Close dialog",
                        );
                        if response.clicked() {
                            *open = false;
                        }
                    },
                );
            }
        },
        true,
    );
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::Role::Dialog, "Test dialog");
    harness
        .get_by_role_and_label(egui::Role::Button, "Close dialog")
        .click_accesskit();
    harness.run_steps(3);
    assert!(!harness.state());
    assert!(harness.query_by_role(egui::Role::Dialog).is_none());
    let warnings = crate::app::diagnostics::take_test_egui_warnings();
    assert!(warnings.is_empty(), "{warnings:#?}");
}

#[test]
fn tab_identity_survives_reordering_and_close_is_a_named_action() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 150.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, reversed| {
                ui.horizontal(|ui| {
                    let labels = if *reversed {
                        ["second", "first"]
                    } else {
                        ["first", "second"]
                    };
                    for label in labels {
                        let (_, close, _) = crate::app::chrome::tab_button_sized(
                            ui,
                            ("accessible-tab", label),
                            label,
                            label == "first",
                            false,
                            180.0,
                        );
                        if close.clicked() {
                            *reversed = !*reversed;
                        }
                    }
                });
            },
            false,
        );
    let before = harness
        .get_by_role_and_label(egui::Role::Tab, "first")
        .accesskit_node()
        .id();
    harness
        .get_by_role_and_label(egui::Role::Button, "Close first")
        .click();
    harness.run_steps(3);
    assert!(*harness.state());
    let after = harness
        .get_by_role_and_label(egui::Role::Tab, "first")
        .accesskit_node()
        .id();
    assert_eq!(before, after);
    let warnings = crate::app::diagnostics::take_test_egui_warnings();
    assert!(warnings.is_empty(), "{warnings:#?}");
}
