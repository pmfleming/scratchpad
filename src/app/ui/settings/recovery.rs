use super::{
    ScratchpadApp, category_heading, egui, expandable_card, inner_select_row, toggle_select_row,
    u32_slider_value_control,
};
use crate::app::services::session_store::StorageUsage;
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
        "Recovery & storage",
        "Retained history, limits and reopening.",
        false,
        |ui| render_recovery(ui, app),
    );
}

fn render_policy(ui: &mut egui::Ui, app: &mut ScratchpadApp, usage: Option<&StorageUsage>) {
    let mut policy = app.state.app_settings.history.recovery.clone();
    let stored = usage
        .filter(|usage| usage.stored_error.is_none())
        .map(|usage| &usage.stored);
    let archive_hint = limit_hint(stored.map(|stored| stored.archives as u64), "archive");
    let snapshot_hint = limit_hint(stored.map(|stored| stored.recovery_points), "snapshot");
    super::sections::byte_budget_row(
        ui,
        "Closed tabs",
        Some(&archive_hint),
        &mut policy.archive_byte_budget,
        1,
        65536,
    );
    super::sections::byte_budget_row(
        ui,
        "Recovery points",
        Some(&snapshot_hint),
        &mut policy.snapshot_byte_budget,
        1,
        16384,
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

fn limit_hint(count: Option<u64>, kind: &str) -> String {
    let count = count.map_or_else(|| "—".into(), |count| count.to_string());
    format!("{count} retained · Logical {kind} limit.")
}

fn render_recovery(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
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
    render_content(
        ui,
        app,
        result.as_ref().map(|result| {
            result
                .as_ref()
                .map(|usage| usage.as_ref())
                .map_err(String::as_str)
        }),
    );
}

fn render_content(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    result: Option<Result<&StorageUsage, &str>>,
) {
    storage_lane(ui, |ui| match result {
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
            error,
        ),
        Some(Ok(usage)) => storage::summary(ui, usage),
    });
    super::inner_divider(ui);
    let usage = result.and_then(Result::ok);
    render_policy(ui, app, usage);
    storage_lane(ui, |ui| {
        if let Some(usage) = usage {
            storage::details(ui, usage);
        }
        storage::location(ui, app.state.persistence.session_store.root());
    });
}

/// Match the leading label inset of the standard settings control rows.
fn storage_lane(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
    });
}
