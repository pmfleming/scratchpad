use super::{ScratchpadApp, egui};
use crate::app::{
    app_state::settings_controller as settings,
    ui::settings::widgets::{ResetButton, SliderRow, numeric_slider_row},
};

const MIB: u64 = 1024 * 1024;
fn memory_row(
    ui: &mut egui::Ui,
    label: &str,
    bytes: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    reset: Option<ResetButton<'_>>,
) -> bool {
    let mut value = *bytes / MIB;
    let response = numeric_slider_row(
        ui,
        label,
        &mut value,
        SliderRow {
            range,
            unit: "MiB",
            reserve_reset: true,
            reset,
            format: None,
        },
    );
    if response.changed {
        *bytes = value * MIB;
    }
    response.reset
}

pub(super) fn history_cache(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let original = app.state.app_settings.history.budget;
    let mut budget = original;
    memory_row(
        ui,
        "Memory per file",
        &mut budget.per_file_byte_budget,
        1..=1024,
        None,
    );
    let custom = !budget.derived_from_memory || budget != original;
    let reset = memory_row(
        ui,
        "Memory across all files",
        &mut budget.aggregate_byte_budget,
        4..=4096,
        Some(ResetButton {
            label: "Reset memory cache to recommended",
            enabled: custom,
            tooltip: if custom {
                "Reset both memory limits to recommended\nSized from available memory"
            } else {
                "Using recommended memory limits\nSized from available memory"
            },
        }),
    );
    if reset {
        settings::reset_history_cache_to_recommended(app);
    } else if budget != original {
        settings::set_history_budget(app, budget);
    }
}

pub(super) fn failure_limit(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let original = app.state.app_settings.history.failure_ram;
    let mut failure = original;
    let reset = memory_row(
        ui,
        "Memory while not saving",
        &mut failure.bytes,
        64..=(crate::app::services::chunk_store::total_memory() / MIB).max(64),
        Some(ResetButton {
            label: "Reset save-failure memory limit to automatic",
            enabled: !failure.automatic,
            tooltip: if failure.automatic {
                "Using automatic limit\n25% of system memory; unsaved text kept in RAM until saving resumes"
            } else {
                "Reset to automatic\n25% of system memory; unsaved text kept in RAM until saving resumes"
            },
        }),
    );
    if reset {
        failure = crate::app::services::chunk_store::FailureRamBudget::default();
    } else if failure.bytes != original.bytes {
        failure.automatic = false;
    }
    if failure != original {
        app.state.app_settings.history.failure_ram = failure;
        if let Some(store) = app.state.persistence.session_store.opened_chunk_store() {
            store.set_failure_limit(failure.bytes as usize);
        }
        settings::persist_settings_or_error(app);
    }
}
