use super::ScratchpadApp;
use crate::app::app_state::settings_controller;
use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;
use crate::app::startup::StartupOptions;

#[test]
fn command_open_settings_selects_only_settings_slot() {
    let mut app = test_app(["one.txt", "two.txt"]);
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(app, 0);
    crate::app::app_state::workspace::display_tabs::toggle_tab_slot_selection(app, 1);

    settings_controller::open_settings(&mut app);

    assert!(crate::app::app_state::settings_state::showing_settings(app));
    assert_eq!(
        selected_slots(&app),
        vec![crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app)]
    );
    assert!(
        crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app)
        )
    );
}

#[test]
fn tab_strip_open_settings_can_preserve_existing_selection() {
    let mut app = test_app(["one.txt", "two.txt"]);
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(app, 0);
    crate::app::app_state::workspace::display_tabs::toggle_tab_slot_selection(app, 1);

    settings_controller::open_settings_preserving_tab_selection(&mut app);

    assert!(crate::app::app_state::settings_state::showing_settings(app));
    assert_eq!(
        selected_slots(&app),
        vec![
            0,
            1,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app)
        ]
    );
}

#[test]
fn close_settings_selects_only_active_workspace_slot() {
    let mut app = test_app(["one.txt", "two.txt"]);
    settings_controller::open_settings_preserving_tab_selection(&mut app);
    crate::app::app_state::workspace::display_tabs::toggle_tab_slot_selection(app, 0);

    settings_controller::close_settings(&mut app);

    assert!(!crate::app::app_state::settings_state::showing_settings(
        app
    ));
    assert_eq!(
        selected_slots(&app),
        vec![crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app)]
    );
    assert!(
        !crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app)
        )
    );
}

#[test]
fn startup_keeps_workspace_active_when_settings_tab_is_open() {
    let root = temp_root();
    let mut app = app_with_root(&root);

    assert!(crate::app::app_state::settings_state::settings_tab_open(
        app
    ));
    assert!(!crate::app::app_state::settings_state::showing_settings(
        app
    ));
    crate::app::app_state::workspace::accessors::persist_session_now(app).unwrap();

    let mut restored = app_with_root(&root);
    restored.set_session_persist_on_drop(false);

    assert!(crate::app::app_state::settings_state::settings_tab_open(
        &restored
    ));
    assert!(!crate::app::app_state::settings_state::showing_settings(
        &restored
    ));
    assert!(
        !crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            &restored,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&restored)
        )
    );
    assert_eq!(
        selected_slots(&restored),
        vec![crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&restored)]
    );
}

#[test]
fn startup_restores_settings_as_active_surface_from_session() {
    let root = temp_root();
    let mut app = app_with_root(&root);

    settings_controller::open_settings(&mut app);
    assert!(crate::app::app_state::settings_state::showing_settings(app));
    crate::app::app_state::workspace::accessors::persist_session_now(app).unwrap();

    let mut restored = app_with_root(&root);
    restored.set_session_persist_on_drop(false);

    assert!(crate::app::app_state::settings_state::showing_settings(
        &restored
    ));
    assert!(
        crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            &restored,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&restored)
        )
    );
    assert_eq!(
        selected_slots(&restored),
        vec![crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&restored)]
    );
}

#[test]
fn reset_settings_to_defaults_restores_startup_default_state() {
    let mut app = test_app(["custom.txt"]);
    settings_controller::close_settings(&mut app);

    settings_controller::reset_settings_to_defaults(&mut app);

    assert!(crate::app::app_state::settings_state::showing_settings(app));
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
        app.tab_manager.tabs.as_slice()[0].buffers.buffer.name,
        crate::app::services::manual_files::USER_MANUAL_FILE_NAME
    );
    assert!(
        crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app)
        )
    );
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
    crate::app::app_state::workspace::display_tabs::clear_tab_selection(app);
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
