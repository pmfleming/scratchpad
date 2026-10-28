use super::render_policy;
use crate::app::{
    app_state::test_support::test_app,
    ui::settings::{SettingsUi, widgets::settings_card_frame},
};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};

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
                    settings_card_frame(ui, |ui| render_policy(ui, app));
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
        for name in ["Closed archives", "Archive retention"] {
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

        for (name, expected) in [("Recovery points", 512), ("Closed archives", 1024)] {
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
    }
}
