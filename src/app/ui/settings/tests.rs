use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use widgets::{
    reset_settings_layout_measurements, settings_card_measurements, settings_control_measurements,
};

fn app() -> ScratchpadApp {
    let root = tempfile::tempdir().unwrap().keep();
    crate::app::app_state::test_support::test_app(&root)
}
fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts.families.insert(
        egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()),
        fonts.families[&egui::FontFamily::Monospace].clone(),
    );
    ctx.set_fonts(fonts);
}

#[test]
fn settings_have_five_independent_categories_without_nested_disclosures() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 6000.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                render_page_body(ui, app, 1100.0);
            },
            app(),
        );
    harness.run();
    assert_eq!(
        harness
            .get_all_by_role(egui::Role::DisclosureTriangle)
            .count(),
        5
    );
    for label in [
        "Appearance",
        "Editors",
        "Workspace",
        "History",
        "Application",
    ] {
        harness.get_by_role_and_label(egui::Role::DisclosureTriangle, label);
    }
    assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 7);
    for label in [
        "Memory per file",
        "Memory across all files",
        "Saved undo per file",
    ] {
        harness.get_by_role_and_label(egui::Role::Slider, label);
    }
    assert!(harness.query_by_label("Reset section").is_none());
    assert!(
        harness
            .query_by_label("Maximum detailed entries per file")
            .is_none()
    );
    harness
        .get_by_role_and_label(egui::Role::DisclosureTriangle, "Appearance")
        .click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_label("Text preview").is_none());
    harness.get_by_label("Tab preview");
    harness
        .get_by_role_and_label(egui::Role::DisclosureTriangle, "History")
        .click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Slider, "Memory per file")
            .is_none()
    );
}

#[test]
fn search_reveals_matching_groups_without_losing_collapse_state() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 6000.0))
        .with_context_setup(setup)
        .build_ui_state(|ui, app| render_page_body(ui, app, 1100.0), app());
    harness.run();
    harness.get_by_label("Collapse all").click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_role(egui::Role::Slider).is_none());
    let ctx = harness.ctx.clone();
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique("settings.search"), "persisted".to_owned())
    });
    harness.run_steps(2);
    assert_eq!(
        harness
            .get_all_by_role(egui::Role::DisclosureTriangle)
            .count(),
        1
    );
    harness.get_by_role_and_label(egui::Role::Slider, "Saved undo per file");
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::unique("settings.search"),
            "nothing-matches".to_owned(),
        )
    });
    harness.run_steps(2);
    harness.get_by_label("No matching settings. Try tabs, font or history.");
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("settings.search"), String::new()));
    harness.run_steps(2);
    assert_eq!(
        harness
            .get_all_by_role(egui::Role::DisclosureTriangle)
            .count(),
        5
    );
    assert!(harness.query_by_role(egui::Role::Slider).is_none());
    harness.get_by_label("Expand all").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 7);
}

#[test]
fn font_dropdowns_share_left_edge_and_all_sliders_share_columns() {
    for width in [1200.0, 900.0, 500.0, 360.0] {
        reset_settings_layout_measurements();
        let ctx = egui::Context::default();
        setup(&ctx);
        let mut app = app();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(width);
            ui.set_min_height(9000.0);
            render_page_body(ui, &mut app, width);
        });
        output.textures_delta.clear();
        let measurements = settings_control_measurements();
        let find = |name: &str| {
            measurements
                .iter()
                .find(|m| m.label == name)
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        let left = |m: &widgets::SettingsControlMeasurement| m.center_x - m.width / 2.0;
        assert!((left(find("combo.Font")) - left(find("combo.Size"))).abs() < 1.0);
        let reference = find("slider.Editor padding");
        if width >= 900.0 {
            assert!((find("label.Editor padding").center_y - reference.center_y).abs() < 1.0);
            assert!((left(find("label.Editor padding")) - left(find("label.Font"))).abs() < 1.0);
        }
        for label in [
            "Indent width",
            "Closed-tab budget",
            "Recovery-point budget",
            "Memory per file",
            "Memory across all files",
            "Saved undo per file",
        ] {
            let slider = find(&format!("slider.{label}"));
            assert!(
                (left(reference) - left(slider)).abs() < 1.0,
                "{label} at {width}"
            );
            assert!(
                (reference.right_x - slider.right_x).abs() < 1.0,
                "{label} at {width}"
            );
            let number = find(&format!("number.{label}"));
            assert!((number.center_y - slider.center_y).abs() < 1.0);
        }
        let cards = settings_card_measurements();
        assert_eq!(cards.len(), 5, "no nested cards");
        for card in &cards {
            assert!(
                card.right_x <= width + 1.0,
                "card overflow at {width}: {card:?}"
            );
            assert!(
                (card.center_x - width / 2.0).abs() < 1.0,
                "card centering at {width}: {card:?}"
            );
        }
    }
}

#[test]
fn appearance_controls_stay_visible_for_system_source_and_editing_switches_source() {
    let mut app = app();
    crate::app::app_state::settings_controller::set_editor_appearance_source(
        &mut app,
        EditorAppearanceSource::System,
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                text_formatting::render_appearance(ui, app);
            },
            app,
        );
    harness.run();
    for label in ["Font", "Size", "Text", "Background", "Search highlight"] {
        harness.get_by_role_and_label(egui::Role::Label, label);
    }
    harness
        .get_by_role_and_label(egui::Role::ComboBox, "Font")
        .click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Mono").click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .editor_appearance_source(),
        EditorAppearanceSource::App
    );
    assert_eq!(
        harness.state().state.app_settings.editor_font(),
        EditorFontPreset::Mono
    );
}

#[test]
fn global_reset_requires_confirmation_and_cancel_does_not_change_settings() {
    let mut app = app();
    app.state.app_settings.editor.editor_gutter = 23;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                opening::render_application(ui, app);
                opening::reset_confirmation(ui.ctx(), app);
            },
            app,
        );
    harness.run();
    harness.get_by_label("Reset settings…").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.editor_gutter(), 23);
    harness.get_by_label("Cancel").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.editor_gutter(), 23);
    harness.get_by_label("Reset settings…").click_accesskit();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::Role::Button, "Reset all settings")
        .click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.app_settings.editor_gutter(),
        crate::app::services::settings_store::AppSettings::default().editor_gutter()
    );
}

#[test]
fn rendering_byte_budgets_does_not_round_or_clamp_stored_values() {
    let ctx = egui::Context::default();
    let mut bytes = 64 * 1024 * 1024 + 123;
    let original = bytes;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_width(880.0);
        sections::byte_budget_row(ui, "Test budget", &mut bytes, 1, 1024);
    });
    output.textures_delta.clear();
    assert_eq!(bytes, original);
}

#[test]
fn auto_hide_reveals_delay_and_keeps_the_saved_value_when_disabled() {
    let mut app = app();
    app.state.app_settings.workspace.auto_hide_tab_list = false;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                appearance::render_workspace(ui, app);
            },
            app,
        );
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(egui::Role::ComboBox, "Hide after")
            .is_none()
    );
    harness
        .get_by_role_and_label(egui::Role::Switch, "Auto-hide tabs")
        .click_accesskit();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::Role::ComboBox, "Hide after")
        .click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("3").click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .tab_list_auto_hide_delay_seconds(),
        3.0
    );
    harness
        .get_by_role_and_label(egui::Role::Switch, "Auto-hide tabs")
        .click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_role_and_label(egui::Role::ComboBox, "Hide after")
            .is_none()
    );
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .tab_list_auto_hide_delay_seconds(),
        3.0
    );
}

#[test]
fn exact_value_inputs_and_recommended_limits_keep_slider_values_in_sync() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                sections::text_history_budget(ui, app);
            },
            app(),
        );
    harness.run();
    for (label, value) in [
        ("Memory per file", 83.0),
        ("Memory across all files", 1024.0),
        ("Saved undo per file", 0.0),
    ] {
        harness
            .get_by_role_and_label(egui::Role::SpinButton, label)
            .set_value_accesskit(value);
        harness.run_steps(2);
        use egui_kittest::kittest::NodeT;
        assert_eq!(
            harness
                .get_by_role_and_label(egui::Role::Slider, label)
                .accesskit_node()
                .numeric_value(),
            Some(value)
        );
    }
    let budget = harness.state().state.app_settings.history.budget;
    assert_eq!(budget.per_file_byte_budget, 83 * 1024 * 1024);
    assert_eq!(budget.aggregate_byte_budget, 1024 * 1024 * 1024);
    assert_eq!(budget.persisted_payload_budget, 0);
    harness.get_by_label("Use recommended").click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .state
            .app_settings
            .history
            .budget
            .derived_from_memory
    );
    let saved = harness
        .state()
        .state
        .persistence
        .settings_store
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(
        saved.history.budget,
        harness.state().state.app_settings.history.budget
    );
}
