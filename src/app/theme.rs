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

pub fn header_bg(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        HEADER_BG
    } else {
        Color32::from_rgb(240, 243, 248)
    }
}

pub fn tab_active_bg(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        TAB_ACTIVE_BG
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}

pub fn tab_hover_bg(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        TAB_HOVER_BG
    } else {
        Color32::from_rgb(226, 232, 240)
    }
}

pub fn action_bg(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ACTION_BG
    } else {
        Color32::from_rgb(230, 236, 244)
    }
}

pub fn action_hover_bg(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ACTION_HOVER_BG
    } else {
        Color32::from_rgb(213, 222, 234)
    }
}

pub fn border(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        BORDER
    } else {
        Color32::from_rgb(184, 194, 208)
    }
}

pub fn text_primary(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        TEXT_PRIMARY
    } else {
        Color32::from_rgb(28, 35, 45)
    }
}

pub fn text_muted(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        TEXT_MUTED
    } else {
        Color32::from_rgba_premultiplied(28, 35, 45, 178)
    }
}

pub fn tab_active_bg_for_visuals(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        TAB_ACTIVE_BG
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}

pub fn border_for_visuals(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        BORDER
    } else {
        Color32::from_rgb(184, 194, 208)
    }
}

pub fn text_primary_for_visuals(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        TEXT_PRIMARY
    } else {
        Color32::from_rgb(28, 35, 45)
    }
}

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
