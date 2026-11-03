use super::*;
use crate::app::app_state::test_support::test_app;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn history_settings_explain_retention_without_expiry_controls() {
    let root = tempfile::tempdir().unwrap();
    let app = test_app(root.path());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1000.0))
        .build_ui_state(render_policy, app);
    harness.run();
    harness.get_by_label(
        "All edit branches and closed-file history are retained until you explicitly forget them.",
    );
    assert!(harness.query_by_label("Recovery-point budget").is_none());
    assert!(harness.query_by_label("Keep closed tabs for").is_none());
}
