use super::{
    ScratchpadApp, egui,
    widgets::{DropdownPlacement, dropdown, group_heading, inner_select_row, toggle_select_row},
};
use crate::app::{services::session_store::StorageUsage, ui::widget_ids};
use std::time::Duration;

mod storage;
#[cfg(test)]
mod tests;

pub(super) fn render_recovery_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let store = app.state.persistence.session_store.clone();
    let settings_root = crate::app::app_state::settings_state::settings_path(app)
        .parent()
        .unwrap_or(store.root())
        .to_owned();
    let key = ("storage", store.root().to_owned(), settings_root.clone());
    let result = crate::app::ui::recovery::query_with_interval(
        ui,
        key.clone(),
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
    if result.as_ref().is_some_and(|result| {
        result
            .as_ref()
            .map_or(true, |usage| usage.stored_error.is_some())
    }) && ui.button("Retry disk usage").clicked()
    {
        crate::app::ui::recovery::refresh_query::<StorageUsage>(ui, key);
    }
}

fn render_policy(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut policy = app.state.app_settings.history.recovery.clone();
    toggle_select_row(
        ui,
        "Restore history when reopening files",
        None,
        "settings.recovery.reopen_archives",
        policy.reopen_archives,
        |value| policy.reopen_archives = value,
    );
    toggle_select_row(
        ui,
        "Also open changed disk files",
        Some("Keep the disk version alongside recovered text."),
        "settings.recovery.open_changed_files",
        policy.open_changed_disk_files,
        |value| policy.open_changed_disk_files = value,
    );
    group_heading(ui, "Retention");
    retention_row(ui, &mut policy.archive_retention_days);
    super::sections::byte_budget_row(
        ui,
        "Closed-tab budget",
        &mut policy.archive_byte_budget,
        1,
        65536,
    );
    super::sections::byte_budget_row(
        ui,
        "Recovery-point budget",
        &mut policy.snapshot_byte_budget,
        1,
        16384,
    );
    ui.label(egui::RichText::new("Age and size limits both apply. No expiry removes only the age limit.").size(12.5).color(super::text_muted(ui)))
        .on_hover_text("Lower limits can expire older recovery data. Budgets limit retained data, not the size of the database file.");
    if policy != app.state.app_settings.history.recovery {
        crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
    }
}

fn retention_row(ui: &mut egui::Ui, days: &mut u32) {
    let id = widget_ids::local(ui, "settings.retention.custom");
    let custom = ui
        .data_mut(|data| data.get_temp::<bool>(id))
        .unwrap_or(false);
    let mut selection = if custom || ![0, 7, 30, 90].contains(days) {
        None
    } else {
        Some(*days)
    };
    inner_select_row(ui, "Keep closed tabs for", None, |ui| {
        if dropdown(
            ui,
            "Retention",
            &mut selection,
            &[Some(7), Some(30), Some(90), Some(0), None],
            |value| match value {
                Some(0) => "No expiry".into(),
                Some(days) => format!("{days} days"),
                None => "Custom…".into(),
            },
            DropdownPlacement::Trailing(180.0),
            "",
        ) {
            ui.data_mut(|data| data.insert_temp(id, selection.is_none()));
            if let Some(value) = selection {
                *days = value;
            } else if *days == 0 {
                *days = 30;
            }
        }
    });
    if selection.is_none() {
        inner_select_row(ui, "Retention period", None, |ui| {
            let response = ui.add(
                egui::DragValue::new(days)
                    .range(1..=36500)
                    .clamp_existing_to_range(false)
                    .update_while_editing(false)
                    .suffix(" days"),
            );
            response.accessible_name("Retention period").on_hover_text(
                "Enter a custom number of days. Choose No expiry to remove the age limit.",
            );
        });
    }
}

fn render_content(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    result: Option<Result<&StorageUsage, &str>>,
) {
    render_policy(ui, app);
    super::sections::text_history_budget(ui, app);
    group_heading(ui, "Storage");
    match result {
        None => storage::notice(
            ui,
            egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
            "Measuring disk usage…",
            "",
        ),
        Some(Err(error)) => storage::notice(
            ui,
            egui_phosphor::regular::WARNING,
            "Disk usage unavailable",
            error,
        ),
        Some(Ok(usage)) => {
            storage::summary(ui, usage);
            storage::details(ui, usage);
        }
    }
}
