use scratchpad::app::fonts::EditorFontPreset;
use scratchpad::app::platform::PlatformProfile;
use scratchpad::app::services::settings_store::{
    AppSettings, FileOpenDisposition, IndentationStyle, SettingsStore, StartupSessionBehavior,
    TabDisplayMode, TabListPosition, WindowState,
};

struct SettingsFixture {
    directory: tempfile::TempDir,
    store: SettingsStore,
}

impl SettingsFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(directory.path().to_path_buf());
        Self { directory, store }
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.directory.path().join(name), contents).unwrap();
    }

    fn load(&self) -> AppSettings {
        self.store.load().unwrap().unwrap()
    }

    fn path_exists(&self, name: &str) -> bool {
        self.directory.path().join(name).exists()
    }
}

fn assert_newer_settings_defaults(settings: &AppSettings) {
    assert_eq!(settings.editor.editor_font, EditorFontPreset::default());
    assert_eq!(
        settings.editor.indentation_style,
        IndentationStyle::TabCharacter
    );
    assert_eq!(settings.editor.tab_display, TabDisplayMode::Hidden);
    assert_eq!(settings.workspace.tab_list_position, TabListPosition::Top);
    assert_eq!(
        settings.workspace.file_open_disposition,
        FileOpenDisposition::NewTab
    );
    assert_eq!(settings.platform.profile, PlatformProfile::Auto);
    assert!(settings.shortcuts.bindings.is_empty());
}

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
    let mut settings = AppSettings::default();
    settings.editor.font_size = 18.0;
    settings.editor.word_wrap = false;
    settings.editor.editor_font = EditorFontPreset::Mono;
    settings.editor.indentation_style = IndentationStyle::Spaces;
    settings.editor.tab_display = TabDisplayMode::Tablines;
    settings.workspace.tab_list_position = TabListPosition::Left;
    settings.workspace.file_open_disposition = FileOpenDisposition::CurrentTab;
    settings.workspace.startup_session_behavior = StartupSessionBehavior::StartFreshSession;

    store.save(&settings).unwrap();
    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.editor.font_size, 18.0);
    assert!(!loaded.editor.word_wrap);
    assert_eq!(loaded.editor.editor_font, EditorFontPreset::Mono);
    assert_eq!(loaded.editor.indentation_style, IndentationStyle::Spaces);
    assert_eq!(loaded.editor.tab_display, TabDisplayMode::Tablines);
    assert_eq!(loaded.workspace.tab_list_position, TabListPosition::Left);
    assert_eq!(
        loaded.workspace.file_open_disposition,
        FileOpenDisposition::CurrentTab
    );
    assert_eq!(
        loaded.workspace.startup_session_behavior,
        StartupSessionBehavior::StartFreshSession
    );
}

#[test]
fn save_omits_ephemeral_tab_order_and_window_geometry() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());
    let mut settings = AppSettings::default();
    settings.workspace.custom_tab_order = vec![10, 20, 30];
    settings.ui.window_state = WindowState {
        position: Some([40.0, 60.0]),
        inner_size: Some([1024.0, 768.0]),
        maximized: true,
    };

    store.save(&settings).unwrap();

    let raw = std::fs::read_to_string(directory.path().join("settings.toml")).unwrap();
    let table = raw.parse::<toml::Table>().unwrap();
    let window_state = table
        .get("ui")
        .and_then(toml::Value::as_table)
        .and_then(|ui| ui.get("window_state"))
        .and_then(toml::Value::as_table)
        .unwrap();

    assert!(!table.contains_key("custom_tab_order"));
    assert!(
        !table
            .get("workspace")
            .and_then(toml::Value::as_table)
            .is_some_and(|workspace| workspace.contains_key("custom_tab_order"))
    );
    assert!(!window_state.contains_key("position"));
    assert!(!window_state.contains_key("inner_size"));
    assert_eq!(
        window_state.get("maximized").and_then(toml::Value::as_bool),
        Some(true)
    );
}

#[test]
fn load_ignores_ephemeral_tab_order_and_window_geometry() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("settings.toml"),
        r##"
[editor]
font_size = 16.0
word_wrap = true

[workspace]
custom_tab_order = [10, 20, 30]

[ui.window_state]
position = [40.0, 60.0]
inner_size = [1024.0, 768.0]
maximized = true
"##,
    )
    .unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());

    let loaded = store.load().unwrap().unwrap();

    assert!(loaded.workspace.custom_tab_order.is_empty());
    assert_eq!(loaded.ui.window_state.position, None);
    assert_eq!(loaded.ui.window_state.inner_size, None);
    assert!(loaded.ui.window_state.maximized);
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
    let fixture = SettingsFixture::new();
    fixture.write(
        "settings.toml",
        r##"
[editor]
font_size = 16.0
word_wrap = true
editor_text_color = "#ffffff"
editor_background_color = "#15181d"
editor_text_highlight_color = "#fff36d"
editor_text_highlight_text_color = "#0b0f3d"

[workspace]
tab_list_width = 184.0
auto_hide_tab_list = false
tab_list_auto_hide_delay_seconds = 3.0
recent_files_enabled = true

[ui]
status_bar_visible = true
"##,
    );

    assert_newer_settings_defaults(&fixture.load());
}

#[test]
fn legacy_show_tab_characters_boolean_maps_to_character_mode() {
    let fixture = SettingsFixture::new();
    fixture.write(
        "settings.toml",
        r#"
[editor]
show_tab_characters = true
"#,
    );

    assert_eq!(fixture.load().editor.tab_display, TabDisplayMode::Character);
}

#[test]
fn legacy_yaml_migrates_to_toml_when_toml_is_missing() {
    let fixture = SettingsFixture::new();
    fixture.write(
        "settings.yaml",
        r##"
editor:
  font_size: 15.0
  word_wrap: false
  editor_gutter: 0
  editor_font: mono
  theme_mode: dark
  editor_text_color: "#eeeeee"
  editor_background_color: "#111111"
  editor_text_highlight_color: "#fff36d"
  editor_text_highlight_text_color: "#0b0f3d"
workspace:
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
ui:
  status_bar_visible: false
  window_state: {}
  settings_tab_open: true
  settings_tab_index: null
"##,
    );

    let loaded = fixture.load();

    assert_eq!(loaded.editor.font_size, 15.0);
    assert_eq!(loaded.editor.editor_font, EditorFontPreset::Mono);
    assert_eq!(loaded.workspace.tab_list_position, TabListPosition::Right);
    assert_eq!(loaded.platform.profile, PlatformProfile::Auto);
    assert!(fixture.path_exists("settings.toml"));
}

#[test]
fn platform_profile_round_trips_in_toml_settings() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());
    let mut settings = AppSettings::default();
    settings.platform.profile = PlatformProfile::Hyprland;

    store.save(&settings).unwrap();
    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.platform.profile, PlatformProfile::Hyprland);
}

#[test]
fn shortcut_overrides_round_trip_in_toml_settings() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());
    let mut settings = AppSettings::default();
    settings
        .shortcuts
        .bindings
        .insert("open_file".to_owned(), "ctrl+shift+p".to_owned());

    store.save(&settings).unwrap();
    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.shortcuts.binding("open_file"), Some("ctrl+shift+p"));
}

#[test]
fn shortcut_overrides_load_from_flat_toml_table() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("settings.toml"),
        r#"
[shortcuts]
open_file = "ctrl+shift+p"
split_left = "ctrl+shift+left"
"#,
    )
    .unwrap();
    let store = SettingsStore::new(directory.path().to_path_buf());

    let loaded = store.load().unwrap().unwrap();

    assert_eq!(loaded.shortcuts.binding("open_file"), Some("ctrl+shift+p"));
    assert_eq!(
        loaded.shortcuts.binding("split_left"),
        Some("ctrl+shift+left")
    );
}
