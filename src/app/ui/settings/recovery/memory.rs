use super::{ScratchpadApp, egui};
use crate::app::{
    app_state::settings_controller as settings,
    services::settings_store::SearchResultLimit,
    ui::settings::widgets::{NumericFormat, ResetButton, SliderRow, numeric_slider_row},
};

const MIB: u64 = 1024 * 1024;
const SEARCH_STOPS: [usize; 8] = [
    50_000, 100_000, 250_000, 500_000, 1_000_000, 2_500_000, 5_000_000, 10_000_000,
];
const UNLIMITED: u64 = SEARCH_STOPS.len() as u64;

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

fn nearest_stop(matches: usize) -> u64 {
    SEARCH_STOPS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            ((**a as f64 / matches as f64).ln().abs())
                .total_cmp(&((**b as f64 / matches as f64).ln().abs()))
        })
        .map_or(0, |(index, _)| index as u64)
}

fn format_stop(stop: u64) -> String {
    let Some(&matches) = SEARCH_STOPS.get(stop as usize) else {
        return "Unlimited".into();
    };
    if matches >= 1_000_000 {
        format!("{}M", matches as f64 / 1_000_000.0)
    } else {
        format!("{}K", matches / 1000)
    }
}

fn parse_stop(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    if text == "unlimited" || text == "∞" {
        return Some(UNLIMITED);
    }
    let (number, factor) = if let Some(number) = text.strip_suffix('k') {
        (number, 1000.0)
    } else if let Some(number) = text.strip_suffix('m') {
        (number, 1_000_000.0)
    } else {
        (text.as_str(), 1.0)
    };
    let value = number.trim().parse::<f64>().ok()? * factor;
    (value.is_finite() && value > 0.0).then(|| nearest_stop(value as usize))
}

pub(super) fn search_limit(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let current = app.state.app_settings.editor.search_result_limit;
    let mut stop = if current == SearchResultLimit::Unlimited {
        UNLIMITED
    } else {
        nearest_stop(current.max_matches())
    };
    let custom = current != SearchResultLimit::Recommended;
    let response = numeric_slider_row(
        ui,
        "Search result limit",
        &mut stop,
        SliderRow {
            range: 0..=UNLIMITED,
            unit: "hits",
            reserve_reset: true,
            reset: Some(ResetButton {
                label: "Reset search result limit to recommended",
                enabled: custom,
                tooltip: if custom {
                    "Reset to recommended (100K)\nBounds memory used by dense searches"
                } else {
                    "Using recommended (100K)\nBounds memory used by dense searches"
                },
            }),
            format: Some(NumericFormat {
                show: format_stop,
                parse: parse_stop,
                current_text: (current != SearchResultLimit::Unlimited
                    && !SEARCH_STOPS.contains(&current.max_matches()))
                .then(|| current.max_matches().to_string()),
            }),
        },
    );
    if response.reset {
        settings::set_search_result_limit(app, SearchResultLimit::Recommended);
    } else if response.changed {
        let limit = SEARCH_STOPS
            .get(stop as usize)
            .map_or(SearchResultLimit::Unlimited, |&matches| {
                SearchResultLimit::from_matches(matches)
            });
        settings::set_search_result_limit(app, limit);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_values_accept_units_snap_and_reject_invalid_input() {
        for (text, expected) in [
            ("50K", 0),
            ("100000", 1),
            ("300k", 2),
            ("2.5M", 5),
            ("Unlimited", UNLIMITED),
            ("∞", UNLIMITED),
            ("1", 0),
        ] {
            assert_eq!(parse_stop(text), Some(expected), "{text}");
        }
        for text in ["", "oops", "-100K", "0", "NaN", "inf", "1e999"] {
            assert_eq!(parse_stop(text), None, "{text}");
        }
        for stop in 0..=UNLIMITED {
            assert_eq!(parse_stop(&format_stop(stop)), Some(stop));
        }
    }
}
