use eframe::egui;

pub(super) struct SettingsTypography {
    pub description: f32,
}
pub(super) struct SettingsLayout {
    pub card_max_width: f32,
    pub section_gap: f32,
}
pub(super) struct ControlMetrics {
    pub column_width: f32,
    pub gap: f32,
}
pub(super) struct SettingsUi;

impl SettingsUi {
    pub(super) const TYPOGRAPHY: SettingsTypography = SettingsTypography { description: 12.5 };
    pub(super) const LAYOUT: SettingsLayout = SettingsLayout {
        card_max_width: 920.0,
        section_gap: 18.0,
    };
    pub(super) const CONTROLS: ControlMetrics = ControlMetrics {
        column_width: 300.0,
        gap: 24.0,
    };

    pub(super) fn apply_typography(ui: &mut egui::Ui) {
        let style = ui.style_mut();
        let font = egui::FontId::proportional(15.0);
        style.override_font_id = Some(font.clone());
        for text in [
            egui::TextStyle::Body,
            egui::TextStyle::Button,
            egui::TextStyle::Small,
        ] {
            style.text_styles.insert(text, font.clone());
        }
    }
    pub(super) fn card_bg(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            egui::Color32::from_rgb(25, 29, 32)
        } else {
            egui::Color32::from_rgb(250, 251, 252)
        }
    }
    pub(super) fn card_border(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            egui::Color32::from_rgb(50, 58, 64)
        } else {
            egui::Color32::from_rgb(204, 213, 226)
        }
    }
    pub(super) fn control_bg(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            egui::Color32::from_rgb(41, 48, 54)
        } else {
            egui::Color32::from_rgb(230, 236, 240)
        }
    }
    pub(super) fn icon_color(ui: &egui::Ui) -> egui::Color32 {
        crate::app::theme::text_muted(ui)
    }
    pub(super) fn accent() -> egui::Color32 {
        egui::Color32::from_rgb(111, 177, 149)
    }
    pub(super) fn card_frame(ui: &egui::Ui) -> egui::Frame {
        egui::Frame::new()
            .fill(Self::card_bg(ui))
            .stroke(egui::Stroke::new(1.0, Self::card_border(ui)))
            .corner_radius(10)
            .inner_margin(egui::Margin {
                left: 18,
                right: 18,
                top: 10,
                bottom: 12,
            })
    }
}
