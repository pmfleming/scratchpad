use eframe::egui;

pub(super) struct SettingsUi;

impl SettingsUi {
    pub(super) const PAGE_MAX_WIDTH: f32 = 980.0;
    pub(super) const BODY_FONT_SIZE: f32 = 15.0;
    pub(super) const TITLE_FONT_SIZE: f32 = 28.0;
    pub(super) const CATEGORY_FONT_SIZE: f32 = 20.0;
    pub(super) const DESCRIPTION_FONT_SIZE: f32 = 12.5;
    pub(super) const CARD_RADIUS: u8 = 10;
    pub(super) const CARD_MIN_HEIGHT: f32 = 72.0;
    pub(super) const INNER_ROW_HEIGHT: f32 = 56.0;
    pub(super) const CONTROL_WIDTH: f32 = 190.0;
    pub(super) const MODE_CONTROL_WIDTH_WITH_CUSTOM: f32 = 228.0;
    pub(super) const CONTROL_GAP: f32 = 8.0;
    pub(super) const ICON_BUTTON_SIZE: f32 = 34.0;
    pub(super) const BODY_TOP_SPACE: f32 = 24.0;
    pub(super) const BODY_BOTTOM_SPACE: f32 = 28.0;
    pub(super) const SECTION_GAP: f32 = 24.0;
    pub(super) const CARD_GAP: f32 = 8.0;
    pub(super) const PREVIEW_TOP_MARGIN: f32 = 14.0;
    pub(super) const CARD_INNER_MARGIN: egui::Margin = egui::Margin {
        left: 18,
        right: 18,
        top: 14,
        bottom: 14,
    };
    pub(super) const PREVIEW_INNER_MARGIN: egui::Margin = egui::Margin {
        left: 20,
        right: 20,
        top: 20,
        bottom: 20,
    };
    pub(super) const VALUE_PILL_INNER_MARGIN: egui::Margin = egui::Margin {
        left: 12,
        right: 12,
        top: 8,
        bottom: 8,
    };
    pub(super) const INFO_CHIP_INNER_MARGIN: egui::Margin = egui::Margin {
        left: 10,
        right: 10,
        top: 5,
        bottom: 5,
    };
    pub(super) const PREVIEW_TEXT: &'static str =
        "I hear the ruin of all space, shattered glass and toppling masonry, and time one livid final flame.";

    const CARD_BG_DARK: egui::Color32 = egui::Color32::from_rgb(42, 47, 57);
    const CARD_BORDER_DARK: egui::Color32 = egui::Color32::from_rgb(61, 67, 77);
    const CONTROL_BG_DARK: egui::Color32 = egui::Color32::from_rgb(58, 63, 71);
    const ACCENT: egui::Color32 = egui::Color32::from_rgb(42, 168, 242);
    const ICON_DARK: egui::Color32 =
        egui::Color32::from_rgba_premultiplied(242, 244, 247, 170);

    pub(super) fn apply_typography(ui: &mut egui::Ui) {
        let font_id = egui::FontId::proportional(Self::BODY_FONT_SIZE);
        let style = ui.style_mut();
        style.override_font_id = Some(font_id.clone());
        style
            .text_styles
            .insert(egui::TextStyle::Body, font_id.clone());
        style
            .text_styles
            .insert(egui::TextStyle::Button, font_id.clone());
        style.text_styles.insert(egui::TextStyle::Small, font_id);
    }

    pub(super) fn page_content_width(ui: &egui::Ui) -> f32 {
        ui.available_width().min(Self::PAGE_MAX_WIDTH)
    }

    pub(super) fn page_horizontal_margin(ui: &egui::Ui, content_width: f32) -> f32 {
        ((ui.available_width() - content_width) * 0.5).max(Self::BODY_TOP_SPACE)
    }

    pub(super) fn header_text_width(ui: &egui::Ui) -> f32 {
        (ui.available_width() - 240.0).max(220.0)
    }

    pub(super) fn row_label_width(ui: &egui::Ui) -> f32 {
        (ui.available_width() - 250.0).max(180.0)
    }

    pub(super) fn divider_width(ui: &egui::Ui) -> f32 {
        (ui.available_width() - 40.0).max(0.0)
    }

    pub(super) fn card_bg(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            Self::CARD_BG_DARK
        } else {
            egui::Color32::from_rgb(255, 255, 255)
        }
    }

    pub(super) fn card_border(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            Self::CARD_BORDER_DARK
        } else {
            egui::Color32::from_rgb(204, 213, 226)
        }
    }

    pub(super) fn control_bg(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            Self::CONTROL_BG_DARK
        } else {
            egui::Color32::from_rgb(238, 243, 249)
        }
    }

    pub(super) fn icon_color(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            Self::ICON_DARK
        } else {
            egui::Color32::from_rgba_premultiplied(28, 35, 45, 170)
        }
    }

    pub(super) fn accent() -> egui::Color32 {
        Self::ACCENT
    }

    pub(super) fn card_frame(ui: &egui::Ui) -> egui::Frame {
        egui::Frame::new()
            .fill(Self::card_bg(ui))
            .stroke(egui::Stroke::new(1.0, Self::card_border(ui)))
            .corner_radius(egui::CornerRadius::same(Self::CARD_RADIUS))
            .inner_margin(Self::CARD_INNER_MARGIN)
    }

    pub(super) fn preview_frame(ui: &egui::Ui, fill: egui::Color32) -> egui::Frame {
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, Self::card_border(ui)))
            .corner_radius(egui::CornerRadius::same(Self::CARD_RADIUS))
            .inner_margin(Self::PREVIEW_INNER_MARGIN)
    }

    pub(super) fn control_width(has_custom_palette: bool) -> f32 {
        if has_custom_palette {
            Self::MODE_CONTROL_WIDTH_WITH_CUSTOM
        } else {
            Self::CONTROL_WIDTH
        }
    }

    pub(super) fn pill_width() -> f32 {
        Self::CONTROL_WIDTH - Self::ICON_BUTTON_SIZE - Self::CONTROL_GAP
    }
}
