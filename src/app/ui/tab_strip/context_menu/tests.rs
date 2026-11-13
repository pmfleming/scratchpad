use super::attach_tab_context_menu;
use crate::app::app_state::{
    ScratchpadApp,
    test_support::{test_app, ui_harness},
};
use crate::app::domain::{BufferState, PendingAction, WorkspaceTab};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};

/// Two clean tabs; the first tab button opens the context menu.
fn menu_harness(root: &std::path::Path) -> Harness<'static, ScratchpadApp> {
    let mut app = test_app(root);
    let tabs = ["a.txt", "b.txt"]
        .map(|name| WorkspaceTab::new(BufferState::new(name.into(), String::new(), None)));
    app.tab_manager.set_tabs(tabs.into(), 1);
    app.state.app_settings.ui.settings_tab_open = false;
    let mut harness = ui_harness(egui::vec2(900.0, 900.0), app, |ui, app| {
        let response = ui.button("first tab");
        attach_tab_context_menu(&response, app, 0);
    });
    harness.get_by_label("first tab").click_secondary();
    harness.run_steps(3);
    harness
}

#[test]
fn context_menu_lists_tab_actions_and_tab_list_controls() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = menu_harness(root.path());
    for label in ["Close", "Rename", "Order tabs", "Pinned", "Auto-hide"] {
        harness.get_by_label(label);
    }
    harness.get_by_label("Auto-hide").click();
    harness.run_steps(3);
    assert!(harness.state().state.app_settings.auto_hide_tab_list());
}

#[test]
fn close_asks_to_close_the_clicked_tab_and_dismisses_the_menu() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = menu_harness(root.path());
    harness
        .get_by_role_and_label(egui::Role::Button, "Close")
        .click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().tab_manager.pending_action,
        Some(PendingAction::CloseTab(0))
    );
    assert!(harness.query_by_label("Order tabs").is_none());
}
