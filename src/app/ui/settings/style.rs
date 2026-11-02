use crate::app::theme;
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
    pub height: f32,
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
        height: 36.0,
    };

    pub(super) fn apply_typography(ui: &mut egui::Ui) {
        let font = ui
            .style()
            .override_font_id
            .clone()
            .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
        let style = ui.style_mut();
        style.override_font_id = Some(font.clone());
        for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
            style.text_styles.insert(text, font.clone());
        }
    }

    pub(super) fn font(ui: &egui::Ui, size: f32) -> egui::FontId {
        let mut font = ui
            .style()
            .override_font_id
            .clone()
            .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
        font.size = size;
        font
    }

    /// Keep settings geometry while sharing the application's palette.
    pub(super) fn apply_controls(ui: &mut egui::Ui) {
        let field = Self::field_bg(ui);
        let hover = Self::control_bg(ui);
        let border = Self::card_border(ui);
        let text = theme::text_primary(ui);
        let selected = theme::tab_selected_bg(ui);
        let accent = Self::accent(ui);
        let style = ui.style_mut();
        style.spacing.interact_size.y = Self::CONTROLS.height;
        style.spacing.button_padding = egui::vec2(10.0, 7.0);
        style.visuals.extreme_bg_color = field;
        style.visuals.widgets.noninteractive.fg_stroke.color = text;
        style.visuals.selection.bg_fill = selected;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, accent);
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.bg_fill = hover;
            widget.weak_bg_fill = hover;
            widget.bg_stroke = egui::Stroke::new(1.0, border);
            widget.fg_stroke = egui::Stroke::new(1.0, text);
            widget.corner_radius = egui::CornerRadius::same(7);
        }
        style.visuals.widgets.inactive.bg_fill = field;
        style.visuals.widgets.inactive.weak_bg_fill = field;
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
    }
    /// Scoped to sliders so their rail and handle don't inherit dark field fills.
    pub(super) fn apply_slider_style(ui: &mut egui::Ui) {
        let rail = theme::text_muted(ui);
        let accent = ui.visuals().selection.stroke.color;
        let style = ui.style_mut();
        style.spacing.slider_rail_height = 6.0;
        style.visuals.widgets.inactive.bg_fill = rail;
        style.visuals.widgets.hovered.bg_fill = accent;
        style.visuals.widgets.active.bg_fill = accent;
        style.visuals.selection.bg_fill = accent;
    }

    pub(super) fn field_bg(ui: &egui::Ui) -> egui::Color32 {
        theme::action_bg(ui)
    }
    pub(super) fn card_bg(ui: &egui::Ui) -> egui::Color32 {
        ui.visuals().window_fill()
    }
    pub(super) fn card_border(ui: &egui::Ui) -> egui::Color32 {
        theme::border(ui)
    }
    pub(super) fn control_bg(ui: &egui::Ui) -> egui::Color32 {
        theme::action_hover_bg(ui)
    }
    pub(super) fn icon_color(ui: &egui::Ui) -> egui::Color32 {
        crate::app::theme::text_muted(ui)
    }
    /// Foreground, resting border, and hover fill for destructive actions.
    pub(super) fn destructive_colors(dark: bool) -> [egui::Color32; 3] {
        let colors = if dark {
            [[240, 164, 164], [98, 67, 69], [57, 40, 43]]
        } else {
            [[157, 45, 56], [179, 109, 118], [251, 233, 236]]
        };
        colors.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
    }
    pub(super) fn accent(ui: &egui::Ui) -> egui::Color32 {
        theme::tab_selected_accent(ui)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_inherit_ui_fonts_and_share_the_app_palette_in_both_themes() {
        for dark in [true, false] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let font = egui::FontId::monospace(17.0);
            ctx.global_style_mut(|style| style.override_font_id = Some(font.clone()));
            let original = ctx.global_style();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let background = ui.visuals().window_fill();
                SettingsUi::apply_typography(ui);
                SettingsUi::apply_controls(ui);
                assert_eq!(ui.style().override_font_id.as_ref(), Some(&font));
                assert_eq!(egui::TextStyle::Body.resolve(ui.style()), font);
                assert_eq!(SettingsUi::font(ui, 12.5).family, font.family);
                assert_eq!(SettingsUi::card_frame(ui).fill, background);
                assert_eq!(SettingsUi::field_bg(ui), theme::action_bg(ui));
                assert_eq!(SettingsUi::control_bg(ui), theme::action_hover_bg(ui));
                assert_eq!(SettingsUi::card_border(ui), theme::border(ui));
                assert_eq!(ui.visuals().text_color(), theme::text_primary(ui));
                assert_eq!(
                    ui.visuals().widgets.inactive.fg_stroke.color,
                    theme::text_primary(ui)
                );
                assert_eq!(
                    ui.visuals().selection.stroke.color,
                    theme::tab_selected_accent(ui)
                );
            });
            output.textures_delta.clear();
            assert_eq!(
                ctx.global_style(),
                original,
                "Settings styling must remain scoped"
            );
        }
    }
}
