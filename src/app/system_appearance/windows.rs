use super::{MAX_FONT_SIZE, MIN_FONT_SIZE, SystemColorScheme};
use eframe::egui;

pub(super) fn detect_font_size() -> Option<f32> {
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

pub(super) fn detect_color_scheme() -> Option<SystemColorScheme> {
    let raw = reg_query(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )?;
    match parse_reg_dword(&raw)? {
        0 => Some(SystemColorScheme::Dark),
        _ => Some(SystemColorScheme::Light),
    }
}

pub(super) fn detect_accent_color() -> Option<egui::Color32> {
    let raw = reg_query(r"HKCU\Software\Microsoft\Windows\DWM", "AccentColor")?;
    let value = parse_reg_dword(&raw)?;
    let r = (value & 0x0000_00ff) as u8;
    let g = ((value & 0x0000_ff00) >> 8) as u8;
    let b = ((value & 0x00ff_0000) >> 16) as u8;
    Some(egui::Color32::from_rgb(r, g, b))
}

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

fn parse_reg_dword(raw: &str) -> Option<u32> {
    raw.split_whitespace().find_map(|part| {
        part.strip_prefix("0x")
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
    })
}

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
