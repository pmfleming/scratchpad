use super::{
    AppSettings, FileOpenDisposition, ScratchpadApp, SessionStore, SettingsStore, StartupExecution,
    StartupOpenTarget, StartupOptions, StartupSessionBehavior,
};
use crate::app::{
    app_state::test_support::wait_for_background_io,
    domain::{BufferState, WorkspaceTab},
};
use std::path::Path;

fn launch(
    root: &Path,
    execution: StartupExecution,
    saved: bool,
    settings: Option<AppSettings>,
    options: StartupOptions,
) -> ScratchpadApp {
    let sessions = SessionStore::new(root.join("session"));
    if saved {
        let tabs = ["first", "second"]
            .map(|name| WorkspaceTab::new(BufferState::new(name.into(), name.into(), None)));
        sessions.persist(&tabs, 1, 18.0, false).unwrap();
    }
    let store = SettingsStore::new(root.join("settings"));
    if let Some(settings) = settings {
        store.save(&settings).unwrap();
    }
    let mut app = ScratchpadApp::build_app(sessions, store, options, execution);
    app.set_session_persist_on_drop(false);
    wait_for_background_io(&mut app);
    assert!(!app.startup_restore_pending());
    app
}

#[test]
fn startup_modes_share_settings_precedence_restore_and_fallback_policy() {
    for execution in [StartupExecution::Blocking, StartupExecution::Background] {
        for saved in [false, true] {
            for settings_file in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let mut settings = AppSettings::default();
                settings.editor.font_size = 23.0;
                let app = launch(
                    root.path(),
                    execution,
                    saved,
                    settings_file.then_some(settings),
                    StartupOptions::default(),
                );
                assert_eq!(app.tab_manager.tabs.len(), if saved { 2 } else { 1 });
                assert_eq!(app.tab_manager.active_tab_index, usize::from(saved));
                let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
                assert_eq!(buffer.text(), if saved { "second" } else { "" });
                let expected = if settings_file {
                    23.0
                } else if saved {
                    18.0
                } else {
                    AppSettings::default().editor.font_size
                };
                assert_eq!(app.state.app_settings.editor.font_size, expected);
                assert!(!app.state.persistence.session_restore_failed);
            }
        }
    }
}

#[test]
fn startup_file_targets_and_explicit_restore_override_saved_preferences() {
    for execution in [StartupExecution::Blocking, StartupExecution::Background] {
        for (setting_restore, explicit, restored) in [
            (false, None, false),
            (false, Some(true), true),
            (true, Some(false), false),
        ] {
            for target in [
                None,
                Some(StartupOpenTarget::SeparateTabs),
                Some(StartupOpenTarget::TabIndex(0)),
            ] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("opened.txt");
                std::fs::write(&path, "opened").unwrap();
                let mut settings = AppSettings::default();
                settings.workspace.startup_session_behavior = if setting_restore {
                    StartupSessionBehavior::ContinuePreviousSession
                } else {
                    StartupSessionBehavior::StartFreshSession
                };
                settings.workspace.file_open_disposition = FileOpenDisposition::CurrentTab;
                let options = StartupOptions {
                    restore_session: explicit.unwrap_or(true),
                    restore_session_explicit: explicit.is_some(),
                    open_target: target.unwrap_or(StartupOpenTarget::SeparateTabs),
                    open_target_explicit: target.is_some(),
                    files: vec![path.clone()],
                    startup_notice: None,
                };
                let app = launch(root.path(), execution, true, Some(settings), options);
                let separate = target == Some(StartupOpenTarget::SeparateTabs);
                assert_eq!(
                    app.tab_manager.tabs.len(),
                    1 + usize::from(restored) + usize::from(separate)
                );
                let (index, _) = app.tab_manager.find_tab_by_path(&path).unwrap();
                assert_eq!(
                    index,
                    if separate {
                        1 + usize::from(restored)
                    } else if target == Some(StartupOpenTarget::TabIndex(0)) {
                        0
                    } else {
                        usize::from(restored)
                    }
                );
                let texts: Vec<_> = app
                    .tab_manager
                    .tabs
                    .as_slice()
                    .iter()
                    .flat_map(WorkspaceTab::buffers)
                    .map(BufferState::text)
                    .collect();
                assert!(texts.iter().any(|text| text == "opened"));
                assert_eq!(texts.iter().any(|text| text == "first"), restored);
                assert_eq!(
                    texts.iter().filter(|text| text.is_empty()).count(),
                    usize::from(!restored),
                    "the initial untitled buffer is retained, not duplicated"
                );
            }
        }
    }
}

#[test]
fn startup_failure_keeps_the_save_fence_with_or_without_requested_files() {
    for execution in [StartupExecution::Blocking, StartupExecution::Background] {
        for with_file in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(root.path().join("session"), "preserve broken store").unwrap();
            let path = root.path().join("opened.txt");
            std::fs::write(&path, "opened").unwrap();
            let options = StartupOptions {
                files: if with_file {
                    vec![path.clone()]
                } else {
                    vec![]
                },
                ..StartupOptions::default()
            };
            let mut app = launch(root.path(), execution, false, None, options);
            assert!(app.state.persistence.session_restore_failed);
            assert_eq!(app.tab_manager.tabs.len(), 1 + usize::from(with_file));
            assert_eq!(app.tab_manager.find_tab_by_path(&path).is_some(), with_file);
            crate::app::services::session_manager::persist_session_now(&mut app).unwrap();
            assert_eq!(
                std::fs::read_to_string(root.path().join("session")).unwrap(),
                "preserve broken store"
            );
        }
    }
}
