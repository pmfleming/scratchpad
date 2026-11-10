use super::SettingsUi;
use crate::app::{fonts::EDITOR_FONT_FAMILY, services::settings_store::AppSettings};
use eframe::egui;

impl SettingsUi {
    /// The editor resolves System/Scratchpad fonts and colors in one place.
    /// Keep this style local to Settings, including child menus and controls.
    pub(in crate::app::ui::settings) fn apply_appearance(
        ui: &mut egui::Ui,
        settings: &AppSettings,
    ) {
        let text = settings.editor_text_color();
        let background = settings.editor_background_color();
        let accent = settings.editor_text_highlight_color();
        let font = egui::FontId::new(
            settings.font_size(),
            egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()),
        );
        let style = ui.style_mut();
        style.override_font_id = Some(font.clone());
        style.wrap_mode = Some(egui::TextWrapMode::Wrap);
        for (kind, ratio) in [
            (egui::TextStyle::Body, 1.0),
            (egui::TextStyle::Button, 1.0),
            (egui::TextStyle::Monospace, 1.0),
            (egui::TextStyle::Small, 12.5 / 14.0),
            (egui::TextStyle::Heading, 20.0 / 14.0),
        ] {
            style.text_styles.insert(
                kind,
                egui::FontId::new(font.size * ratio, font.family.clone()),
            );
        }
        let visuals = &mut style.visuals;
        visuals.dark_mode = background.intensity() < 0.5;
        visuals.override_text_color = Some(text);
        visuals.weak_text_color = Some(background.lerp_to_gamma(text, 0.75));
        visuals.panel_fill = background;
        visuals.window_fill = background.lerp_to_gamma(text, 0.03);
        visuals.extreme_bg_color = background.lerp_to_gamma(text, 0.07);
        visuals.window_stroke.color = background.lerp_to_gamma(text, 0.3);
        visuals.widgets.noninteractive.bg_stroke.color = visuals.window_stroke.color;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke.color = settings.editor_text_highlight_text_color();
        visuals.hyperlink_color = accent;
    }

    pub(in crate::app::ui::settings) fn scale(ui: &egui::Ui) -> f32 {
        ui.style()
            .override_font_id
            .as_ref()
            .filter(|font| matches!(&font.family, egui::FontFamily::Name(name) if name.as_ref() == EDITOR_FONT_FAMILY))
            .map_or(1.0, |font| font.size / 14.0)
    }

    pub(in crate::app::ui::settings) fn control_height(ui: &egui::Ui) -> f32 {
        Self::CONTROLS.height * Self::scale(ui).max(1.0)
    }

    pub(in crate::app::ui::settings) fn column_width(ui: &egui::Ui) -> f32 {
        Self::CONTROLS.column_width * Self::scale(ui).max(1.0)
    }

    pub(in crate::app::ui::settings) fn size_dropdown_width(ui: &egui::Ui, width: f32) -> f32 {
        (96.0 * Self::scale(ui).max(1.0)).min(width * 0.45)
    }

    pub(in crate::app::ui::settings) fn row_height(ui: &egui::Ui) -> f32 {
        56.0 * Self::scale(ui).max(1.0)
    }
}
