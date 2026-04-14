#![forbid(unsafe_code)]

use scratchpad::ScratchpadApp;
use scratchpad::app::domain::PaneNode;
use scratchpad::app::services::file_controller::FileController;
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::services::settings_store::{
    AppSettings, FileOpenDisposition, SettingsStore, StartupSessionBehavior,
};
use scratchpad::app::startup::{StartupOpenTarget, StartupOptions};
use std::fs;

fn collect_leaf_area_fractions(node: &PaneNode, area_fraction: f32, output: &mut Vec<f32>) {
    match node {
        PaneNode::Leaf { .. } => output.push(area_fraction),
        PaneNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            collect_leaf_area_fractions(first, area_fraction * ratio, output);
            collect_leaf_area_fractions(second, area_fraction * (1.0 - ratio), output);
        }
    }
}

fn test_app() -> ScratchpadApp {
    let session_root = tempfile::tempdir().expect("create session dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    ScratchpadApp::with_session_store(session_store)
}

fn write_settings_file(app: &ScratchpadApp, contents: &str) {
    let path = app.settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create settings dir");
    }
    fs::write(path, contents).expect("write settings file");
}

#[test]
fn open_here_splits_file_into_current_workspace() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("split-target.txt");
    fs::write(&path, "alpha\nbeta\n").expect("write temp file");

    let mut app = test_app();
    FileController::open_external_paths_here(&mut app, vec![path.clone()]);

    assert_eq!(app.tabs().len(), 1);
    let tab = &app.tabs()[app.active_tab_index()];
    assert_eq!(tab.views.len(), 2);
    assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
}

#[test]
fn open_file_flags_settings_toml_buffer() {
    let mut app = test_app();
    write_settings_file(
        &app,
        "font_size = 14.0\nword_wrap = true\nlogging_enabled = true\n",
    );
    let settings_path = app.settings_path().to_path_buf();

    FileController::open_external_paths(&mut app, vec![settings_path.clone()]);

    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(settings_path.as_path())
    );
    assert!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .is_settings_file
    );
}

#[test]
fn open_here_flags_settings_toml_buffer() {
    let mut app = test_app();
    write_settings_file(
        &app,
        "font_size = 14.0\nword_wrap = true\nlogging_enabled = true\n",
    );
    let settings_path = app.settings_path().to_path_buf();

    FileController::open_external_paths_here(&mut app, vec![settings_path.clone()]);

    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(settings_path.as_path())
    );
    assert!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .is_settings_file
    );
}

#[test]
fn opening_file_from_dirty_settings_tab_refreshes_settings_on_focus_loss() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("opened.txt");
    fs::write(&path, "alpha\n").expect("write temp file");

    let mut app = test_app();
    write_settings_file(
        &app,
        "font_size = 14.0\nword_wrap = true\nlogging_enabled = true\n",
    );
    let settings_path = app.settings_path().to_path_buf();
    FileController::open_external_paths(&mut app, vec![settings_path]);
    let settings_tab_index = app.active_tab_index();

    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .replace_text(
            [
                "font_size = 24.0",
                "word_wrap = false",
                "logging_enabled = false",
                "editor_font = \"standard\"",
                "settings_tab_open = false",
                "",
            ]
            .join("\n"),
        );
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .is_dirty = true;

    FileController::open_external_paths(&mut app, vec![path.clone()]);

    assert_eq!(app.font_size(), 24.0);
    assert!(!app.word_wrap());
    assert!(!app.logging_enabled());
    assert_eq!(
        app.editor_font(),
        scratchpad::app::fonts::EditorFontPreset::Standard
    );
    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(path.as_path())
    );
}

#[test]
fn opening_file_here_from_dirty_settings_tab_refreshes_settings_on_focus_loss() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("opened-here.txt");
    fs::write(&path, "beta\n").expect("write temp file");

    let mut app = test_app();
    write_settings_file(
        &app,
        "font_size = 14.0\nword_wrap = true\nlogging_enabled = true\n",
    );
    let settings_path = app.settings_path().to_path_buf();
    FileController::open_external_paths(&mut app, vec![settings_path]);
    let settings_tab_index = app.active_tab_index();

    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .replace_text(
            [
                "font_size = 25.0",
                "word_wrap = false",
                "logging_enabled = false",
                "editor_font = \"standard\"",
                "settings_tab_open = false",
                "",
            ]
            .join("\n"),
        );
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .is_dirty = true;

    FileController::open_external_paths_here(&mut app, vec![path.clone()]);

    assert_eq!(app.font_size(), 25.0);
    assert!(!app.word_wrap());
    assert!(!app.logging_enabled());
    assert_eq!(
        app.editor_font(),
        scratchpad::app::fonts::EditorFontPreset::Standard
    );
    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(path.as_path())
    );
}

#[test]
fn open_here_migrates_existing_tab_into_current_workspace() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("migrate-me.txt");
    fs::write(&path, "gamma\ndelta\n").expect("write temp file");

    let mut app = test_app();
    FileController::open_external_paths(&mut app, vec![path.clone()]);
    app.create_untitled_tab();

    FileController::open_external_paths_here(&mut app, vec![path.clone()]);

    assert_eq!(app.tabs().len(), 2);
    let tab = &app.tabs()[app.active_tab_index()];
    assert_eq!(tab.views.len(), 2);
    assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
}

#[test]
fn open_here_batches_multiple_new_files_into_equal_tile_shares() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let first_path = temp_dir.path().join("first.txt");
    let second_path = temp_dir.path().join("second.txt");
    let third_path = temp_dir.path().join("third.txt");
    fs::write(&first_path, "first\n").expect("write first temp file");
    fs::write(&second_path, "second\n").expect("write second temp file");
    fs::write(&third_path, "third\n").expect("write third temp file");

    let mut app = test_app();
    FileController::open_external_paths_here(
        &mut app,
        vec![first_path.clone(), second_path.clone(), third_path.clone()],
    );

    let tab = &app.tabs()[app.active_tab_index()];
    assert_eq!(tab.views.len(), 4);

    let mut areas = Vec::new();
    collect_leaf_area_fractions(&tab.root_pane, 1.0, &mut areas);
    assert!(areas.iter().all(|area| (area - 0.25).abs() < f32::EPSILON));
}

#[test]
fn startup_clean_launch_skips_restored_session() {
    let session_root = tempfile::tempdir().expect("create session dir");
    let settings_root = tempfile::tempdir().expect("create settings dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());

    let mut original = ScratchpadApp::with_session_store(session_store);
    original.tabs_mut()[0].buffer.name = "restored.txt".to_owned();
    original.create_untitled_tab();
    original.tabs_mut()[1].buffer.name = "second.txt".to_owned();
    original
        .session_store()
        .persist(
            original.tabs(),
            original.active_tab_index(),
            original.font_size(),
            original.word_wrap(),
            original.logging_enabled(),
        )
        .expect("persist session");

    let clean_store = SessionStore::new(session_root.path().to_path_buf());
    let clean_options = StartupOptions {
        restore_session: false,
        restore_session_explicit: true,
        ..Default::default()
    };
    let clean = ScratchpadApp::with_stores_and_startup(
        clean_store,
        SettingsStore::new(settings_root.path().to_path_buf()),
        clean_options,
    );

    assert_eq!(clean.tabs().len(), 1);
    assert_eq!(clean.tabs()[0].buffer.name, "Untitled");
}

#[test]
fn startup_active_target_adds_files_into_current_workspace() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("startup-here.txt");
    fs::write(&path, "hello\nworld\n").expect("write temp file");

    let session_root = tempfile::tempdir().expect("create session dir");
    let settings_root = tempfile::tempdir().expect("create settings dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    let options = StartupOptions {
        open_target: StartupOpenTarget::ActiveTab,
        open_target_explicit: true,
        files: vec![path.clone()],
        ..Default::default()
    };
    let app = ScratchpadApp::with_stores_and_startup(
        session_store,
        SettingsStore::new(settings_root.path().to_path_buf()),
        options,
    );

    assert_eq!(app.tabs().len(), 1);
    let tab = &app.tabs()[app.active_tab_index()];
    assert_eq!(tab.views.len(), 2);
    assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
}

#[test]
fn startup_uses_saved_file_open_preference_when_cli_target_is_not_explicit() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("startup-prefers-current-tab.txt");
    fs::write(&path, "hello\nworld\n").expect("write temp file");

    let session_root = tempfile::tempdir().expect("create session dir");
    let root = session_root.path().to_path_buf();
    SettingsStore::new(root.clone())
        .save(&AppSettings {
            file_open_disposition: FileOpenDisposition::CurrentTab,
            ..AppSettings::default()
        })
        .expect("save startup preferences");

    let app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.clone()),
        SettingsStore::new(root),
        StartupOptions {
            files: vec![path.clone()],
            ..Default::default()
        },
    );

    assert_eq!(app.tabs().len(), 1);
    let tab = &app.tabs()[app.active_tab_index()];
    assert_eq!(tab.views.len(), 2);
    assert_eq!(tab.active_buffer().path.as_deref(), Some(path.as_path()));
}

#[test]
fn saved_startup_behavior_can_skip_session_restore_without_clean_switch() {
    let session_root = tempfile::tempdir().expect("create session dir");
    let root = session_root.path().to_path_buf();

    let mut original = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.clone()),
        SettingsStore::new(root.clone()),
        StartupOptions::default(),
    );
    original.tabs_mut()[0].buffer.name = "restored.txt".to_owned();
    original.create_untitled_tab();
    SettingsStore::new(root.clone())
        .save(&AppSettings {
            startup_session_behavior: StartupSessionBehavior::StartFreshSession,
            ..AppSettings::default()
        })
        .expect("save startup behavior");
    original
        .session_store()
        .persist(
            original.tabs(),
            original.active_tab_index(),
            original.font_size(),
            original.word_wrap(),
            original.logging_enabled(),
        )
        .expect("persist session");

    let restored = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.clone()),
        SettingsStore::new(root),
        StartupOptions::default(),
    );

    assert_eq!(restored.tabs().len(), 1);
    assert_eq!(restored.tabs()[0].buffer.name, "Untitled");
}
