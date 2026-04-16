use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::fonts::{EDITOR_FONT_FAMILY, EditorFontPreset};
use crate::app::services::settings_store::{AppThemeMode, TabListPosition};
use crate::app::theme::*;
use eframe::egui;

mod appearance;
mod opening;
mod sections;
mod style;
mod text_formatting;
mod widgets;

use sections::render_settings_categories;
use style::SettingsUi;
use widgets::*;

const FONT_SIZE_OPTIONS: [u32; 9] = [11, 12, 14, 16, 18, 20, 24, 28, 32];

pub(crate) fn show_page(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        with_settings_page(ui, |ui, horizontal_overflow| {
            render_page_body(ui, app, horizontal_overflow)
        })
    });
}

fn with_settings_page(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui, bool)) {
    ui.scope(|ui| {
        SettingsUi::apply_typography(ui);
        let viewport_size = SettingsUi::page_viewport_size(ui);
        let surface_size = SettingsUi::page_surface_size(ui);
        let horizontal_overflow = SettingsUi::page_overflows_horizontally(viewport_size);
        egui::ScrollArea::both()
            .id_salt("settings_page_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_size(surface_size);
                ui.set_width(surface_size.x);
                ui.set_max_width(surface_size.x);
                add_contents(ui, horizontal_overflow);
            });
    });
}

fn render_page_body(ui: &mut egui::Ui, app: &mut ScratchpadApp, horizontal_overflow: bool) {
    let content_width = SettingsUi::page_content_width(ui);
    let horizontal_margin =
        SettingsUi::page_horizontal_margin(ui, content_width, horizontal_overflow);

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

pub(crate) fn apply_dialog_typography(ui: &mut egui::Ui) {
    SettingsUi::apply_typography(ui);
}

pub(crate) fn dialog_card_frame(ui: &egui::Ui) -> egui::Frame {
    SettingsUi::card_frame(ui)
}

pub(crate) fn dialog_card_border(ui: &egui::Ui) -> egui::Color32 {
    SettingsUi::card_border(ui)
}
