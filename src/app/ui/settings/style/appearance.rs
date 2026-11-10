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
            Self::layout_font_size(ui, settings.font_size()),
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

    /// Changing the base font also changes every control's geometry. Keep the
    /// Settings layout at its starting size until a font-slider gesture ends.
    pub(in crate::app::ui::settings) fn hold_font_slider_layout(ui: &egui::Ui) {
        let size = ui.style().override_font_id.as_ref().map(|font| font.size);
        ui.data_mut(|data| {
            let id = egui::Id::unique("settings.font_size.drag_layout");
            if data.get_temp::<f32>(id).is_none() {
                if let Some(size) = size {
                    data.insert_temp(id, size);
                }
            }
        });
    }

    fn layout_font_size(ui: &egui::Ui, configured: f32) -> f32 {
        let dragging = ui.input(|input| input.pointer.primary_down());
        ui.data_mut(|data| {
            let id = egui::Id::unique("settings.font_size.drag_layout");
            if dragging {
                data.get_temp::<f32>(id).unwrap_or(configured)
            } else {
                data.remove::<f32>(id);
                configured
            }
        })
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
