use super::appearance::{render_appearance_category, render_tab_position_category};
use super::opening::render_opening_category;
use super::text_formatting::render_text_formatting_category;
use super::*;

pub(super) fn render_settings_categories(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    for render_category in [
        render_text_formatting_category as fn(&mut egui::Ui, &mut ScratchpadApp),
        render_appearance_category,
        render_opening_category,
        render_tab_position_category,
        render_advanced_category,
    ] {
        ui.add_space(SettingsUi::LAYOUT.section_gap);
        render_category(ui, app);
    }
}

fn render_advanced_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "Advanced");
    settings_file_card(
        ui,
        egui_phosphor::regular::FLOPPY_DISK,
        "Settings file",
        "TOML on startup.",
        app,
    );
    ui.add_space(SettingsUi::LAYOUT.card_gap);
    text_history_budget_card(ui, app);
    ui.add_space(SettingsUi::LAYOUT.card_gap);
    action_card(
        ui,
        egui_phosphor::regular::ARROW_SQUARE_UP,
        "Reset to defaults",
        "Restore app defaults.",
        "Reset to defaults",
        ScratchpadApp::reset_settings_to_defaults,
        app,
    );
}

fn text_history_budget_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    expandable_card(
        ui,
        "advanced.text_history_budget",
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        "Undo memory",
        "Undo history limits.",
        false,
        |ui| {
            let mut budget = app.app_settings.history_budget;
            byte_budget_row(
                ui,
                "Per-file",
                "Per-file undo limit.",
                &mut budget.per_file_byte_budget,
                1,
                1024,
            );
            byte_budget_row(
                ui,
                "Total",
                "Total undo limit.",
                &mut budget.aggregate_byte_budget,
                4,
                4096,
            );
            byte_budget_row(
                ui,
                "Session",
                "Saved session undo limit.",
                &mut budget.persisted_payload_budget,
                0,
                1024,
            );
            inner_select_row(
                ui,
                "Automatic defaults",
                Some("Checks available memory."),
                |ui| {
                    available_width_control(ui, |ui| {
                        let button_width = SettingsUi::control_width(ui);
                        let response = widget_ids::surface_response(
                            ui,
                            "settings.history_budget.reset_auto",
                            widget_ids::WidgetRole::ActionButton,
                            |ui| {
                                ui.add_sized(
                                    egui::vec2(button_width, 0.0),
                                    egui::Button::new("Reset to auto"),
                                )
                            },
                        );
                        record_settings_control_box("button.Reset to auto", response.rect);
                        if response.clicked() {
                            app.reset_history_budget_to_auto();
                        }
                    });
                },
            );
            if budget != app.app_settings.history_budget {
                app.set_history_budget(budget);
            }
        },
    );
}

fn byte_budget_row(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    value: &mut u64,
    min_mb: u64,
    max_mb: u64,
) {
    const BYTES_PER_DISPLAY_MB: u64 = 1024 * 1024;

    let mut mb = (*value / BYTES_PER_DISPLAY_MB).clamp(min_mb, max_mb);
    inner_select_row(ui, label, Some(description), |ui| {
        slider_value_control(
            ui,
            format!("slider.{label}"),
            82.0,
            format!("{mb} MB"),
            |ui, slider_width| {
                ui.add_sized(
                    egui::vec2(slider_width, 0.0),
                    egui::Slider::new(&mut mb, min_mb..=max_mb)
                        .step_by(1.0)
                        .show_value(false),
                )
            },
        );
    });
    *value = mb * BYTES_PER_DISPLAY_MB;
}
