use crate::app::services::store_io::write_atomic;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod model;

pub use model::{
    AppSettings, AppThemeMode, DEFAULT_AUTO_HIDE_TAB_LIST, DEFAULT_EDITOR_BACKGROUND_COLOR,
    DEFAULT_EDITOR_GUTTER, DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, DEFAULT_FONT_SIZE, DEFAULT_RECENT_FILES_ENABLED,
    DEFAULT_STATUS_BAR_VISIBLE, DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS, DEFAULT_TAB_LIST_WIDTH,
    DEFAULT_WINDOW_INNER_SIZE, DEFAULT_WORD_WRAP, EditorSettings, FileOpenDisposition,
    HistorySettings, LEGACY_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR,
    LIGHT_EDITOR_TEXT_COLOR, MIN_WINDOW_INNER_SIZE, NewTabPlacement, StartupSessionBehavior,
    TabListPosition, TabOrderDirection, TabOrderMode, UiSettings, WindowState, WorkspaceSettings,
};
pub(crate) use model::{color_from_hex, color_to_hex, default_font_size, default_word_wrap};

const SETTINGS_FILE_NAME: &str = "settings.toml";
const LEGACY_SETTINGS_FILE_NAME: &str = "settings.yaml";

pub struct SettingsStore {
    root: PathBuf,
    settings_path: PathBuf,
    legacy_settings_path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("scratchpad"))
    }
}

impl SettingsStore {
    pub fn new(root: PathBuf) -> Self {
        let settings_path = root.join(SETTINGS_FILE_NAME);
        let legacy_settings_path = root.join(LEGACY_SETTINGS_FILE_NAME);
        Self {
            root,
            settings_path,
            legacy_settings_path,
        }
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

        Ok(None)
    }

    fn load_toml(&self) -> io::Result<Option<AppSettings>> {
        let raw = fs::read_to_string(&self.settings_path)?;
        let settings = parse_toml_settings(&raw)?;
        Ok(Some(settings))
    }

    fn load_legacy_yaml(&self) -> io::Result<AppSettings> {
        let raw = fs::read_to_string(&self.legacy_settings_path)?;
        serde_yaml::from_str(&raw).map_err(invalid_data)
    }

    pub fn save(&self, settings: &AppSettings) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let toml = toml::to_string_pretty(settings).map_err(invalid_data)?;
        write_atomic(&self.settings_path, toml.as_bytes())
    }

    pub fn path(&self) -> &Path {
        &self.settings_path
    }
}

pub(crate) fn parse_toml_settings(raw: &str) -> io::Result<AppSettings> {
    let value = raw.parse::<toml::Value>().map_err(invalid_data)?;
    if !value.is_table() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "settings TOML must contain a top-level table",
        ));
    }

    value.try_into().map_err(invalid_data)
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
