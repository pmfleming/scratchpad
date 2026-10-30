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
        if retention_slider(ui, days) {
            ui.data_mut(|data| data.insert_temp(id, false));
            selection = if [0, 7, 30, 90].contains(days) {
                Some(*days)
            } else {
                None
            };
        }
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
            130.0,
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

/// A logarithmic day scale leaves useful room for short durations while still
/// covering custom values up to 100 years. Only the far-right stop is unlimited.
fn retention_slider(ui: &mut egui::Ui, days: &mut u32) -> bool {
    let lane = ui.max_rect();
    let width = (lane.width() - 130.0 - 10.0).max(20.0);
    let rect = egui::Rect::from_min_size(lane.min, egui::vec2(width, lane.height()));
    let mut value = if *days == 0 {
        f64::INFINITY
    } else {
        f64::from(*days)
    };
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
                    egui::Slider::new(&mut value, 1.0..=f64::INFINITY)
                        .logarithmic(true)
                        .largest_finite(36500.0)
                        // Round to whole days without arithmetic stepping:
                        // infinity minus a fixed step would stay infinite.
                        .fixed_decimals(0)
                        .clamping(egui::SliderClamping::Edits)
                        .handle_shape(egui::style::HandleShape::Circle)
                        .trailing_fill(true)
                        .show_value(false)
                        .custom_formatter(|value, _| {
                            if value.is_infinite() {
                                "No expiry".into()
                            } else {
                                format!("{value:.0} days")
                            }
                        }),
                )
            })
        },
    )
    .inner;
    record_settings_control_box("slider.Retention", response.rect);
    let changed = response.changed();
    response
        .on_hover_text("Keep closed tabs for 1 day to 100 years. Drag fully right for No expiry.")
        .accessible_name("Keep closed tabs for");
    if changed {
        *days = if value.is_infinite() {
            0
        } else {
            value.round() as u32
        };
    }
    changed
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
