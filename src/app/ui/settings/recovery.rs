use super::{ScratchpadApp, egui, widgets::group_heading};
use crate::app::services::session_store::StorageUsage;
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

pub(super) fn render_policy(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    group_heading(ui, "History memory cache");
    super::sections::text_history_budget(ui, app);
    if ui.button("Use recommended").clicked() {
        crate::app::app_state::settings_controller::reset_history_cache_to_recommended(app);
    }
    ui.add_space(8.0);
    ui.label(
        "All edit branches and closed-file history are retained until you explicitly forget them.",
    );
}

fn render_content(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    result: Option<Result<&StorageUsage, &str>>,
) {
    render_policy(ui, app);
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
