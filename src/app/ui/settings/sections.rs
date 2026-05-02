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
        "Text history",
        "Per-file undo storage and replay payload budgets.",
        false,
        |ui| {
            let mut budget = app.app_settings.history_budget;
            let auto = crate::app::domain::TextHistoryBudget::derive_from_available_memory();
            budget_row(
                ui,
                "Per-file entry limit",
                auto.per_file_entry_limit as u64,
                &mut budget.per_file_entry_limit,
                100,
                100_000,
            );
            byte_budget_row(
                ui,
                "Per-file byte budget",
                auto.per_file_byte_budget,
                &mut budget.per_file_byte_budget,
                1,
                1024,
            );
            byte_budget_row(
                ui,
                "Aggregate byte budget",
                auto.aggregate_byte_budget,
                &mut budget.aggregate_byte_budget,
                4,
                4096,
            );
            byte_budget_row(
                ui,
                "Persisted payload budget",
                auto.persisted_payload_budget,
                &mut budget.persisted_payload_budget,
                0,
                1024,
            );
            inner_select_row(ui, "Automatic defaults", Some(auto_label(&budget)), |ui| {
                fixed_width_control(ui, |ui| {
                    if ui.button("Reset to auto").clicked() {
                        app.reset_history_budget_to_auto();
                    }
                });
            });
            if budget != app.app_settings.history_budget {
                app.set_history_budget(budget);
            }
        },
    );
}

fn budget_row(
    ui: &mut egui::Ui,
    label: &str,
    auto_value: u64,
    value: &mut usize,
    min: usize,
    max: usize,
) {
    inner_select_row(ui, label, Some(&format!("Auto: {auto_value}")), |ui| {
        fixed_width_control(ui, |ui| {
            ui.add(egui::DragValue::new(value).range(min..=max).speed(10));
        });
    });
}

fn byte_budget_row(
    ui: &mut egui::Ui,
    label: &str,
    auto_value: u64,
    value: &mut u64,
    min_mib: u64,
    max_mib: u64,
) {
    let mut mib = (*value / (1024 * 1024)).clamp(min_mib, max_mib);
    inner_select_row(
        ui,
        label,
        Some(&format!("Auto: {} MiB", auto_value / (1024 * 1024))),
        |ui| {
            fixed_width_control(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut mib).range(min_mib..=max_mib));
                    ui.label("MiB");
                });
            });
        },
    );
    *value = mib * 1024 * 1024;
}

fn auto_label(budget: &crate::app::domain::TextHistoryBudget) -> &'static str {
    if budget.derived_from_memory {
        "Using memory-derived startup values."
    } else {
        "Using user-set values."
    }
}
