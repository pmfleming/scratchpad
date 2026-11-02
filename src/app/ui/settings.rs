use crate::app::app_state::ScratchpadApp;
use crate::app::fonts::{EDITOR_FONT_FAMILY, EditorFontPreset};
use crate::app::services::settings_store::{
    AppThemeMode, EditorAppearanceSource, IndentationStyle, TabDisplayMode, TabListPosition,
};
use crate::app::theme::{text_muted, text_primary};
use crate::app::ui::widget_ids;
use eframe::egui;

mod appearance;
mod opening;
mod recovery;
mod sections;
mod style;
#[cfg(test)]
mod tests;
mod text_formatting;
mod widgets;

use style::SettingsUi;
const FONT_SIZE_OPTIONS: [u32; 9] = [11, 12, 14, 16, 18, 20, 24, 28, 32];
pub(crate) const PREVIEW_QUOTES: [(&str, &str); 3] = [
    (
        "A man of genius makes no mistakes. His errors are volitional and are the portals of discovery.",
        "His errors are volitional",
    ),
    (
        "To learn one must be humble. But life is the great teacher.",
        "life is the great teacher",
    ),
    (
        "Hold to the now, the here, through which all future plunges to the past.",
        "the now, the here",
    ),
];

pub(crate) fn show_page(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show(ui, |ui| {
        widget_ids::feature_scope(ui, "settings", |ui| {
            SettingsUi::apply_typography(ui);
            let width = ui.available_width();
            egui::ScrollArea::vertical()
                .id_salt(widget_ids::scroll_id(ui, "settings_page_scroll"))
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Never,
                    ..Default::default()
                })
                .auto_shrink([false, false])
                .show(ui, |ui| render_page_body(ui, app, width));
        });
    });
    opening::reset_confirmation(ui.ctx(), app);
}

fn render_page_body(ui: &mut egui::Ui, app: &mut ScratchpadApp, viewport_width: f32) {
    SettingsUi::apply_typography(ui);
    SettingsUi::apply_controls(ui);
    let available = ui.available_width().min(viewport_width);
    let width = (available - 20.0).clamp(0.0, SettingsUi::LAYOUT.card_max_width);
    let margin = ((available - width) * 0.5).max(0.0);
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(margin);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.set_width(width);
            ui.set_max_width(width);
            ui.label(
                egui::RichText::new("Settings")
                    .size(28.0)
                    .strong()
                    .color(text_primary(ui)),
            );
            ui.add_space(16.0);
            sections::render_settings_categories(ui, app);
        });
    });
    ui.add_space(28.0);
}

pub(crate) fn apply_dialog_typography(ui: &mut egui::Ui) {
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
// Dialogs also use these helpers; keep their established geometry and palette
// independent of the settings-page redesign.
pub(crate) fn dialog_card_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(if ui.visuals().dark_mode {
            egui::Color32::from_rgb(42, 47, 57)
        } else {
            egui::Color32::WHITE
        })
        .stroke(egui::Stroke::new(1.0, dialog_card_border(ui)))
        .corner_radius(10)
        .inner_margin(egui::Margin {
            left: 18,
            right: 18,
            top: 14,
            bottom: 14,
        })
}
pub(crate) fn dialog_card_border(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(61, 67, 77)
    } else {
        egui::Color32::from_rgb(204, 213, 226)
    }
}
