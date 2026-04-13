use super::appearance::{render_appearance_category, render_tab_position_category};
use super::opening::render_opening_category;
use super::text_formatting::render_text_formatting_category;
use super::*;

pub(super) fn render_settings_categories(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    for render_category in [
        render_appearance_category as fn(&mut egui::Ui, &mut ScratchpadApp),
        render_opening_category,
        render_text_formatting_category,
        render_tab_position_category,
        render_diagnostics_category,
        render_advanced_category,
    ] {
        ui.add_space(SettingsUi::LAYOUT.section_gap);
        render_category(ui, app);
    }
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
    ui.add_space(SettingsUi::LAYOUT.card_gap);
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
