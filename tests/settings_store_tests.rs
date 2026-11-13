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

    fn write_toml(&self, contents: &str) -> AppSettings {
        self.write("settings.toml", contents);
        self.load()
    }

    fn load(&self) -> AppSettings {
        self.store.load().unwrap().unwrap()
    }

    fn round_trip(&self, settings: &AppSettings) -> AppSettings {
        self.store.save(settings).unwrap();
        self.load()
    }

    fn saved_table(&self) -> toml::Table {
        let raw = std::fs::read_to_string(self.directory.path().join("settings.toml")).unwrap();
        raw.parse().unwrap()
    }

    fn path_exists(&self, name: &str) -> bool {
        self.directory.path().join(name).exists()
    }
}

#[test]
fn loading_without_a_usable_toml_file_reports_none_or_invalid_data() {
    let fixture = SettingsFixture::new();
    assert!(fixture.store.load().unwrap().is_none());

    // Legacy YAML is ignored rather than migrated.
    fixture.write("settings.yaml", "editor:\n  font_size: 15.0\n");
    assert!(fixture.store.load().unwrap().is_none());
    assert!(!fixture.path_exists("settings.toml"));

    fixture.write("settings.toml", "font_size = [");
    let error = fixture.store.load().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn documented_settings_example_stays_loadable() {
    let fixture = SettingsFixture::new();
    let settings = fixture.write_toml(include_str!("../examples/settings.toml"));

    assert_eq!(settings.platform.profile, PlatformProfile::Auto);
    assert_eq!(settings.editor.editor_font, EditorFontPreset::Standard);
    assert_eq!(settings.shortcuts.binding("open_user_manual"), Some("f1"));
}

#[test]
fn save_and_load_round_trip_toml_settings() {
    let fixture = SettingsFixture::new();
    let mut settings = AppSettings::default();
    settings.editor.font_size = 18.0;
    settings.editor.word_wrap = false;
    settings.editor.editor_font = EditorFontPreset::Mono;
    settings.editor.indentation_style = IndentationStyle::Spaces;
    settings.editor.tab_display = TabDisplayMode::Tablines;
    settings.workspace.tab_list_position = TabListPosition::Left;
    settings.workspace.file_open_disposition = FileOpenDisposition::CurrentTab;
    settings.workspace.startup_session_behavior = StartupSessionBehavior::StartFreshSession;
    settings.platform.profile = PlatformProfile::Hyprland;
    settings
        .shortcuts
        .bindings
        .insert("open_file".to_owned(), "ctrl+shift+p".to_owned());

    let loaded = fixture.round_trip(&settings);

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
    assert_eq!(loaded.platform.profile, PlatformProfile::Hyprland);
    assert_eq!(loaded.shortcuts.binding("open_file"), Some("ctrl+shift+p"));
}

#[test]
fn save_omits_ephemeral_tab_order_and_window_geometry() {
    let fixture = SettingsFixture::new();
    let mut settings = AppSettings::default();
    settings.workspace.custom_tab_order = vec![10, 20, 30];
    settings.ui.window_state = WindowState {
        position: Some([40.0, 60.0]),
        inner_size: Some([1024.0, 768.0]),
        maximized: true,
    };

    fixture.store.save(&settings).unwrap();

    let table = fixture.saved_table();
    let section = |name: &str| table.get(name).and_then(toml::Value::as_table);
    let window_state = section("ui")
        .and_then(|ui| ui.get("window_state"))
        .and_then(toml::Value::as_table)
        .unwrap();
    assert!(!table.contains_key("custom_tab_order"));
    assert!(
        !section("workspace").is_some_and(|workspace| workspace.contains_key("custom_tab_order"))
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
    let fixture = SettingsFixture::new();
    let loaded = fixture.write_toml(
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
    );

    assert!(loaded.workspace.custom_tab_order.is_empty());
    assert_eq!(loaded.ui.window_state.position, None);
    assert_eq!(loaded.ui.window_state.inner_size, None);
    assert!(loaded.ui.window_state.maximized);
}

#[test]
fn older_toml_missing_newer_fields_uses_defaults() {
    let fixture = SettingsFixture::new();
    let settings = fixture.write_toml(
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
fn legacy_show_tab_characters_boolean_maps_to_character_mode() {
    let fixture = SettingsFixture::new();
    let settings = fixture.write_toml("[editor]\nshow_tab_characters = true\n");

    assert_eq!(settings.editor.tab_display, TabDisplayMode::Character);
}

#[test]
fn shortcut_overrides_load_from_flat_toml_table() {
    let fixture = SettingsFixture::new();
    let loaded = fixture.write_toml(
        r#"
[shortcuts]
open_file = "ctrl+shift+p"
split_left = "ctrl+shift+left"
"#,
    );

    assert_eq!(loaded.shortcuts.binding("open_file"), Some("ctrl+shift+p"));
    assert_eq!(
        loaded.shortcuts.binding("split_left"),
        Some("ctrl+shift+left")
    );
}
