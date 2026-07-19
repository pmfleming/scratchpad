use eframe::egui;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{
    OnceLock,
    atomic::{AtomicU8, Ordering},
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

const FALLBACK_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 72.0;
const SYSTEM_ACCENT_FALLBACK: egui::Color32 = egui::Color32::from_rgb(42, 168, 242);
const OBSERVED_THEME_UNKNOWN: u8 = 0;
const OBSERVED_THEME_LIGHT: u8 = 1;
const OBSERVED_THEME_DARK: u8 = 2;

static OBSERVED_SYSTEM_THEME: AtomicU8 = AtomicU8::new(OBSERVED_THEME_UNKNOWN);

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HyprlandBorderStyle {
    pub active: egui::Color32,
    pub inactive: egui::Color32,
    pub width: f32,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct SystemAppearanceBridge {
    #[serde(default)]
    font: BridgeFont,
    #[serde(default)]
    palette: BridgePalette,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct BridgeFont {
    family: Option<String>,
    size: Option<f32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct BridgePalette {
    color_scheme: Option<String>,
    text: Option<String>,
    background: Option<String>,
    accent: Option<String>,
    highlight: Option<String>,
    highlight_text: Option<String>,
}

#[must_use]
pub(crate) fn editor_font_size() -> f32 {
    system_font_size().unwrap_or(FALLBACK_FONT_SIZE)
}

#[must_use]
pub(crate) fn editor_font_family() -> Option<String> {
    system_font_family()
}

#[must_use]
pub(crate) fn theme_preference() -> egui::ThemePreference {
    theme_preference_for_bridge_scheme(bridge_color_scheme())
}

fn theme_preference_for_bridge_scheme(
    bridge_scheme: Option<SystemColorScheme>,
) -> egui::ThemePreference {
    match bridge_scheme {
        Some(SystemColorScheme::Light) => egui::ThemePreference::Light,
        Some(SystemColorScheme::Dark) => egui::ThemePreference::Dark,
        None => egui::ThemePreference::System,
    }
}

pub(crate) fn observe_system_theme(theme: Option<egui::Theme>) {
    let observed = match theme {
        Some(egui::Theme::Light) => OBSERVED_THEME_LIGHT,
        Some(egui::Theme::Dark) => OBSERVED_THEME_DARK,
        None => OBSERVED_THEME_UNKNOWN,
    };
    OBSERVED_SYSTEM_THEME.store(observed, Ordering::Relaxed);
}

#[must_use]
pub(crate) fn editor_palette() -> SystemEditorPalette {
    if let Some(palette) = bridge_editor_palette() {
        return palette;
    }

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

#[must_use]
pub(crate) fn hyprland_border_style() -> Option<HyprlandBorderStyle> {
    #[cfg(target_os = "linux")]
    {
        static STYLE: OnceLock<Option<HyprlandBorderStyle>> = OnceLock::new();
        *STYLE.get_or_init(linux::detect_hyprland_border_style)
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn system_font_size() -> Option<f32> {
    static FONT_SIZE: OnceLock<Option<f32>> = OnceLock::new();
    *FONT_SIZE.get_or_init(detect_system_font_size)
}

fn system_font_family() -> Option<String> {
    static FONT_FAMILY: OnceLock<Option<String>> = OnceLock::new();
    FONT_FAMILY.get_or_init(detect_system_font_family).clone()
}

fn system_color_scheme() -> Option<SystemColorScheme> {
    bridge_color_scheme()
        .or_else(observed_system_color_scheme)
        .or_else(detected_system_color_scheme)
}

fn system_accent_color() -> Option<egui::Color32> {
    static ACCENT: OnceLock<Option<egui::Color32>> = OnceLock::new();
    *ACCENT.get_or_init(detect_system_accent_color)
}

fn system_appearance_bridge() -> Option<&'static SystemAppearanceBridge> {
    static BRIDGE: OnceLock<Option<SystemAppearanceBridge>> = OnceLock::new();
    BRIDGE.get_or_init(load_system_appearance_bridge).as_ref()
}

fn load_system_appearance_bridge() -> Option<SystemAppearanceBridge> {
    for path in system_appearance_bridge_paths() {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Ok(bridge) = toml::from_str::<SystemAppearanceBridge>(&raw) {
            return Some(bridge);
        }
    }
    None
}

fn system_appearance_bridge_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("SCRATCHPAD_SYSTEM_APPEARANCE_FILE")
        && !path.as_os_str().is_empty()
    {
        paths.push(PathBuf::from(path));
    }

    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        paths.push(
            PathBuf::from(config_home)
                .join("scratchpad")
                .join("system-appearance.toml"),
        );
    } else if let Some(home) = std::env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join(".config")
                .join("scratchpad")
                .join("system-appearance.toml"),
        );
    }

    paths.push(PathBuf::from("/etc/scratchpad/system-appearance.toml"));
    paths
}

fn bridge_editor_palette() -> Option<SystemEditorPalette> {
    let palette = &system_appearance_bridge()?.palette;
    let text = parse_hex_color(palette.text.as_deref()?)?;
    let background = parse_hex_color(palette.background.as_deref()?)?;
    let highlight = palette
        .highlight
        .as_deref()
        .or(palette.accent.as_deref())
        .and_then(parse_hex_color)
        .unwrap_or(SYSTEM_ACCENT_FALLBACK);
    let highlight_text = palette
        .highlight_text
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or_else(|| crate::app::color_contrast::optimal_text_color(highlight));

    Some(SystemEditorPalette {
        text,
        background,
        highlight,
        highlight_text,
    })
}

fn bridge_color_scheme() -> Option<SystemColorScheme> {
    system_appearance_bridge()
        .and_then(|bridge| bridge.palette.color_scheme.as_deref())
        .and_then(parse_color_scheme)
}

fn observed_system_color_scheme() -> Option<SystemColorScheme> {
    match OBSERVED_SYSTEM_THEME.load(Ordering::Relaxed) {
        OBSERVED_THEME_LIGHT => Some(SystemColorScheme::Light),
        OBSERVED_THEME_DARK => Some(SystemColorScheme::Dark),
        _ => None,
    }
}

fn detected_system_color_scheme() -> Option<SystemColorScheme> {
    static SCHEME: OnceLock<Option<SystemColorScheme>> = OnceLock::new();
    *SCHEME.get_or_init(detect_system_color_scheme)
}

fn detect_system_font_size() -> Option<f32> {
    if let Some(size) = system_appearance_bridge()
        .and_then(|bridge| bridge.font.size)
        .filter(|size| size.is_finite())
        .map(|size| size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
    {
        return Some(size);
    }

    #[cfg(target_os = "linux")]
    {
        linux::detect_font_size()
    }

    #[cfg(target_os = "windows")]
    {
        windows::detect_font_size()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_system_font_family() -> Option<String> {
    if let Some(family) = system_appearance_bridge()
        .and_then(|bridge| bridge.font.family.as_deref())
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .map(str::to_owned)
    {
        return Some(family);
    }

    #[cfg(target_os = "linux")]
    {
        linux::detect_font_family()
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn detect_system_color_scheme() -> Option<SystemColorScheme> {
    #[cfg(target_os = "linux")]
    {
        linux::detect_color_scheme()
    }

    #[cfg(target_os = "windows")]
    {
        windows::detect_color_scheme()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_system_accent_color() -> Option<egui::Color32> {
    if let Some(accent) = system_appearance_bridge().and_then(|bridge| {
        bridge
            .palette
            .highlight
            .as_deref()
            .or(bridge.palette.accent.as_deref())
            .and_then(parse_hex_color)
    }) {
        return Some(accent);
    }

    #[cfg(target_os = "windows")]
    {
        windows::detect_accent_color()
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(any(target_os = "linux", test))]
fn parse_trailing_font_size(font_name: &str) -> Option<f32> {
    font_name
        .split_whitespace()
        .next_back()
        .and_then(|size| size.parse::<f32>().ok())
}

#[cfg(any(target_os = "linux", test))]
fn parse_font_family(font_name: &str) -> Option<String> {
    let trimmed = font_name.trim();
    let Some((family, _size)) = trimmed.rsplit_once(char::is_whitespace) else {
        return (!trimmed.is_empty()).then(|| trimmed.to_owned());
    };

    if parse_trailing_font_size(trimmed).is_some() {
        let family = family.trim();
        (!family.is_empty()).then(|| family.to_owned())
    } else {
        Some(trimmed.to_owned())
    }
}

fn parse_color_scheme(value: &str) -> Option<SystemColorScheme> {
    match value.trim().to_ascii_lowercase().as_str() {
        "dark" | "prefer-dark" => Some(SystemColorScheme::Dark),
        "light" | "prefer-light" => Some(SystemColorScheme::Light),
        _ => None,
    }
}

fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&trimmed[0..2], 16).ok()?;
    let g = u8::from_str_radix(&trimmed[2..4], 16).ok()?;
    let b = u8::from_str_radix(&trimmed[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::{
        SystemColorScheme, parse_color_scheme, parse_font_family, parse_hex_color,
        parse_trailing_font_size, theme_preference_for_bridge_scheme,
    };

    #[test]
    fn gtk_font_name_size_is_read_from_last_token() {
        assert_eq!(parse_trailing_font_size("Cantarell 11"), Some(11.0));
        assert_eq!(
            parse_trailing_font_size("Noto Sans Display 10.5"),
            Some(10.5)
        );
        assert_eq!(parse_trailing_font_size("NoSize"), None);
    }

    #[test]
    fn gtk_font_name_family_removes_trailing_size() {
        assert_eq!(
            parse_font_family("JetBrainsMono Nerd Font 12").as_deref(),
            Some("JetBrainsMono Nerd Font")
        );
        assert_eq!(parse_font_family("NoSize").as_deref(), Some("NoSize"));
    }

    #[test]
    fn bridge_color_scheme_parses_theme_values() {
        assert_eq!(parse_color_scheme("dark"), Some(SystemColorScheme::Dark));
        assert_eq!(
            parse_color_scheme("prefer-light"),
            Some(SystemColorScheme::Light)
        );
        assert_eq!(parse_color_scheme("system"), None);
    }

    #[test]
    fn native_system_theme_remains_live_without_a_bridge_override() {
        assert_eq!(
            theme_preference_for_bridge_scheme(None),
            eframe::egui::ThemePreference::System
        );
        assert_eq!(
            theme_preference_for_bridge_scheme(Some(SystemColorScheme::Dark)),
            eframe::egui::ThemePreference::Dark
        );
    }

    #[test]
    fn bridge_hex_color_parses_hash_prefixed_rgb() {
        assert_eq!(
            parse_hex_color("#101418"),
            Some(eframe::egui::Color32::from_rgb(16, 20, 24))
        );
        assert_eq!(parse_hex_color("#bad"), None);
    }
}
