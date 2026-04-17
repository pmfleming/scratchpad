use eframe::egui;

pub(super) struct SettingsTypography {
    pub body: f32,
    pub title: f32,
    pub category: f32,
    pub description: f32,
}

pub(super) struct SettingsLayout {
    pub page_max_width: f32,
    pub page_min_viewport_width: f32,
    pub page_min_viewport_height: f32,
    pub page_side_padding: f32,
    pub card_max_width: f32,
    pub preview_max_width: f32,
    pub card_radius: u8,
    pub card_min_height: f32,
    pub inner_row_height: f32,
    pub body_top_space: f32,
    pub body_bottom_space: f32,
    pub section_gap: f32,
    pub card_gap: f32,
    pub preview_top_margin: f32,
}

pub(super) struct ControlMetrics {
    pub width: f32,
    pub gap: f32,
    pub icon_button_size: f32,
}

pub(super) struct SettingsMargins {
    pub card_inner: egui::Margin,
    pub preview_inner: egui::Margin,
    pub value_pill_inner: egui::Margin,
    pub info_chip_inner: egui::Margin,
}

pub(super) struct SettingsUi;

impl SettingsUi {
    pub(super) const TYPOGRAPHY: SettingsTypography = SettingsTypography {
        body: 15.0,
        title: 28.0,
        category: 20.0,
        description: 12.5,
    };
    pub(super) const LAYOUT: SettingsLayout = SettingsLayout {
        page_max_width: 980.0,
        page_min_viewport_width: 1180.0,
        page_min_viewport_height: 720.0,
        page_side_padding: 24.0,
        card_max_width: 760.0,
        preview_max_width: 420.0,
        card_radius: 10,
        card_min_height: 72.0,
        inner_row_height: 56.0,
        body_top_space: 24.0,
        body_bottom_space: 28.0,
        section_gap: 24.0,
        card_gap: 8.0,
        preview_top_margin: 14.0,
    };
    pub(super) const CONTROLS: ControlMetrics = ControlMetrics {
        width: 190.0,
        gap: 8.0,
        icon_button_size: 34.0,
    };
    pub(super) const MARGINS: SettingsMargins = SettingsMargins {
        card_inner: egui::Margin {
            left: 18,
            right: 18,
            top: 14,
            bottom: 14,
        },
        preview_inner: egui::Margin::same(20),
        value_pill_inner: egui::Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 8,
        },
        info_chip_inner: egui::Margin {
            left: 10,
            right: 10,
            top: 5,
            bottom: 5,
        },
    };
    const CARD_BG_DARK: egui::Color32 = egui::Color32::from_rgb(42, 47, 57);
    const CARD_BORDER_DARK: egui::Color32 = egui::Color32::from_rgb(61, 67, 77);
    const CONTROL_BG_DARK: egui::Color32 = egui::Color32::from_rgb(58, 63, 71);
    const ACCENT: egui::Color32 = egui::Color32::from_rgb(42, 168, 242);
    const ICON_DARK: egui::Color32 = egui::Color32::from_rgba_premultiplied(242, 244, 247, 170);
    const CARD_BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
    const CARD_BORDER_LIGHT: egui::Color32 = egui::Color32::from_rgb(204, 213, 226);
    const CONTROL_BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(238, 243, 249);
    const ICON_LIGHT: egui::Color32 = egui::Color32::from_rgba_premultiplied(28, 35, 45, 170);

    pub(super) fn apply_typography(ui: &mut egui::Ui) {
        let style = ui.style_mut();
        style.override_font_id = Some(egui::FontId::proportional(Self::TYPOGRAPHY.body));
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(Self::TYPOGRAPHY.body),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(Self::TYPOGRAPHY.body),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(Self::TYPOGRAPHY.body),
        );
    }

    pub(super) fn page_viewport_size(ui: &egui::Ui) -> egui::Vec2 {
        ui.available_size()
    }

    pub(super) fn page_surface_size(ui: &egui::Ui) -> egui::Vec2 {
        Self::page_surface_size_for_viewport(Self::page_viewport_size(ui))
    }

    pub(super) fn page_overflows_horizontally(viewport_size: egui::Vec2) -> bool {
        Self::page_surface_size_for_viewport(viewport_size).x > viewport_size.x
    }

    pub(super) fn page_content_width(ui: &egui::Ui) -> f32 {
        Self::page_content_width_for_surface(ui.available_width())
    }

    pub(super) fn card_width(ui: &egui::Ui) -> f32 {
        ui.available_width().min(Self::LAYOUT.card_max_width)
    }

    pub(super) fn control_width(ui: &egui::Ui) -> f32 {
        ui.available_width().clamp(0.0, Self::CONTROLS.width)
    }

    pub(super) fn preview_width(ui: &egui::Ui) -> f32 {
        ui.available_width().min(Self::LAYOUT.preview_max_width)
    }

    pub(super) fn page_horizontal_margin(
        ui: &egui::Ui,
        content_width: f32,
        align_to_viewport_start: bool,
    ) -> f32 {
        Self::page_horizontal_margin_for_surface(
            ui.available_width(),
            content_width,
            align_to_viewport_start,
        )
    }

    pub(super) fn header_text_width(ui: &egui::Ui) -> f32 {
        let available_width = ui.available_width().max(0.0);
        let preferred_width =
            (available_width - Self::CONTROLS.width - Self::CONTROLS.gap - 42.0).max(220.0);
        preferred_width.min(available_width)
    }

    pub(super) fn row_label_width(ui: &egui::Ui) -> f32 {
        let available_width = ui.available_width().max(0.0);
        let preferred_width =
            (available_width - Self::CONTROLS.width - Self::CONTROLS.gap - 52.0).max(180.0);
        preferred_width.min(available_width)
    }

    pub(super) fn divider_width(ui: &egui::Ui) -> f32 {
        (ui.available_width() - 40.0).max(0.0)
    }

    pub(super) fn card_bg(ui: &egui::Ui) -> egui::Color32 {
        Self::theme_color(ui, Self::CARD_BG_DARK, Self::CARD_BG_LIGHT)
    }

    pub(super) fn card_border(ui: &egui::Ui) -> egui::Color32 {
        Self::theme_color(ui, Self::CARD_BORDER_DARK, Self::CARD_BORDER_LIGHT)
    }

    pub(super) fn control_bg(ui: &egui::Ui) -> egui::Color32 {
        Self::theme_color(ui, Self::CONTROL_BG_DARK, Self::CONTROL_BG_LIGHT)
    }

    pub(super) fn icon_color(ui: &egui::Ui) -> egui::Color32 {
        Self::theme_color(ui, Self::ICON_DARK, Self::ICON_LIGHT)
    }

    pub(super) fn accent() -> egui::Color32 {
        Self::ACCENT
    }

    pub(super) fn card_frame(ui: &egui::Ui) -> egui::Frame {
        Self::framed(
            Self::card_bg(ui),
            Self::card_border(ui),
            Self::MARGINS.card_inner,
        )
    }

    pub(super) fn preview_frame(ui: &egui::Ui, fill: egui::Color32) -> egui::Frame {
        Self::framed(fill, Self::card_border(ui), Self::MARGINS.preview_inner)
    }

    fn framed(
        fill: egui::Color32,
        border: egui::Color32,
        inner_margin: egui::Margin,
    ) -> egui::Frame {
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(Self::LAYOUT.card_radius))
            .inner_margin(inner_margin)
    }

    fn theme_color(ui: &egui::Ui, dark: egui::Color32, light: egui::Color32) -> egui::Color32 {
        if ui.visuals().dark_mode { dark } else { light }
    }

    fn page_surface_size_for_viewport(viewport_size: egui::Vec2) -> egui::Vec2 {
        egui::vec2(
            viewport_size.x.max(Self::LAYOUT.page_min_viewport_width),
            viewport_size.y.max(Self::LAYOUT.page_min_viewport_height),
        )
    }

    fn page_content_width_for_surface(surface_width: f32) -> f32 {
        (surface_width - Self::LAYOUT.page_side_padding * 2.0)
            .clamp(0.0, Self::LAYOUT.page_max_width)
    }

    fn page_horizontal_margin_for_surface(
        surface_width: f32,
        content_width: f32,
        align_to_viewport_start: bool,
    ) -> f32 {
        if align_to_viewport_start {
            Self::LAYOUT.page_side_padding
        } else {
            ((surface_width - content_width) * 0.5).max(Self::LAYOUT.page_side_padding)
        }
    }

    pub(super) fn pill_outer_width(control_width: f32) -> f32 {
        (control_width - Self::CONTROLS.icon_button_size - Self::CONTROLS.gap).max(0.0)
    }

    pub(super) fn pill_content_width(outer_width: f32) -> f32 {
        let horizontal_padding =
            (Self::MARGINS.value_pill_inner.left + Self::MARGINS.value_pill_inner.right) as f32;
        (outer_width - horizontal_padding).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_surface_grows_to_min_viewport() {
        let surface = SettingsUi::page_surface_size_for_viewport(egui::vec2(900.0, 600.0));

        assert_eq!(surface.x, SettingsUi::LAYOUT.page_min_viewport_width);
        assert_eq!(surface.y, SettingsUi::LAYOUT.page_min_viewport_height);
    }

    #[test]
    fn page_surface_preserves_large_viewport() {
        let surface = SettingsUi::page_surface_size_for_viewport(egui::vec2(1440.0, 900.0));

        assert_eq!(surface, egui::vec2(1440.0, 900.0));
    }

    #[test]
    fn page_content_width_respects_padding_and_max_width() {
        let laptop_width = SettingsUi::page_content_width_for_surface(1180.0);
        let narrow_width = SettingsUi::page_content_width_for_surface(800.0);

        assert_eq!(laptop_width, SettingsUi::LAYOUT.page_max_width);
        assert_eq!(narrow_width, 752.0);
    }
}
