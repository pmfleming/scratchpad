use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::fonts::{EDITOR_FONT_FAMILY, EditorFontPreset};
use crate::app::theme::{ACTION_BG, ACTION_HOVER_BG, BORDER, TEXT_MUTED, TEXT_PRIMARY};
use eframe::egui;

mod widgets;

use widgets::*;

const SETTINGS_PAGE_MAX_WIDTH: f32 = 980.0;
const SETTINGS_BODY_FONT_SIZE: f32 = 15.0;
const SETTINGS_TITLE_FONT_SIZE: f32 = 28.0;
const SETTINGS_CATEGORY_FONT_SIZE: f32 = 20.0;
const SETTINGS_DESCRIPTION_FONT_SIZE: f32 = 12.5;
const SETTINGS_CARD_RADIUS: u8 = 10;
const SETTINGS_CARD_MIN_HEIGHT: f32 = 72.0;
const SETTINGS_INNER_ROW_HEIGHT: f32 = 56.0;
const SETTINGS_CONTROL_WIDTH: f32 = 190.0;
const SETTINGS_CONTROL_GAP: f32 = 8.0;
const SETTINGS_ICON_BUTTON_SIZE: f32 = 34.0;
const SETTINGS_PILL_WIDTH: f32 =
    SETTINGS_CONTROL_WIDTH - SETTINGS_ICON_BUTTON_SIZE - SETTINGS_CONTROL_GAP;
const EDITOR_GUTTER_RANGE: core::ops::RangeInclusive<u8> = 0..=32;
const SETTINGS_PREVIEW_TEXT: &str = "I hear the ruin of all space, shattered glass and toppling masonry, and time one livid final flame.";
const FONT_SIZE_OPTIONS: [u32; 9] = [11, 12, 14, 16, 18, 20, 24, 28, 32];

const SETTINGS_CARD_BG: egui::Color32 = egui::Color32::from_rgb(42, 47, 57);
const SETTINGS_CARD_BORDER: egui::Color32 = egui::Color32::from_rgb(61, 67, 77);
const SETTINGS_CONTROL_BG: egui::Color32 = egui::Color32::from_rgb(58, 63, 71);
const SETTINGS_PREVIEW_BG: egui::Color32 = egui::Color32::from_rgb(37, 42, 51);
const SETTINGS_ACCENT: egui::Color32 = egui::Color32::from_rgb(42, 168, 242);
const SETTINGS_ICON: egui::Color32 = egui::Color32::from_rgba_premultiplied(242, 244, 247, 170);

pub(crate) fn show_page(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.scope(|ui| {
            apply_settings_typography(ui);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let available_width = ui.available_width();
                    let content_width = available_width.min(SETTINGS_PAGE_MAX_WIDTH);
                    let horizontal_margin = ((available_width - content_width) * 0.5).max(24.0);

                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.add_space(horizontal_margin);
                        ui.vertical(|ui| {
                            ui.set_width(content_width);
                            render_page_heading(ui);
                            ui.add_space(24.0);
                            render_text_formatting_category(ui, app);
                            ui.add_space(24.0);
                            render_diagnostics_category(ui, app);
                            ui.add_space(24.0);
                            render_advanced_category(ui, app);
                        });
                    });
                    ui.add_space(28.0);
                });
        });
    });
}

fn apply_settings_typography(ui: &mut egui::Ui) {
    let font_id = egui::FontId::proportional(SETTINGS_BODY_FONT_SIZE);
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

fn render_page_heading(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Settings")
            .size(SETTINGS_TITLE_FONT_SIZE)
            .strong()
            .color(TEXT_PRIMARY),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Editor appearance, runtime behavior, and stored configuration.")
            .size(SETTINGS_DESCRIPTION_FONT_SIZE)
            .color(TEXT_MUTED),
    );
}

fn render_text_formatting_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Text Formatting");
    expandable_card(
        ui,
        "settings_font_card",
        egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
        "Font",
        "Choose the text appearance for editor content.",
        true,
        |ui| {
            inner_select_row(ui, "Family", Some("Pick the bundled editor font."), |ui| {
                let mut selected_font = app.editor_font();
                egui::ComboBox::from_id_salt("settings_editor_font")
                    .selected_text(selected_font.label())
                    .width(SETTINGS_CONTROL_WIDTH)
                    .show_ui(ui, |ui| {
                        for preset in EditorFontPreset::ALL {
                            ui.selectable_value(&mut selected_font, preset, preset.label());
                        }
                    });
                if selected_font != app.editor_font() {
                    app.set_editor_font(selected_font);
                }
            });
            inner_divider(ui);
            inner_select_row(ui, "Size", Some("Adjust the editor text size."), |ui| {
                let mut selected_size = app.font_size().round() as u32;
                egui::ComboBox::from_id_salt("settings_font_size")
                    .selected_text(selected_size.to_string())
                    .width(SETTINGS_CONTROL_WIDTH)
                    .show_ui(ui, |ui| {
                        for option in FONT_SIZE_OPTIONS {
                            ui.selectable_value(&mut selected_size, option, option.to_string());
                        }
                    });

                let selected_size = selected_size as f32;
                if (selected_size - app.font_size()).abs() > f32::EPSILON {
                    app.set_font_size(selected_size);
                }
            });
            inner_divider(ui);
            inner_select_row(
                ui,
                "Gutter",
                Some("Add space around the editor text area."),
                |ui| {
                    let mut selected_gutter = app.editor_gutter();
                    ui.add(
                        egui::DragValue::new(&mut selected_gutter)
                            .range(EDITOR_GUTTER_RANGE)
                            .speed(0.25)
                            .suffix(" px"),
                    );

                    if selected_gutter != app.editor_gutter() {
                        app.set_editor_gutter(selected_gutter);
                    }
                },
            );
            ui.add_space(14.0);
            render_preview_panel(ui, app);
        },
    );
    ui.add_space(8.0);
    toggle_card(
        ui,
        egui_phosphor::regular::TEXT_OUTDENT,
        "Word wrap",
        "Fit text within the editor width by default.",
        app.word_wrap(),
        |enabled| app.set_word_wrap(enabled),
    );
}

fn render_diagnostics_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Diagnostics");
    toggle_card(
        ui,
        egui_phosphor::regular::MAGNIFYING_GLASS,
        "File logging",
        "Write runtime diagnostics while Scratchpad is running.",
        app.logging_enabled(),
        |enabled| app.set_logging_enabled(enabled),
    );
}

fn render_advanced_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Advanced");
    settings_file_card(
        ui,
        egui_phosphor::regular::FLOPPY_DISK,
        "Settings file",
        "Stored as TOML and loaded on startup.",
        app,
    );
    ui.add_space(8.0);
    action_card(
        ui,
        egui_phosphor::regular::ARROW_SQUARE_UP,
        "Reset to defaults",
        "Restore the current settings file to app defaults.",
        "Reset to defaults",
        ScratchpadApp::reset_settings_to_defaults,
        app,
    );
}

fn category_heading(ui: &mut egui::Ui, heading: &str) {
    ui.label(
        egui::RichText::new(heading)
            .size(SETTINGS_CATEGORY_FONT_SIZE)
            .strong()
            .color(TEXT_PRIMARY),
    );
    ui.add_space(12.0);
}
