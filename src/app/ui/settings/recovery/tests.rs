use super::{render_content, render_policy, render_recovery_category};
use crate::app::services::session_store::StorageUsage;
use crate::app::{
    app_state::test_support::test_app,
    ui::settings::{SettingsUi, widgets::settings_card_frame},
};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};

fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

#[test]
fn recovery_policy_uses_standard_aligned_controls_without_long_descriptions() {
    for dark in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let app = test_app(root.path());
        let original = app.state.app_settings.history.recovery.clone();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .with_context_setup(move |ctx| {
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
            })
            .build_ui_state(
                |ui, app| {
                    SettingsUi::apply_typography(ui);
                    ui.set_width(720.0);
                    let mut usage = StorageUsage::default();
                    usage.stored.archives = 5;
                    usage.stored.recovery_points = 16;
                    settings_card_frame(ui, |ui| render_policy(ui, app, Some(&usage)));
                },
                app,
            );
        harness.run();
        assert_eq!(
            harness.state().state.app_settings.history.recovery,
            original
        );
        assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 3);
        assert_eq!(harness.get_all_by_role(egui::Role::Switch).count(), 2);
        assert!(harness.query_by_role(egui::Role::CheckBox).is_none());
        assert!(harness.query_by_role(egui::Role::SpinButton).is_none());
        let snapshot = harness
            .get_by_role_and_label(egui::Role::Slider, "Recovery points")
            .rect();
        for name in ["Closed tabs", "Archive retention"] {
            let rect = harness
                .get_by_role_and_label(egui::Role::Slider, name)
                .rect();
            assert_eq!(rect.left(), snapshot.left());
            assert_eq!(rect.right(), snapshot.right());
        }
        let first_switch = harness
            .get_by_role_and_label(egui::Role::Switch, "Restore archived tabs")
            .rect();
        let last_switch = harness
            .get_by_role_and_label(egui::Role::Switch, "Open changed files")
            .rect();
        assert_eq!(first_switch.right(), last_switch.right());
        assert!(last_switch.bottom() - snapshot.top() < 320.0);
        assert!(harness.query_by_label("Recent edits are undoable. Older states become snapshot-only recovery points. When budgets are full, older unprotected recovery data expires automatically.").is_none());

        harness.get_by_label("5 retained · Logical archive limit.");
        harness.get_by_label("16 retained · Logical snapshot limit.");
        for (name, expected) in [("Recovery points", 512), ("Closed tabs", 1024)] {
            harness
                .get_by_role_and_label(egui::Role::Slider, name)
                .set_value_accesskit(f64::from(expected));
            harness.run_steps(2);
        }
        let policy = &harness.state().state.app_settings.history.recovery;
        assert_eq!(policy.snapshot_byte_budget, 512 * 1024 * 1024);
        assert_eq!(policy.archive_byte_budget, 1024 * 1024 * 1024);
        for (days, label) in [
            (0, "No expiry"),
            (1, "1 day"),
            (90, "90 days"),
            (36500, "36500 days"),
        ] {
            harness
                .get_by_role_and_label(egui::Role::Slider, "Archive retention")
                .set_value_accesskit(f64::from(days));
            harness.run_steps(2);
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
            harness.get_by_label(label);
        }
        for name in ["Restore archived tabs", "Open changed files"] {
            harness
                .get_by_role_and_label(egui::Role::Switch, name)
                .click_accesskit();
            harness.run_steps(2);
        }
        let policy = &harness.state().state.app_settings.history.recovery;
        assert!(!policy.reopen_archives);
        assert!(!policy.open_changed_disk_files);
        let saved = harness
            .state()
            .state
            .persistence
            .settings_store
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(saved.history.recovery, *policy);
    }
}

#[test]
fn recovery_category_combines_usage_and_policy_with_aligned_rows() {
    let root = tempfile::tempdir().unwrap();
    let app = test_app(root.path());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 1600.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                SettingsUi::apply_typography(ui);
                ui.set_width(720.0);
                render_recovery_category(ui, app);
            },
            app,
        );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while harness.query_by_label("Files on disk").is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "usage query did not complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
        harness.run_steps(1);
    }
    for title in ["Undo memory", "Recovery & storage"] {
        harness.get_by_role_and_label(egui::Role::DisclosureTriangle, title);
    }
    for removed in [
        "Automatic recovery",
        "Disk usage",
        "Data files",
        "Closed archives",
    ] {
        assert!(
            harness.query_by_label(removed).is_none(),
            "removed {removed}"
        );
    }
    assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 6);
    let x = harness
        .get_by_role_and_label(egui::Role::Label, "Closed tabs")
        .rect()
        .left();
    for label in ["Files on disk", "Disk breakdown", "Recovery details"] {
        assert!(
            (harness.get_by_label(label).rect().left() - x).abs() < 1.0,
            "misaligned {label}"
        );
    }
    assert!(harness.query_by_label("Session database").is_none());
    assert!(harness.query_by_label("Archive references").is_none());
    harness
        .get_by_role_and_label(egui::Role::DisclosureTriangle, "Recovery & storage")
        .click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_label("Files on disk").is_none());
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Slider, "Closed tabs")
            .is_none()
    );
}

#[test]
fn policy_remains_editable_while_usage_loads_or_fails() {
    for state in ["loading", "disk-error", "database-error"] {
        let root = tempfile::tempdir().unwrap();
        let app = test_app(root.path());
        let usage = StorageUsage {
            total_file_bytes: 4096,
            database: 4096,
            stored_error: Some("database unavailable".into()),
            ..Default::default()
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(800.0, 1000.0))
            .with_context_setup(setup)
            .build_ui_state(
                move |ui, app| {
                    SettingsUi::apply_typography(ui);
                    ui.set_width(720.0);
                    settings_card_frame(ui, |ui| {
                        render_content(
                            ui,
                            app,
                            match state {
                                "loading" => None,
                                "disk-error" => Some(Err("directory unavailable")),
                                _ => Some(Ok(&usage)),
                            },
                        )
                    });
                },
                app,
            );
        harness.run();
        harness.get_by_label(match state {
            "loading" => "Measuring disk usage…",
            "disk-error" => "Disk usage unavailable",
            _ => "Recovery details unavailable",
        });
        harness.get_by_label("— retained · Logical archive limit.");
        harness.get_by_label("— retained · Logical snapshot limit.");
        assert!(harness.query_by_label("Recovery details").is_none());
        assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 3);
        harness
            .get_by_role_and_label(egui::Role::Slider, "Archive retention")
            .set_value_accesskit(30.0);
        harness.run_steps(2);
        assert_eq!(
            harness
                .state()
                .state
                .app_settings
                .history
                .recovery
                .archive_retention_days,
            30
        );
        harness.get_by_label("Copy data directory");
    }
}
