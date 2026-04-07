use eframe::egui::{Color32, Vec2};

pub const HEADER_BG: Color32 = Color32::from_rgb(24, 27, 33);
pub const TAB_ACTIVE_BG: Color32 = Color32::from_rgb(48, 54, 64);
pub const TAB_HOVER_BG: Color32 = Color32::from_rgb(43, 49, 58);
pub const ACTION_BG: Color32 = Color32::from_rgb(38, 43, 50);
pub const ACTION_HOVER_BG: Color32 = Color32::from_rgb(52, 58, 68);
pub const CLOSE_BG: Color32 = Color32::from_rgb(124, 49, 49);
pub const CLOSE_HOVER_BG: Color32 = Color32::from_rgb(164, 58, 58);
pub const BORDER: Color32 = Color32::from_rgb(59, 66, 76);
pub const TEXT_PRIMARY: Color32 = Color32::WHITE;

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
