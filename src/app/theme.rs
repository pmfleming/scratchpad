use eframe::egui::{self, Color32, Vec2};

pub const HEADER_BG: Color32 = Color32::from_rgb(24, 27, 33);
pub const TAB_ACTIVE_BG: Color32 = Color32::from_rgb(48, 54, 64);
pub const TAB_HOVER_BG: Color32 = Color32::from_rgb(43, 49, 58);
pub const ACTION_BG: Color32 = Color32::from_rgb(38, 43, 50);
pub const ACTION_HOVER_BG: Color32 = Color32::from_rgb(52, 58, 68);
pub const EDITOR_BG: Color32 = Color32::from_rgb(21, 24, 29);
pub const CLOSE_BG: Color32 = Color32::from_rgb(124, 49, 49);
pub const CLOSE_HOVER_BG: Color32 = Color32::from_rgb(164, 58, 58);
pub const BORDER: Color32 = Color32::from_rgb(59, 66, 76);
pub const TEXT_PRIMARY: Color32 = Color32::WHITE;
pub const TEXT_MUTED: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 160);
const HEADER_BG_LIGHT: Color32 = Color32::from_rgb(240, 243, 248);
const TAB_ACTIVE_BG_LIGHT: Color32 = Color32::from_rgb(255, 255, 255);
const TAB_HOVER_BG_LIGHT: Color32 = Color32::from_rgb(226, 232, 240);
const ACTION_BG_LIGHT: Color32 = Color32::from_rgb(230, 236, 244);
const ACTION_HOVER_BG_LIGHT: Color32 = Color32::from_rgb(213, 222, 234);
const BORDER_LIGHT: Color32 = Color32::from_rgb(184, 194, 208);
const TEXT_PRIMARY_LIGHT: Color32 = Color32::from_rgb(28, 35, 45);
const TEXT_MUTED_LIGHT: Color32 = Color32::from_rgba_premultiplied(28, 35, 45, 178);

fn theme_color(dark_mode: bool, dark: Color32, light: Color32) -> Color32 {
    if dark_mode {
        dark
    } else {
        light
    }
}

fn ui_theme_color(ui: &egui::Ui, dark: Color32, light: Color32) -> Color32 {
    theme_color(ui.visuals().dark_mode, dark, light)
}

macro_rules! ui_theme_fn {
    ($name:ident, $dark:ident, $light:ident) => {
        pub fn $name(ui: &egui::Ui) -> Color32 {
            ui_theme_color(ui, $dark, $light)
        }
    };
}

macro_rules! visuals_theme_fn {
    ($name:ident, $dark:ident, $light:ident) => {
        pub fn $name(visuals: &egui::Visuals) -> Color32 {
            theme_color(visuals.dark_mode, $dark, $light)
        }
    };
}

ui_theme_fn!(header_bg, HEADER_BG, HEADER_BG_LIGHT);
ui_theme_fn!(tab_active_bg, TAB_ACTIVE_BG, TAB_ACTIVE_BG_LIGHT);
ui_theme_fn!(tab_hover_bg, TAB_HOVER_BG, TAB_HOVER_BG_LIGHT);
ui_theme_fn!(action_bg, ACTION_BG, ACTION_BG_LIGHT);
ui_theme_fn!(action_hover_bg, ACTION_HOVER_BG, ACTION_HOVER_BG_LIGHT);
ui_theme_fn!(border, BORDER, BORDER_LIGHT);
ui_theme_fn!(text_primary, TEXT_PRIMARY, TEXT_PRIMARY_LIGHT);
ui_theme_fn!(text_muted, TEXT_MUTED, TEXT_MUTED_LIGHT);

visuals_theme_fn!(
    tab_active_bg_for_visuals,
    TAB_ACTIVE_BG,
    TAB_ACTIVE_BG_LIGHT
);
visuals_theme_fn!(border_for_visuals, BORDER, BORDER_LIGHT);
visuals_theme_fn!(text_primary_for_visuals, TEXT_PRIMARY, TEXT_PRIMARY_LIGHT);

pub const HEADER_CONTROL_HEIGHT: f32 = 30.0;
pub const HEADER_VERTICAL_PADDING: f32 = 2.0;
pub const BUTTON_SIZE: Vec2 = Vec2::new(30.0, HEADER_CONTROL_HEIGHT);
pub const CAPTION_BUTTON_SIZE: Vec2 = Vec2::new(36.0, HEADER_CONTROL_HEIGHT);
pub const CAPTION_BUTTON_SPACING: f32 = 0.0;
pub const CAPTION_TRAILING_PADDING: f32 = 0.0;
pub const HEADER_LEFT_PADDING: f32 = 8.0;
pub const HEADER_RIGHT_PADDING: f32 = 0.0;
pub const HEADER_HEIGHT: f32 = HEADER_CONTROL_HEIGHT + HEADER_VERTICAL_PADDING * 2.0;
pub const TAB_HEIGHT: f32 = HEADER_CONTROL_HEIGHT;
pub const TAB_BUTTON_WIDTH: f32 = 140.0;
