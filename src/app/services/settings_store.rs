use crate::app::services::store_io::write_atomic;
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod model;

pub use model::{
    AppSettings, AppThemeMode, DEFAULT_AUTO_HIDE_TAB_LIST, DEFAULT_EDITOR_BACKGROUND_COLOR,
    DEFAULT_EDITOR_GUTTER, DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, DEFAULT_FONT_SIZE, DEFAULT_RECENT_FILES_ENABLED,
    DEFAULT_STATUS_BAR_VISIBLE, DEFAULT_TAB_DISPLAY, DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS,
    DEFAULT_TAB_LIST_WIDTH, DEFAULT_WINDOW_INNER_SIZE, DEFAULT_WORD_WRAP, EditorAppearanceSource,
    EditorSettings, FileOpenDisposition, HistorySettings, IndentationStyle,
    LEGACY_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR,
    LIGHT_EDITOR_TEXT_COLOR, MIN_WINDOW_INNER_SIZE, NewTabPlacement, PlatformSettings,
    ShortcutSettings, StartupSessionBehavior, TabDisplayMode, TabListPosition, TabOrderDirection,
    TabOrderMode, UiSettings, WindowState, WorkspaceSettings,
};
pub(crate) use model::{color_from_hex, color_to_hex, default_font_size, default_word_wrap};

const SETTINGS_FILE_NAME: &str = "settings.toml";
const LEGACY_SETTINGS_FILE_NAME: &str = "settings.yaml";

pub struct SettingsStore {
    root: PathBuf,
    settings_path: PathBuf,
    legacy_settings_path: PathBuf,
    fallback_root: Option<PathBuf>,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("scratchpad"))
    }
}

impl SettingsStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let settings_path = root.join(SETTINGS_FILE_NAME);
        let legacy_settings_path = root.join(LEGACY_SETTINGS_FILE_NAME);
        Self {
            root,
            settings_path,
            legacy_settings_path,
            fallback_root: None,
        }
    }

    #[must_use]
    pub fn with_fallback(root: PathBuf, fallback_root: PathBuf) -> Self {
        let mut store = Self::new(root);
        store.fallback_root = Some(fallback_root);
        store
    }

    pub fn load(&self) -> io::Result<Option<AppSettings>> {
        if self.settings_path.exists() {
            return self.load_toml();
        }

        if self.legacy_settings_path.exists() {
            let settings = self.load_legacy_yaml()?;
            self.save(&settings)?;
            return Ok(Some(settings));
        }

        if let Some(settings) = self.load_fallback_settings()? {
            self.save(&settings)?;
            return Ok(Some(settings));
        }

        Ok(None)
    }

    fn load_toml(&self) -> io::Result<Option<AppSettings>> {
        let raw = fs::read_to_string(&self.settings_path)?;
        let settings = parse_toml_settings(&raw)?;
        Ok(Some(settings))
    }

    fn load_legacy_yaml(&self) -> io::Result<AppSettings> {
        Self::load_yaml_path(&self.legacy_settings_path)
    }

    fn load_fallback_settings(&self) -> io::Result<Option<AppSettings>> {
        let Some(fallback_root) = &self.fallback_root else {
            return Ok(None);
        };

        let fallback_settings_path = fallback_root.join(SETTINGS_FILE_NAME);
        if fallback_settings_path.exists() {
            let raw = fs::read_to_string(fallback_settings_path)?;
            return parse_toml_settings(&raw).map(Some);
        }

        let fallback_legacy_settings_path = fallback_root.join(LEGACY_SETTINGS_FILE_NAME);
        if fallback_legacy_settings_path.exists() {
            return Self::load_yaml_path(&fallback_legacy_settings_path).map(Some);
        }

        Ok(None)
    }

    fn load_yaml_path(path: &Path) -> io::Result<AppSettings> {
        let raw = fs::read_to_string(path)?;
        serde_yaml::from_str(&raw).map_err(invalid_data)
    }

    pub fn save(&self, settings: &AppSettings) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let toml = toml::to_string_pretty(settings).map_err(invalid_data)?;
        write_atomic(&self.settings_path, toml.as_bytes())
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.settings_path
    }
}

pub(crate) fn parse_toml_settings(raw: &str) -> io::Result<AppSettings> {
    let table = raw.parse::<toml::Table>().map_err(invalid_data)?;

    let mut settings = AppSettings::default();
    apply_section(&table, "editor", &mut settings.editor)?;
    apply_section(&table, "workspace", &mut settings.workspace)?;
    apply_section(&table, "ui", &mut settings.ui)?;
    apply_section(&table, "history", &mut settings.history)?;
    apply_section(&table, "platform", &mut settings.platform)?;
    apply_section(&table, "shortcuts", &mut settings.shortcuts)?;
    Ok(settings)
}

fn apply_section<T>(table: &toml::Table, key: &str, target: &mut T) -> io::Result<()>
where
    T: DeserializeOwned,
{
    if let Some(section) = table.get(key) {
        *target = section.clone().try_into().map_err(invalid_data)?;
    }
    Ok(())
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::SettingsStore;
    use crate::app::platform::PlatformProfile;
    use std::fs;

    #[test]
    fn fallback_settings_are_loaded_and_migrated_to_primary_root() {
        let primary = tempfile::tempdir().expect("create primary settings root");
        let fallback = tempfile::tempdir().expect("create fallback settings root");
        fs::write(
            fallback.path().join("settings.toml"),
            "[platform]\nprofile = \"hyprland\"\n",
        )
        .expect("write fallback settings");

        let store = SettingsStore::with_fallback(
            primary.path().to_path_buf(),
            fallback.path().to_path_buf(),
        );
        let settings = store
            .load()
            .expect("load settings")
            .expect("fallback settings should exist");

        assert_eq!(settings.platform.profile, PlatformProfile::Hyprland);
        assert!(primary.path().join("settings.toml").exists());
    }
}
