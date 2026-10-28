use super::{
    ScratchpadApp, category_heading, egui, expandable_card, inner_select_row, toggle_select_row,
    u32_slider_value_control,
};
use std::time::Duration;

mod storage;
#[cfg(test)]
mod tests;

pub(super) fn render_recovery_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "History & Recovery");
    super::sections::text_history_budget_card(ui, app);
    ui.add_space(super::SettingsUi::LAYOUT.card_gap);
    expandable_card(
        ui,
        "recovery.policy",
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        "Automatic recovery",
        "Retention and reopening policy.",
        false,
        |ui| render_policy(ui, app),
    );
    ui.add_space(super::SettingsUi::LAYOUT.card_gap);
    expandable_card(
        ui,
        "recovery.storage",
        egui_phosphor::regular::FLOPPY_DISK,
        "Disk usage",
        "Actual data-directory usage and stored recovery data.",
        false,
        |ui| {
            render_usage(ui, app);
        },
    );
}

fn render_policy(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut policy = app.state.app_settings.history.recovery.clone();
    super::sections::byte_budget_row(
        ui,
        "Recovery points",
        Some("Snapshot storage limit."),
        &mut policy.snapshot_byte_budget,
        1,
        16384,
    );
    super::sections::byte_budget_row(
        ui,
        "Closed archives",
        Some("Closed-tab storage limit."),
        &mut policy.archive_byte_budget,
        1,
        65536,
    );
    inner_select_row(
        ui,
        "Archive retention",
        Some("Zero means no age limit."),
        |ui| {
            let days = policy.archive_retention_days;
            let value = if days == 0 {
                "No expiry".to_owned()
            } else if days == 1 {
                "1 day".to_owned()
            } else {
                format!("{days} days")
            };
            u32_slider_value_control(
                ui,
                "settings.recovery.retention",
                "slider.Archive retention",
                &mut policy.archive_retention_days,
                0..=36500,
                82.0,
                value,
            );
        },
    );
    toggle_select_row(
        ui,
        "Restore archived tabs",
        Some("When reopening files."),
        "settings.recovery.reopen_archives",
        policy.reopen_archives,
        |enabled| policy.reopen_archives = enabled,
    );
    toggle_select_row(
        ui,
        "Open changed files",
        Some("Alongside recovered text."),
        "settings.recovery.open_changed_files",
        policy.open_changed_disk_files,
        |enabled| policy.open_changed_disk_files = enabled,
    );
    if policy != app.state.app_settings.history.recovery {
        crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
    }
}

fn render_usage(ui: &mut egui::Ui, app: &ScratchpadApp) {
    let store = app.state.persistence.session_store.clone();
    let settings_root = crate::app::app_state::settings_state::settings_path(app)
        .parent()
        .unwrap_or(store.root())
        .to_owned();
    let result = crate::app::ui::recovery::query_with_interval(
        ui,
        ("storage", store.root().to_owned(), settings_root.clone()),
        Duration::from_secs(5),
        move || store.storage_usage_with_settings(&settings_root),
    );
    match result {
        None => storage::notice(
            ui,
            egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
            "Measuring disk usage…",
            "Collecting file sizes and recovery details in the background.",
        ),
        Some(Err(error)) => storage::notice(
            ui,
            egui_phosphor::regular::WARNING,
            "Disk usage unavailable",
            &error,
        ),
        Some(Ok(usage)) => storage::render(ui, &usage),
    }
    storage::location(ui, app.state.persistence.session_store.root());
}
