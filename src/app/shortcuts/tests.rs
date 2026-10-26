use super::handle_shortcuts;
use crate::app::app_state::{ScratchpadApp, workspace::accessors};
use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
use crate::app::services::{session_store::SessionStore, settings_store::SettingsStore};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::startup::StartupOptions;
use eframe::egui;

#[test]
fn configured_utility_keys_reach_their_application_actions() {
    type Check = fn(&ScratchpadApp) -> bool;
    let cases: &[(ShortcutAction, Check)] = &[
        (ShortcutAction::RenameTab, |app| {
            accessors::tab_rename_matches_slot(app, 0)
        }),
        (ShortcutAction::OpenTextHistory, |app| {
            app.state.dialogs.text_history.is_open()
        }),
        (ShortcutAction::OpenEncodingDialog, |app| {
            app.state.dialogs.encoding.is_open()
        }),
        (ShortcutAction::OpenStatusHistory, |app| {
            app.state.dialogs.status_history.is_open()
        }),
        (ShortcutAction::CopyActivePath, |app| {
            app.state
                .status
                .current
                .as_ref()
                .is_some_and(|status| status.text == "No file path to copy.")
        }),
        // Untitled buffers exercise the safe no-path route, never launch an explorer.
        (ShortcutAction::RevealActivePath, |app| {
            app.state
                .status
                .current
                .as_ref()
                .is_some_and(|status| status.text == "No file path to reveal.")
        }),
        (ShortcutAction::ToggleTabList, |app| {
            app.state.app_settings.auto_hide_tab_list()
        }),
        (ShortcutAction::ToggleTabListAutoHide, |app| {
            app.state.app_settings.auto_hide_tab_list()
        }),
        (ShortcutAction::ToggleReadingOrder, |app| {
            app.tab_manager
                .active_tab()
                .unwrap()
                .active_buffer()
                .right_to_left_reading_order
        }),
        (ShortcutAction::ToggleControlChars, |app| {
            app.tab_manager
                .active_tab()
                .unwrap()
                .active_buffer()
                .show_control_chars
        }),
    ];
    for &(action, check) in cases {
        let root = tempfile::tempdir().unwrap();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.path().join("session")),
            SettingsStore::new(root.path().join("settings")),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app.tab_manager = TabManager::for_test_tabs(vec![WorkspaceTab::new(BufferState::new(
            "note".into(),
            "text".into(),
            None,
        ))]);
        app.state.app_settings.workspace.auto_hide_tab_list = false;
        // A collision with the last utility action must preserve dispatch priority.
        app.state.app_settings.shortcuts.bindings = [ShortcutAction::ToggleControlChars, action]
            .into_iter()
            .map(|action| (action.config_key().into(), "alt+q".into()))
            .collect();
        assert!(!check(&app), "{action:?} must start inactive");
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Q,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            ..Default::default()
        };
        let mut output =
            egui::Context::default().run_ui(input, |ui| handle_shortcuts(&mut app, ui.ctx()));
        output.textures_delta.clear();
        assert!(check(&app), "{action:?} was not dispatched");
        assert_eq!(
            app.tab_manager
                .active_tab()
                .unwrap()
                .active_buffer()
                .show_control_chars,
            action == ShortcutAction::ToggleControlChars,
            "a key must not dispatch a second utility action",
        );
    }
}
