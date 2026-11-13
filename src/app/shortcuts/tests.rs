use super::handle_shortcuts;
use crate::app::app_state::{ScratchpadApp, test_support::test_app, workspace::accessors};
use crate::app::commands::{AppCommand, SettingsCommand, handle_command};
use crate::app::domain::{BufferState, SplitAxis, TabManager, ViewId, WorkspaceTab};
use crate::app::shortcut_keymap::ShortcutAction;
use eframe::egui;

fn press(ctx: &egui::Context, app: &mut ScratchpadApp, key: egui::Key, modifiers: egui::Modifiers) {
    ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ui| handle_shortcuts(app, ui.ctx()),
    )
    .textures_delta
    .clear();
}

fn zoom_label(app: &ScratchpadApp, view: ViewId) -> String {
    let tab = app.tab_manager.active_tab().unwrap();
    tab.layout.view(view).unwrap().zoom_label()
}

fn line_numbers_visible(app: &ScratchpadApp) -> bool {
    let tab = app.tab_manager.active_tab().unwrap();
    tab.layout.line_numbers_visible()
}

#[test]
fn zoom_shortcuts_are_view_local_and_reset_is_distinct_from_line_numbers() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let mut tab = WorkspaceTab::new(BufferState::new("note".into(), "text".into(), None));
    let first = tab.layout.active_view_id();
    let second = tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.activate_view(first);
    app.tab_manager.set_tabs(vec![tab], 0);
    let baseline = app.state.app_settings.font_size();
    let ctx = egui::Context::default();
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    let ctrl = egui::Modifiers::CTRL;

    press(&ctx, &mut app, egui::Key::Equals, ctrl);
    assert_eq!(zoom_label(&app, first), "110%");
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .activate_view(second);
    press(&ctx, &mut app, egui::Key::Minus, ctrl);
    assert_eq!(zoom_label(&app, second), "90%");
    press(&ctx, &mut app, egui::Key::Num0, ctrl);
    assert_eq!(zoom_label(&app, second), "100%");
    assert_eq!(zoom_label(&app, first), "110%");
    assert!(!line_numbers_visible(&app));
    press(&ctx, &mut app, egui::Key::Num9, ctrl);
    assert!(line_numbers_visible(&app));

    // A rebound reset frees its default key.
    app.state
        .app_settings
        .shortcuts
        .bindings
        .insert("reset_zoom".into(), "alt+r".into());
    press(&ctx, &mut app, egui::Key::Equals, ctrl);
    press(&ctx, &mut app, egui::Key::Num0, ctrl);
    assert_eq!(zoom_label(&app, second), "110%");
    press(&ctx, &mut app, egui::Key::R, egui::Modifiers::ALT);
    assert_eq!(zoom_label(&app, second), "100%");

    // Settings own the keyboard while open.
    handle_command(
        &mut app,
        AppCommand::Settings(SettingsCommand::OpenSettings),
    );
    press(&ctx, &mut app, egui::Key::Equals, ctrl);
    assert_eq!(zoom_label(&app, second), "100%");
    assert_eq!(app.state.app_settings.font_size(), baseline);
}

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
        let mut app = test_app(root.path());
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
        press(
            &egui::Context::default(),
            &mut app,
            egui::Key::Q,
            egui::Modifiers::ALT,
        );
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
