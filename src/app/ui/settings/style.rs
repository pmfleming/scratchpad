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
    /// Settings-only control skin. Dialogs retain their established styling and
    /// the selected editor font remains the UI font.
    pub(super) fn apply_controls(ui: &mut egui::Ui) {
        let field = Self::field_bg(ui);
        let hover = Self::control_bg(ui);
        let border = Self::card_border(ui);
        let text = crate::app::theme::text_primary(ui);
        let (selected, accent) = if ui.visuals().dark_mode {
            (
                egui::Color32::from_rgb(41, 63, 54),
                egui::Color32::from_rgb(168, 214, 194),
            )
        } else {
            (
                egui::Color32::from_rgb(217, 237, 226),
                egui::Color32::from_rgb(37, 91, 67),
            )
        };
        let style = ui.style_mut();
        style.spacing.interact_size.y = Self::CONTROLS.height;
        style.spacing.button_padding = egui::vec2(10.0, 7.0);
        style.visuals.extreme_bg_color = field;
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
        let rail = if ui.visuals().dark_mode {
            egui::Color32::from_rgb(110, 128, 137)
        } else {
            egui::Color32::from_rgb(105, 122, 132)
        };
        let accent = ui.visuals().selection.stroke.color;
        let style = ui.style_mut();
        style.spacing.slider_rail_height = 6.0;
        style.visuals.widgets.inactive.bg_fill = rail;
        style.visuals.widgets.hovered.bg_fill = accent;
        style.visuals.widgets.active.bg_fill = accent;
        style.visuals.selection.bg_fill = accent;
    }

    pub(super) fn field_bg(ui: &egui::Ui) -> egui::Color32 {
        if ui.visuals().dark_mode {
            egui::Color32::from_rgb(23, 27, 30)
        } else {
            egui::Color32::from_rgb(241, 244, 246)
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
    /// Foreground, resting border, and hover fill for destructive actions.
    pub(super) fn destructive_colors(dark: bool) -> [egui::Color32; 3] {
        let colors = if dark {
            [[240, 164, 164], [98, 67, 69], [57, 40, 43]]
        } else {
            [[157, 45, 56], [179, 109, 118], [251, 233, 236]]
        };
        colors.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
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
