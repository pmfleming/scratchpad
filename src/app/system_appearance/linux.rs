use super::{
    MAX_FONT_SIZE, MIN_FONT_SIZE, SystemColorScheme, parse_font_family, parse_trailing_font_size,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) fn detect_font_size() -> Option<f32> {
    let settings = gtk_settings();
    settings
        .get("gtk-font-name")
        .and_then(|font_name| parse_trailing_font_size(font_name))
        .map(|size| size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
}

pub(super) fn detect_font_family() -> Option<String> {
    let settings = gtk_settings();
    settings
        .get("gtk-font-name")
        .and_then(|font_name| parse_font_family(font_name))
}

pub(super) fn detect_color_scheme() -> Option<SystemColorScheme> {
    if std::env::var("GTK_THEME")
        .ok()
        .is_some_and(|theme| theme.to_ascii_lowercase().contains("dark"))
    {
        return Some(SystemColorScheme::Dark);
    }

    let settings = gtk_settings();
    if is_dark_setting(settings.get("gtk-application-prefer-dark-theme"))
        || is_dark_theme_name(settings.get("gtk-theme-name"))
    {
        return Some(SystemColorScheme::Dark);
    }

    gsettings_color_scheme()
}

fn gtk_settings() -> HashMap<String, String> {
    let mut settings = HashMap::new();
    for path in gtk_settings_paths() {
        merge_gtk_settings_file(&path, &mut settings);
    }
    settings
}

fn gtk_settings_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        let config_home = PathBuf::from(config_home);
        paths.push(config_home.join("gtk-4.0/settings.ini"));
        paths.push(config_home.join("gtk-3.0/settings.ini"));
    } else if let Some(home) = std::env::var_os("HOME") {
        let config_home = PathBuf::from(home).join(".config");
        paths.push(config_home.join("gtk-4.0/settings.ini"));
        paths.push(config_home.join("gtk-3.0/settings.ini"));
    }
    paths.push(PathBuf::from("/etc/gtk-4.0/settings.ini"));
    paths.push(PathBuf::from("/etc/gtk-3.0/settings.ini"));
    paths
}

fn merge_gtk_settings_file(path: &Path, settings: &mut HashMap<String, String>) {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };

    for line in raw.lines().filter_map(parse_gtk_setting_line) {
        settings.entry(line.0).or_insert(line.1);
    }
}

fn parse_gtk_setting_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    Some((key.trim().to_owned(), value.trim().to_owned()))
}

fn is_dark_setting(value: Option<&String>) -> bool {
    value.is_some_and(|value| matches!(value.trim(), "1" | "true" | "True" | "TRUE"))
}

fn is_dark_theme_name(value: Option<&String>) -> bool {
    value.is_some_and(|theme| theme.to_ascii_lowercase().contains("dark"))
}

fn gsettings_color_scheme() -> Option<SystemColorScheme> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if value.contains("dark") {
        Some(SystemColorScheme::Dark)
    } else if value.contains("light") {
        Some(SystemColorScheme::Light)
    } else {
        None
    }
}
