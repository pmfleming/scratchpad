use scratchpad::app::fonts::EditorFontPreset;
use scratchpad::app::services::settings_store::{
    AppSettings, FileOpenDisposition, SettingsStore, StartupSessionBehavior, TabListPosition,
};

#[test]
fn missing_settings_file_returns_none() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());

    assert!(store.load().unwrap().is_none());
}

#[test]
fn save_and_load_round_trip_toml_settings() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());
    let settings = AppSettings {
        font_size: 18.0,
        word_wrap: false,
        editor_font: EditorFontPreset::Mono,
        tab_list_position: TabListPosition::Left,
        file_open_disposition: FileOpenDisposition::CurrentTab,
        startup_session_behavior: StartupSessionBehavior::StartFreshSession,
        ..AppSettings::default()
    };

    store.save(&settings).unwrap();
    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.font_size, 18.0);
    assert!(!loaded.word_wrap);
    assert_eq!(loaded.editor_font, EditorFontPreset::Mono);
    assert_eq!(loaded.tab_list_position, TabListPosition::Left);
    assert_eq!(
        loaded.file_open_disposition,
        FileOpenDisposition::CurrentTab
    );
    assert_eq!(
        loaded.startup_session_behavior,
        StartupSessionBehavior::StartFreshSession
    );
}

#[test]
fn malformed_toml_returns_invalid_data() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("settings.toml"), "font_size = [").unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());

    let error = store.load().unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn older_toml_missing_newer_fields_uses_defaults() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("settings.toml"),
        r##"
font_size = 16.0
word_wrap = true
editor_text_color = "#ffffff"
editor_background_color = "#15181d"
editor_text_highlight_color = "#fff36d"
editor_text_highlight_text_color = "#0b0f3d"
tab_list_width = 184.0
auto_hide_tab_list = false
tab_list_auto_hide_delay_seconds = 3.0
recent_files_enabled = true
status_bar_visible = true
"##,
    )
    .unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());

    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.editor_font, EditorFontPreset::default());
    assert_eq!(loaded.tab_list_position, TabListPosition::Top);
    assert_eq!(loaded.file_open_disposition, FileOpenDisposition::NewTab);
}

#[test]
fn legacy_yaml_migrates_to_toml_when_toml_is_missing() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("settings.yaml"),
        r##"
font_size: 15.0
word_wrap: false
editor_gutter: 0
editor_font: mono
theme_mode: dark
editor_text_color: "#eeeeee"
editor_background_color: "#111111"
editor_text_highlight_color: "#fff36d"
editor_text_highlight_text_color: "#0b0f3d"
tab_list_position: right
tab_order_mode: custom
custom_tab_order: []
file_open_disposition: current_tab
new_tab_placement: start
startup_session_behavior: start_fresh_session
tab_list_width: 200.0
auto_hide_tab_list: true
tab_list_auto_hide_delay_seconds: 2.0
recent_files_enabled: false
status_bar_visible: false
window_state: {}
settings_tab_open: true
settings_tab_index: null
"##,
    )
    .unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());

    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.font_size, 15.0);
    assert_eq!(loaded.editor_font, EditorFontPreset::Mono);
    assert_eq!(loaded.tab_list_position, TabListPosition::Right);
    assert!(directory.path().join("settings.toml").exists());
}
