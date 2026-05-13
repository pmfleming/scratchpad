use super::ScratchpadApp;
use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;
use crate::app::startup::StartupOptions;

#[test]
fn command_open_settings_selects_only_settings_slot() {
    let mut app = test_app(["one.txt", "two.txt"]);
    app.select_only_tab_slot(0);
    app.toggle_tab_slot_selection(1);

    app.open_settings();

    assert!(app.showing_settings());
    assert_eq!(selected_slots(&app), vec![app.active_tab_slot_index()]);
    assert!(app.tab_slot_is_settings(app.active_tab_slot_index()));
}

#[test]
fn tab_strip_open_settings_can_preserve_existing_selection() {
    let mut app = test_app(["one.txt", "two.txt"]);
    app.select_only_tab_slot(0);
    app.toggle_tab_slot_selection(1);

    app.open_settings_preserving_tab_selection();

    assert!(app.showing_settings());
    assert_eq!(
        selected_slots(&app),
        vec![0, 1, app.active_tab_slot_index()]
    );
}

#[test]
fn close_settings_selects_only_active_workspace_slot() {
    let mut app = test_app(["one.txt", "two.txt"]);
    app.open_settings_preserving_tab_selection();
    app.toggle_tab_slot_selection(0);

    app.close_settings();

    assert!(!app.showing_settings());
    assert_eq!(selected_slots(&app), vec![app.active_tab_slot_index()]);
    assert!(!app.tab_slot_is_settings(app.active_tab_slot_index()));
}

#[test]
fn startup_keeps_workspace_active_when_settings_tab_is_open() {
    let root = temp_root();
    let mut app = app_with_root(&root);

    assert!(app.settings_tab_open());
    assert!(!app.showing_settings());
    app.persist_session_now().unwrap();

    let mut restored = app_with_root(&root);
    restored.set_session_persist_on_drop(false);

    assert!(restored.settings_tab_open());
    assert!(!restored.showing_settings());
    assert!(!restored.tab_slot_is_settings(restored.active_tab_slot_index()));
    assert_eq!(
        selected_slots(&restored),
        vec![restored.active_tab_slot_index()]
    );
}

#[test]
fn startup_restores_settings_as_active_surface_from_session() {
    let root = temp_root();
    let mut app = app_with_root(&root);

    app.open_settings();
    assert!(app.showing_settings());
    app.persist_session_now().unwrap();

    let mut restored = app_with_root(&root);
    restored.set_session_persist_on_drop(false);

    assert!(restored.showing_settings());
    assert!(restored.tab_slot_is_settings(restored.active_tab_slot_index()));
    assert_eq!(
        selected_slots(&restored),
        vec![restored.active_tab_slot_index()]
    );
}

#[test]
fn reset_settings_to_defaults_restores_startup_default_state() {
    let mut app = test_app(["custom.txt"]);
    app.close_settings();

    app.reset_settings_to_defaults();

    assert!(app.showing_settings());
    assert_eq!(
        app.state.app_settings.tab_list_position(),
        crate::app::services::settings_store::TabListPosition::Top
    );
    assert_eq!(
        app.state.app_settings.theme_mode(),
        crate::app::services::settings_store::AppThemeMode::System
    );
    assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].buffer.name,
        crate::app::services::manual_files::USER_MANUAL_FILE_NAME
    );
    assert!(app.tab_slot_is_settings(app.active_tab_slot_index()));
}

fn test_app<const N: usize>(names: [&str; N]) -> ScratchpadApp {
    let temp_dir = tempfile::tempdir().expect("create temp app root");
    let root = temp_dir.keep();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.clone()),
        SettingsStore::new(root),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app.tab_manager = TabManager {
        tabs: names.into_iter().map(test_tab).collect(),
        active_tab_index: 0,
        pending_action: None,
        session_dirty: false,
        pending_scroll_to_active: false,
        buffer_tab_index: Default::default(),
        cold_session_tabs: Default::default(),
    };
    app.tab_manager.rebuild_buffer_tab_index();
    app.clear_tab_selection();
    app
}

fn temp_root() -> std::path::PathBuf {
    tempfile::tempdir().expect("create temp app root").keep()
}

fn app_with_root(root: &std::path::Path) -> ScratchpadApp {
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app
}

fn test_tab(name: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
}

fn selected_slots(app: &ScratchpadApp) -> Vec<usize> {
    app.state.workspace_selection.selected_slots().collect()
}
