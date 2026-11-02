use super::{
    ScratchpadApp, SettingsUi, egui,
    widgets::{
        dropdown, group_heading, inner_select_row, record_settings_control_box, toggle_select_row,
    },
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

pub(super) fn render_policy(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
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
    if policy != app.state.app_settings.history.recovery {
        crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
    }
    if retention_limits_heading(ui) {
        crate::app::app_state::settings_controller::reset_retention_and_limits_to_recommended(app);
    }
    let mut policy = app.state.app_settings.history.recovery.clone();
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
    if policy != app.state.app_settings.history.recovery {
        crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
    }
    super::sections::text_history_budget(ui, app);
}

fn retention_limits_heading(ui: &mut egui::Ui) -> bool {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Retention & limits")
                .size(12.5)
                .strong()
                .color(super::text_muted(ui)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = widget_ids::surface_response(
                ui,
                "settings.retention.reset",
                widget_ids::WidgetRole::IconButton,
                |ui| {
                    ui.add_sized(
                        egui::Vec2::splat(SettingsUi::CONTROLS.height),
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                                .font(egui_phosphor::font_id(18.0)),
                        ),
                    )
                },
            );
            record_settings_control_box("button.Use recommended", response.rect);
            response
                .on_hover_text("Reset retention and all limits to recommended values.")
                .accessible_name("Use recommended")
                .clicked()
        })
        .inner
    })
    .inner
}

fn retention_row(ui: &mut egui::Ui, days: &mut u32) {
    inner_select_row(ui, "Keep closed tabs for", None, |ui| {
        retention_slider(ui, days);
        dropdown(
            ui,
            "Retention",
            days,
            &[7, 30, 90, 365],
            |days| match days {
                // Preserve the display of existing policies until explicitly edited.
                0 => "No expiry".into(),
                1 => "1 day".into(),
                days => format!("{days} days"),
            },
            130.0,
            "",
        );
    });
}

/// A logarithmic day scale leaves useful room for short durations.
fn retention_slider(ui: &mut egui::Ui, days: &mut u32) {
    let lane = ui.max_rect();
    let width = (lane.width() - 130.0 - 10.0).max(20.0);
    let rect = egui::Rect::from_min_size(lane.min, egui::vec2(width, lane.height()));
    let mut value = f64::from(*days);
    let response = widget_ids::rect_scope_with_layout(
        ui,
        rect,
        "settings.retention.slider.layout",
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().slider_width = width;
            SettingsUi::apply_slider_style(ui);
            widget_ids::surface_response(ui, "settings.retention", "slider", |ui| {
                ui.add(
                    egui::Slider::new(&mut value, 1.0..=365.0)
                        .logarithmic(true)
                        .fixed_decimals(0)
                        .clamping(egui::SliderClamping::Edits)
                        .handle_shape(egui::style::HandleShape::Circle)
                        .trailing_fill(true)
                        .show_value(false)
                        .custom_formatter(|value, _| format!("{value:.0} days")),
                )
            })
        },
    )
    .inner;
    record_settings_control_box("slider.Retention", response.rect);
    let changed = response.changed();
    response
        .on_hover_text("Keep closed tabs for 1 to 365 days.")
        .accessible_name("Keep closed tabs for");
    if changed {
        *days = value.round() as u32;
    }
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
