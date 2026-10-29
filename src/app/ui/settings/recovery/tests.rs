use super::*;
use crate::app::{app_state::test_support::test_app, ui::settings::SettingsUi};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

#[test]
fn recovery_budgets_and_retention_presets_persist_without_render_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let app = test_app(root.path());
    let original = app.state.app_settings.history.recovery.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .build_ui_state(
            |ui, app| {
                SettingsUi::apply_typography(ui);
                ui.set_width(880.0);
                render_policy(ui, app);
            },
            app,
        );
    harness.run();
    assert_eq!(
        harness.state().state.app_settings.history.recovery,
        original
    );
    assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 2);
    for (name, value) in [
        ("Closed-tab budget", 1024.0),
        ("Recovery-point budget", 512.0),
    ] {
        harness
            .get_by_role_and_label(egui::Role::Slider, name)
            .set_value_accesskit(value);
        harness.run_steps(2);
    }
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .history
            .recovery
            .archive_byte_budget,
        1024 * 1024 * 1024
    );
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .history
            .recovery
            .snapshot_byte_budget,
        512 * 1024 * 1024
    );
    for label in [
        "Restore history when reopening files",
        "Also open changed disk files",
    ] {
        harness
            .get_by_role_and_label(egui::Role::Switch, label)
            .click_accesskit();
        harness.run_steps(2);
    }
    let saved = harness
        .state()
        .state
        .persistence
        .settings_store
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(
        saved.history.recovery,
        harness.state().state.app_settings.history.recovery
    );
    assert!(!saved.history.recovery.reopen_archives);
    assert!(!saved.history.recovery.open_changed_disk_files);
}

#[test]
fn custom_retention_values_are_preserved_and_no_expiry_is_explicit() {
    for days in [0, 7, 30, 90, 61, 36500] {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_app(root.path());
        app.state
            .app_settings
            .history
            .recovery
            .archive_retention_days = days;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 1400.0))
            .build_ui_state(
                |ui, app| {
                    ui.set_width(880.0);
                    render_policy(ui, app);
                },
                app,
            );
        harness.run();
        assert_eq!(
            harness
                .state()
                .state
                .app_settings
                .history
                .recovery
                .archive_retention_days,
            days
        );
        if days == 0 {
            assert_eq!(
                harness
                    .get_by_role_and_label(egui::Role::ComboBox, "Keep closed tabs for")
                    .accesskit_node()
                    .value(),
                Some("No expiry".to_owned())
            );
        }
        assert_eq!(
            harness
                .query_by_role_and_label(egui::Role::Label, "Retention period")
                .is_some(),
            ![0, 7, 30, 90].contains(&days)
        );
    }
}

#[test]
fn retention_presets_and_custom_days_are_editable_and_persisted() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    app.state
        .app_settings
        .history
        .recovery
        .archive_retention_days = 30;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                render_policy(ui, app);
            },
            app,
        );
    harness.run();
    for (label, expected) in [("90 days", 90), ("No expiry", 0), ("Custom…", 30)] {
        harness
            .get_by_role_and_label(egui::Role::ComboBox, "Keep closed tabs for")
            .click_accesskit();
        harness.run_steps(2);
        harness.get_by_label(label).click_accesskit();
        harness.run_steps(2);
        assert_eq!(
            harness
                .state()
                .state
                .app_settings
                .history
                .recovery
                .archive_retention_days,
            expected
        );
    }
    harness
        .get_by_role_and_label(egui::Role::SpinButton, "Retention period")
        .set_value_accesskit(61.0);
    harness.run_steps(2);
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .history
            .recovery
            .archive_retention_days,
        61
    );
    let saved = harness
        .state()
        .state
        .persistence
        .settings_store
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(saved.history.recovery.archive_retention_days, 61);
}

#[test]
fn usage_errors_leave_policy_and_undo_limits_editable() {
    for state in ["loading", "disk-error", "database-error"] {
        let root = tempfile::tempdir().unwrap();
        let app = test_app(root.path());
        let usage = StorageUsage {
            stored_error: Some("database unavailable".into()),
            ..Default::default()
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 3000.0))
            .with_context_setup(|ctx| {
                let mut fonts = egui::FontDefinitions::default();
                egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
                ctx.set_fonts(fonts);
            })
            .build_ui_state(
                move |ui, app| {
                    ui.set_width(880.0);
                    render_content(
                        ui,
                        app,
                        match state {
                            "loading" => None,
                            "disk-error" => Some(Err("directory unavailable")),
                            _ => Some(Ok(&usage)),
                        },
                    );
                },
                app,
            );
        harness.run();
        harness.get_by_label(match state {
            "loading" => "Measuring disk usage…",
            "disk-error" => "Disk usage unavailable",
            _ => "Recovery details unavailable",
        });
        assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 5);
        assert!(
            harness
                .query_by_role(egui::Role::DisclosureTriangle)
                .is_none()
        );
        harness
            .get_by_role_and_label(egui::Role::Slider, "Memory per file")
            .set_value_accesskit(72.0);
        harness.run_steps(2);
        assert_eq!(
            harness
                .state()
                .state
                .app_settings
                .history
                .budget
                .per_file_byte_budget,
            72 * 1024 * 1024
        );
        assert!(harness.query_by_label("Copy data directory").is_none());
    }
}
