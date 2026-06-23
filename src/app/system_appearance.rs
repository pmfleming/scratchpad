use eframe::egui;
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FALLBACK_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 72.0;
const SYSTEM_ACCENT_FALLBACK: egui::Color32 = egui::Color32::from_rgb(42, 168, 242);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SystemColorScheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemEditorPalette {
    pub text: egui::Color32,
    pub background: egui::Color32,
    pub highlight: egui::Color32,
    pub highlight_text: egui::Color32,
}

#[must_use]
pub(crate) fn editor_font_size() -> f32 {
    system_font_size().unwrap_or(FALLBACK_FONT_SIZE)
}

#[must_use]
pub(crate) fn editor_palette() -> SystemEditorPalette {
    let highlight = system_accent_color().unwrap_or(SYSTEM_ACCENT_FALLBACK);
    let highlight_text = crate::app::color_contrast::optimal_text_color(highlight);
    let (text, background) = match system_color_scheme().unwrap_or(SystemColorScheme::Dark) {
        SystemColorScheme::Light => (egui::Color32::BLACK, egui::Color32::WHITE),
        SystemColorScheme::Dark => (egui::Color32::WHITE, egui::Color32::from_rgb(21, 24, 29)),
    };

    SystemEditorPalette {
        text,
        background,
        highlight,
        highlight_text,
    }
}

fn system_font_size() -> Option<f32> {
    static FONT_SIZE: OnceLock<Option<f32>> = OnceLock::new();
    *FONT_SIZE.get_or_init(detect_system_font_size)
}

fn system_color_scheme() -> Option<SystemColorScheme> {
    static SCHEME: OnceLock<Option<SystemColorScheme>> = OnceLock::new();
    *SCHEME.get_or_init(detect_system_color_scheme)
}

fn system_accent_color() -> Option<egui::Color32> {
    static ACCENT: OnceLock<Option<egui::Color32>> = OnceLock::new();
    *ACCENT.get_or_init(detect_system_accent_color)
}

fn detect_system_font_size() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        detect_linux_font_size()
    }

    #[cfg(target_os = "windows")]
    {
        detect_windows_font_size()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_system_color_scheme() -> Option<SystemColorScheme> {
    #[cfg(target_os = "linux")]
    {
        detect_linux_color_scheme()
    }

    #[cfg(target_os = "windows")]
    {
        detect_windows_color_scheme()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_system_accent_color() -> Option<egui::Color32> {
    #[cfg(target_os = "windows")]
    {
        detect_windows_accent_color()
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_font_size() -> Option<f32> {
    let settings = linux_gtk_settings();
    settings
        .get("gtk-font-name")
        .and_then(|font_name| parse_trailing_font_size(font_name))
        .map(|size| size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
}

#[cfg(target_os = "linux")]
fn detect_linux_color_scheme() -> Option<SystemColorScheme> {
    if std::env::var("GTK_THEME")
        .ok()
        .is_some_and(|theme| theme.to_ascii_lowercase().contains("dark"))
    {
        return Some(SystemColorScheme::Dark);
    }

    let settings = linux_gtk_settings();
    if settings
        .get("gtk-application-prefer-dark-theme")
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "True" | "TRUE"))
    {
        return Some(SystemColorScheme::Dark);
    }

    if settings
        .get("gtk-theme-name")
        .is_some_and(|theme| theme.to_ascii_lowercase().contains("dark"))
    {
        return Some(SystemColorScheme::Dark);
    }

    gsettings_color_scheme()
}

#[cfg(target_os = "linux")]
fn linux_gtk_settings() -> HashMap<String, String> {
    let mut settings = HashMap::new();
    for path in gtk_settings_paths() {
        merge_gtk_settings_file(&path, &mut settings);
    }
    settings
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn merge_gtk_settings_file(path: &Path, settings: &mut HashMap<String, String>) {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        settings
            .entry(key.trim().to_owned())
            .or_insert_with(|| value.trim().to_owned());
    }
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "windows")]
fn detect_windows_font_size() -> Option<f32> {
    let raw = reg_query(r"HKCU\Control Panel\Desktop\WindowMetrics", "MessageFont")?;
    let bytes = parse_reg_binary_bytes(&raw)?;
    if bytes.len() < 4 {
        return None;
    }
    let height = i32::from_le_bytes(bytes[0..4].try_into().ok()?);
    if height >= 0 {
        return None;
    }

    let dpi = reg_query(r"HKCU\Control Panel\Desktop\WindowMetrics", "AppliedDPI")
        .and_then(|raw| parse_reg_dword(&raw))
        .unwrap_or(96) as f32;
    Some(((-height) as f32 * 72.0 / dpi).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
}

#[cfg(target_os = "windows")]
fn detect_windows_color_scheme() -> Option<SystemColorScheme> {
    let raw = reg_query(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )?;
    match parse_reg_dword(&raw)? {
        0 => Some(SystemColorScheme::Dark),
        _ => Some(SystemColorScheme::Light),
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_accent_color() -> Option<egui::Color32> {
    let raw = reg_query(r"HKCU\Software\Microsoft\Windows\DWM", "AccentColor")?;
    let value = parse_reg_dword(&raw)?;
    let r = (value & 0x0000_00ff) as u8;
    let g = ((value & 0x0000_ff00) >> 8) as u8;
    let b = ((value & 0x00ff_0000) >> 16) as u8;
    Some(egui::Color32::from_rgb(r, g, b))
}

#[cfg(target_os = "windows")]
fn reg_query(key: &str, value: &str) -> Option<String> {
    let output = std::process::Command::new("reg")
        .args(["query", key, "/v", value])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "windows")]
fn parse_reg_dword(raw: &str) -> Option<u32> {
    raw.split_whitespace().find_map(|part| {
        part.strip_prefix("0x")
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
    })
}

#[cfg(target_os = "windows")]
fn parse_reg_binary_bytes(raw: &str) -> Option<Vec<u8>> {
    let hex = raw
        .split_whitespace()
        .skip_while(|part| *part != "REG_BINARY")
        .skip(1)
        .collect::<String>();
    if hex.len() < 2 || hex.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks(2) {
        let chunk = std::str::from_utf8(chunk).ok()?;
        bytes.push(u8::from_str_radix(chunk, 16).ok()?);
    }
    Some(bytes)
}

fn parse_trailing_font_size(font_name: &str) -> Option<f32> {
    font_name
        .split_whitespace()
        .next_back()
        .and_then(|size| size.parse::<f32>().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_trailing_font_size;

    #[test]
    fn gtk_font_name_size_is_read_from_last_token() {
        assert_eq!(parse_trailing_font_size("Cantarell 11"), Some(11.0));
        assert_eq!(
            parse_trailing_font_size("Noto Sans Display 10.5"),
            Some(10.5)
        );
        assert_eq!(parse_trailing_font_size("NoSize"), None);
    }
}
