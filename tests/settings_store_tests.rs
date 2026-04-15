#![forbid(unsafe_code)]

use scratchpad::app::fonts::EditorFontPreset;
use scratchpad::app::services::settings_store::{
    AppSettings, AppThemeMode, DEFAULT_EDITOR_GUTTER, DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS,
    DEFAULT_TAB_LIST_WIDTH, FileOpenDisposition, SettingsStore, StartupSessionBehavior,
    TabListPosition,
};
use std::fs;

fn test_store() -> SettingsStore {
    let root = tempfile::tempdir().expect("create settings dir");
    SettingsStore::new(root.keep())
}

fn write_settings(store: &SettingsStore, contents: &str) {
    fs::write(store.path(), contents).expect("write settings");
}

fn legacy_defaults(font_size: f32, word_wrap: bool, logging_enabled: bool) -> AppSettings {
    AppSettings {
        font_size,
        word_wrap,
        logging_enabled,
        editor_gutter: DEFAULT_EDITOR_GUTTER,
        editor_font: EditorFontPreset::Standard,
        settings_tab_open: false,
        settings_tab_index: None,
        ..AppSettings::default()
    }
}

fn assert_current_defaults(settings: &AppSettings) {
    assert_eq!(settings.tab_list_position, TabListPosition::Top);
    assert_eq!(settings.file_open_disposition, FileOpenDisposition::NewTab);
    assert_eq!(
        settings.startup_session_behavior,
        StartupSessionBehavior::ContinuePreviousSession
    );
    assert_eq!(settings.tab_list_width, DEFAULT_TAB_LIST_WIDTH);
    assert_eq!(
        settings.tab_list_auto_hide_delay_seconds,
        DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS
    );
    assert!(settings.recent_files_enabled);
}

#[test]
fn missing_settings_file_returns_none() {
    let store = test_store();

    assert_eq!(store.load().expect("load settings"), None);
}

#[test]
fn save_and_load_round_trip_toml_settings() {
    let store = test_store();
    let settings = AppSettings {
        font_size: 18.0,
        word_wrap: false,
        logging_enabled: false,
        editor_gutter: 6,
        editor_font: EditorFontPreset::Standard,
        theme_mode: AppThemeMode::Light,
        editor_text_color: "#111111".to_owned(),
        editor_background_color: "#eeeeee".to_owned(),
        tab_list_position: TabListPosition::Right,
        file_open_disposition: FileOpenDisposition::CurrentTab,
        startup_session_behavior: StartupSessionBehavior::StartFreshSession,
        tab_list_width: 220.0,
        auto_hide_tab_list: true,
        tab_list_auto_hide_delay_seconds: 4.5,
        recent_files_enabled: false,
        settings_tab_open: true,
        settings_tab_index: Some(2),
    };

    store.save(&settings).expect("save settings");
    let loaded = store.load().expect("load settings");

    assert_eq!(loaded, Some(settings));
    assert_eq!(
        store.path().file_name().and_then(|name| name.to_str()),
        Some("settings.toml")
    );
}

#[test]
fn malformed_toml_returns_invalid_data_error() {
    let store = test_store();
    write_settings(&store, "font_size = [oops");

    let error = store.load().expect_err("load should fail");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn missing_editor_font_field_defaults_for_older_toml() {
    let store = test_store();
    write_settings(
        &store,
        "font_size = 16.0\nword_wrap = false\nlogging_enabled = true\n",
    );

    let loaded = store.load().expect("load settings");
    assert_eq!(loaded, Some(legacy_defaults(16.0, false, true)));
}

#[test]
fn missing_tab_list_position_defaults_for_older_toml() {
    let store = test_store();
    write_settings(
        &store,
        "font_size = 16.0\nword_wrap = false\nlogging_enabled = true\n",
    );

    let loaded = store.load().expect("load settings").expect("settings");
    assert_current_defaults(&loaded);
}

#[test]
fn legacy_auto_hide_fields_migrate_to_single_tab_list_setting() {
    let store = test_store();
    write_settings(
        &store,
        "font_size = 16.0\nword_wrap = false\nlogging_enabled = true\nauto_hide_top_bars = true\n",
    );

    let loaded = store.load().expect("load settings").expect("settings");

    assert!(loaded.auto_hide_tab_list);
}

#[test]
fn legacy_yaml_migrates_to_toml_when_toml_is_missing() {
    let store = test_store();
    let legacy_path = store.path().with_file_name("settings.yaml");
    fs::write(
        &legacy_path,
        "font_size: 16.0\nword_wrap: false\nlogging_enabled: true\n",
    )
    .expect("write legacy yaml");

    let loaded = store.load().expect("load settings");

    assert_eq!(loaded, Some(legacy_defaults(16.0, false, true)));
    assert!(store.path().exists());
    assert!(legacy_path.exists());
}
