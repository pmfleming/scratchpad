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
        "Stored as TOML and loaded on startup.",
        app,
    );
    ui.add_space(SettingsUi::LAYOUT.card_gap);
    text_history_budget_card(ui, app);
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

fn text_history_budget_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    expandable_card(
        ui,
        "advanced.text_history_budget",
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        "Memory Assigned to Undo Operations",
        "Adjust the size of undo history.",
        false,
        |ui| {
            let mut budget = app.app_settings.history_budget;
            byte_budget_row(
                ui,
                "Per-file",
                "how much undo data one file is allowed to keep.",
                &mut budget.per_file_byte_budget,
                1,
                1024,
            );
            byte_budget_row(
                ui,
                "Total",
                "Data allowed across all files.",
                &mut budget.aggregate_byte_budget,
                4,
                4096,
            );
            byte_budget_row(
                ui,
                "Session",
                "how much undo/replay data can be saved/restored between sessions.",
                &mut budget.persisted_payload_budget,
                0,
                1024,
            );
            inner_select_row(
                ui,
                "Automatic defaults",
                Some("Use values based on this system's available memory."),
                |ui| {
                    fixed_width_control(ui, |ui| {
                        if widget_ids::surface_response(
                            ui,
                            "settings.history_budget.reset_auto",
                            widget_ids::WidgetRole::ActionButton,
                            |ui| ui.button("Reset to auto"),
                        )
                        .clicked()
                        {
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
        fixed_width_control(ui, |ui| {
            let control_width = SettingsUi::control_width(ui);
            ui.horizontal(|ui| {
                ui.add_sized(
                    egui::vec2((control_width - 58.0).max(0.0), 0.0),
                    egui::Slider::new(&mut mb, min_mb..=max_mb)
                        .step_by(1.0)
                        .show_value(false),
                );
                ui.add_space(8.0);
                ui.label(format!("{mb} MB"));
            });
        });
    });
    *value = mb * BYTES_PER_DISPLAY_MB;
}
