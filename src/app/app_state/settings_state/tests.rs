use super::*;
use crate::app::services::session_store::SessionStore;
use std::time::{Duration, Instant};

fn test_app() -> ScratchpadApp {
    let session_root = tempfile::tempdir().expect("create session dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    ScratchpadApp::with_session_store(session_store)
}

fn set_custom_palette(app: &mut ScratchpadApp) {
    app.set_editor_text_color(egui::Color32::from_rgb(12, 34, 56));
    app.set_editor_background_color(egui::Color32::from_rgb(210, 220, 230));
}

fn assert_light_palette(app: &ScratchpadApp) {
    assert_eq!(app.editor_text_color(), egui::Color32::BLACK);
    assert_eq!(app.editor_background_color(), egui::Color32::WHITE);
}

fn assert_dark_palette(app: &ScratchpadApp) {
    assert_eq!(app.editor_text_color(), egui::Color32::WHITE);
    assert_eq!(
        app.editor_background_color(),
        egui::Color32::from_rgb(21, 24, 29)
    );
}

fn assert_default_highlight_color(app: &ScratchpadApp) {
    assert_eq!(
        app.editor_text_highlight_color(),
        egui::Color32::from_rgb(255, 243, 109)
    );
}

#[test]
fn applying_light_mode_settings_migrates_stock_editor_palette() {
    let mut app = test_app();

    app.apply_settings(AppSettings {
        theme_mode: AppThemeMode::Light,
        editor_text_color: DEFAULT_EDITOR_TEXT_COLOR.to_owned(),
        editor_background_color: DEFAULT_EDITOR_BACKGROUND_COLOR.to_owned(),
        ..AppSettings::default()
    });

    assert_light_palette(&app);
    assert_default_highlight_color(&app);
}

#[test]
fn switching_to_light_mode_updates_stock_editor_palette() {
    let mut app = test_app();

    app.set_theme_mode(AppThemeMode::Light);

    assert_light_palette(&app);
}

#[test]
fn switching_theme_preserves_custom_editor_palette() {
    let mut app = test_app();

    let custom_text = egui::Color32::from_rgb(12, 34, 56);
    let custom_background = egui::Color32::from_rgb(210, 220, 230);
    app.set_editor_text_color(custom_text);
    app.set_editor_background_color(custom_background);

    app.set_theme_mode(AppThemeMode::Light);

    assert_eq!(app.editor_text_color(), custom_text);
    assert_eq!(app.editor_background_color(), custom_background);
}

#[test]
fn custom_palette_is_detected_after_editor_color_change() {
    let mut app = test_app();

    assert!(!app.has_custom_editor_palette());

    app.set_editor_text_color(egui::Color32::from_rgb(12, 34, 56));

    assert!(app.has_custom_editor_palette());
}

#[test]
fn applying_light_theme_preset_clears_custom_palette() {
    let mut app = test_app();
    set_custom_palette(&mut app);

    app.apply_theme_mode_preset(AppThemeMode::Light, Some(egui::Theme::Light));

    assert_eq!(app.theme_mode(), AppThemeMode::Light);
    assert!(!app.has_custom_editor_palette());
    assert_light_palette(&app);
}

#[test]
fn applying_system_theme_preset_clears_custom_palette() {
    let mut app = test_app();
    set_custom_palette(&mut app);

    app.apply_theme_mode_preset(AppThemeMode::System, Some(egui::Theme::Dark));

    assert_eq!(app.theme_mode(), AppThemeMode::System);
    assert!(!app.has_custom_editor_palette());
    assert_dark_palette(&app);
}

#[test]
fn changing_highlight_color_is_persisted_independently() {
    let mut app = test_app();
    let highlight = egui::Color32::from_rgb(255, 235, 59);

    app.set_editor_text_highlight_color(highlight);
    app.set_theme_mode(AppThemeMode::Light);

    assert_eq!(app.editor_text_highlight_color(), highlight);
}

#[test]
fn applying_settings_clamps_auto_hide_delay_seconds() {
    let mut app = test_app();

    app.apply_settings(AppSettings {
        tab_list_auto_hide_delay_seconds: 99.0,
        ..AppSettings::default()
    });

    assert_eq!(
        app.tab_list_auto_hide_delay_seconds(),
        ScratchpadApp::TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS
    );
}

#[test]
fn delay_tab_list_hide_uses_configured_grace_period() {
    let mut app = test_app();
    let now = Instant::now();

    app.set_tab_list_auto_hide_delay_seconds(4.5);
    app.delay_tab_list_hide(now);

    assert!(app.vertical_tab_list_open);
    assert_eq!(
        app.vertical_tab_list_hide_deadline,
        Some(now + Duration::from_secs_f32(4.5))
    );
}

#[test]
fn changing_tab_list_position_starts_a_short_chrome_transition() {
    let mut app = test_app();

    app.set_tab_list_position(TabListPosition::Left);

    assert!(app.chrome_transition_active());
}

#[test]
fn changing_tab_list_width_starts_a_short_chrome_transition() {
    let mut app = test_app();

    app.set_tab_list_width_from_layout(180.0);

    assert!(app.chrome_transition_active());
}
