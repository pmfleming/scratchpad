use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::fonts::{EDITOR_FONT_FAMILY, EditorFontPreset};
use crate::app::services::settings_store::{AppThemeMode, TabListPosition};
use crate::app::theme::*;
use eframe::egui;

mod appearance;
mod sections;
mod style;
mod text_formatting;
mod widgets;

use sections::render_settings_categories;
use style::SettingsUi;
use widgets::*;

const EDITOR_GUTTER_RANGE: core::ops::RangeInclusive<u8> = 0..=32;
const FONT_SIZE_OPTIONS: [u32; 9] = [11, 12, 14, 16, 18, 20, 24, 28, 32];

pub(crate) fn show_page(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.scope(|ui| {
            SettingsUi::apply_typography(ui);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let content_width = SettingsUi::page_content_width(ui);
                    let horizontal_margin = SettingsUi::page_horizontal_margin(ui, content_width);

                    ui.add_space(SettingsUi::LAYOUT.body_top_space);
                    ui.horizontal(|ui| {
                        ui.add_space(horizontal_margin);
                        ui.vertical(|ui| {
                            ui.set_width(content_width);
                            render_page_heading(ui);
                            render_settings_categories(ui, app);
                        });
                    });
                    ui.add_space(SettingsUi::LAYOUT.body_bottom_space);
                });
        });
    });
}

fn render_page_heading(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Settings")
            .size(SettingsUi::TYPOGRAPHY.title)
            .strong()
            .color(text_primary(ui)),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Editor appearance, runtime behavior, and stored configuration.")
            .size(SettingsUi::TYPOGRAPHY.description)
            .color(text_muted(ui)),
    );
}

fn category_heading(ui: &mut egui::Ui, heading: &str) {
    ui.label(
        egui::RichText::new(heading)
            .size(SettingsUi::TYPOGRAPHY.category)
            .strong()
            .color(text_primary(ui)),
    );
    ui.add_space(12.0);
}
