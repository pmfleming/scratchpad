use super::*;
use crate::app::app_state::test_support::test_app;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

#[test]
fn history_settings_explain_retention_without_expiry_controls() {
    let root = tempfile::tempdir().unwrap();
    let app = test_app(root.path());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1000.0))
        .with_context_setup(crate::app::ui::settings::tests::setup)
        .build_ui_state(
            |ui, app| {
                render_policy(ui, app);
                crate::app::ui::settings::search_options::render(ui, app);
            },
            app,
        );
    harness.run();
    harness.get_by_label(
        "All edit branches and closed-file history are retained until you explicitly forget them.",
    );
    assert!(harness.query_by_label("Recovery-point budget").is_none());
    assert!(harness.query_by_label("Keep closed tabs for").is_none());
}

#[test]
fn inline_resets_affect_only_their_own_limits_and_persist() {
    use crate::app::services::settings_store::SearchResultLimit;
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    app.state.app_settings.history.budget.derived_from_memory = false;
    app.state.app_settings.history.failure_ram.automatic = false;
    app.state.app_settings.search.result_limit = SearchResultLimit::Unlimited;
    let failure = app.state.app_settings.history.failure_ram;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .with_context_setup(crate::app::ui::settings::tests::setup)
        .build_ui_state(
            |ui, app| {
                render_policy(ui, app);
                crate::app::ui::settings::search_options::render(ui, app);
            },
            app,
        );
    harness.run();
    harness
        .get_by_label("Reset memory cache to recommended")
        .click_accesskit();
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
    assert_eq!(
        harness.state().state.app_settings.history.failure_ram,
        failure
    );
    assert_eq!(
        harness.state().state.app_settings.search.result_limit,
        SearchResultLimit::Unlimited
    );

    harness
        .get_by_label("Reset search result limit to recommended")
        .click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.app_settings.search.result_limit,
        SearchResultLimit::Recommended
    );
    assert_eq!(
        harness.state().state.app_settings.history.failure_ram,
        failure
    );
    harness
        .get_by_label("Reset save-failure memory limit to automatic")
        .click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .state
            .app_settings
            .history
            .failure_ram
            .automatic
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
    assert_eq!(
        saved.history.failure_ram,
        harness.state().state.app_settings.history.failure_ram
    );
    assert_eq!(saved.search.result_limit, SearchResultLimit::Recommended);
    for label in [
        "Reset memory cache to recommended",
        "Reset search result limit to recommended",
        "Reset save-failure memory limit to automatic",
    ] {
        assert!(harness.get_by_label(label).accesskit_node().is_disabled());
    }
}

#[test]
fn search_slider_applies_finite_and_unlimited_limits() {
    use crate::app::services::settings_store::SearchResultLimit;
    let root = tempfile::tempdir().unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .with_context_setup(crate::app::ui::settings::tests::setup)
        .build_ui_state(
            crate::app::ui::settings::search_options::render,
            test_app(root.path()),
        );
    harness.run();
    for (stop, limit) in [
        (0.0, SearchResultLimit::from_matches(50_000)),
        (4.0, SearchResultLimit::from_matches(1_000_000)),
        (8.0, SearchResultLimit::Unlimited),
    ] {
        harness
            .get_by_role_and_label(egui::Role::Slider, "Search result limit")
            .set_value_accesskit(stop);
        harness.run_steps(2);
        assert_eq!(
            harness.state().state.app_settings.search.result_limit,
            limit
        );
        assert_eq!(
            harness
                .state()
                .state
                .persistence
                .settings_store
                .load()
                .unwrap()
                .unwrap()
                .search
                .result_limit,
            limit
        );
    }
}
