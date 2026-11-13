use super::{ScratchpadApp, egui};
use crate::app::{
    app_state::settings_controller as settings,
    services::settings_store::{DefaultSearchScope, ReviewThreshold, SearchResultLimit},
    ui::settings::widgets::{
        NumericFormat, ResetButton, SliderRow, group_heading, numeric_slider_row,
    },
};
const SEARCH_STOPS: [usize; 8] = [
    50_000, 100_000, 250_000, 500_000, 1_000_000, 2_500_000, 5_000_000, 10_000_000,
];
const UNLIMITED: u64 = SEARCH_STOPS.len() as u64;

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
    if is_unlimited(&text) {
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
    let current = app.state.app_settings.search.result_limit;
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

pub(super) fn render(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let original = app.state.app_settings.search.clone();
    let mut value = original.clone();
    super::widgets::segmented_row(
        ui,
        "Default scope",
        &mut value.default_scope,
        &[
            (DefaultSearchScope::LastUsed, "Last used"),
            (DefaultSearchScope::File, "File"),
            (DefaultSearchScope::Tab, "Tab"),
            (DefaultSearchScope::AllTabs, "All tabs"),
        ],
    );
    ui.label(
        egui::RichText::new("Selected text starts a locked Selection search.")
            .size(12.5)
            .color(crate::app::theme::text_muted(ui)),
    );
    super::widgets::toggle_select_row(
        ui,
        "Preserve case by default",
        None,
        "search.preserve_case",
        value.preserve_case,
        |v| value.preserve_case = v,
    );
    let mut recent = value.recent_searches as u64;
    let recent_custom = recent != 20;
    let recent_response = numeric_slider_row(
        ui,
        "Recent searches",
        &mut recent,
        SliderRow {
            range: 0..=100,
            unit: "queries",
            reserve_reset: true,
            reset: Some(ResetButton {
                label: "Reset recent searches",
                enabled: recent_custom,
                tooltip: "Recommended: 20; zero clears and disables recent searches",
            }),
            format: None,
        },
    );
    if recent_response.reset {
        value.recent_searches = 20;
    } else if recent_response.changed {
        value.recent_searches = recent as usize;
    }
    if value != original {
        app.state.app_settings.search = value;
        crate::app::app_state::search_state::recent::commit(app);
        settings::persist_settings_or_error(app);
    }
    search_limit(ui, app);
    group_heading(ui, "Review before replacing");
    let original = app.state.app_settings.search.clone();
    let mut value = original.clone();
    threshold_row(
        ui,
        "Files changed",
        "Reset file review threshold",
        &mut value.review_files_above,
        1,
    );
    threshold_row(
        ui,
        "Matches changed",
        "Reset match review threshold",
        &mut value.review_matches_above,
        100,
    );
    ui.label(
        egui::RichText::new(
            "Review when either threshold is exceeded. Unlimited disables that check.",
        )
        .size(12.5)
        .color(crate::app::theme::text_muted(ui)),
    );
    if value != original {
        app.state.app_settings.search = value;
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
        settings::persist_settings_or_error(app);
    }
}

/// Slider position past the largest count, meaning "never review".
const UNLIMITED_REVIEW: u64 = 1_000_001;

fn threshold_row(
    ui: &mut egui::Ui,
    label: &str,
    reset_label: &'static str,
    value: &mut ReviewThreshold,
    recommended: usize,
) {
    let mut count = match value {
        ReviewThreshold::Count(n) => *n as u64,
        ReviewThreshold::Unlimited(_) => UNLIMITED_REVIEW,
    };
    let response = numeric_slider_row(
        ui,
        label,
        &mut count,
        SliderRow {
            range: 0..=UNLIMITED_REVIEW,
            unit: "",
            reserve_reset: true,
            reset: Some(ResetButton {
                label: reset_label,
                enabled: *value != ReviewThreshold::Count(recommended),
                tooltip: "Reset to recommended",
            }),
            format: Some(NumericFormat {
                show: format_review_count,
                parse: parse_review_count,
                current_text: None,
            }),
        },
    );
    if response.reset {
        *value = ReviewThreshold::Count(recommended);
    } else if response.changed {
        *value = if count == UNLIMITED_REVIEW {
            ReviewThreshold::UNLIMITED
        } else {
            ReviewThreshold::Count(count as usize)
        };
    }
}

fn is_unlimited(text: &str) -> bool {
    text.eq_ignore_ascii_case("unlimited") || text == "∞"
}

fn format_review_count(count: u64) -> String {
    if count == UNLIMITED_REVIEW {
        "Unlimited".into()
    } else {
        count.to_string()
    }
}

fn parse_review_count(text: &str) -> Option<u64> {
    let text = text.trim();
    if is_unlimited(text) {
        return Some(UNLIMITED_REVIEW);
    }
    text.parse::<u64>()
        .ok()
        .filter(|count| *count < UNLIMITED_REVIEW)
}

#[cfg(test)]
mod tests {
    use super::{
        UNLIMITED, UNLIMITED_REVIEW, format_review_count, format_stop, parse_review_count,
        parse_stop,
    };

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
        for (text, expected) in [
            (" 0 ", Some(0)),
            ("1000000", Some(1_000_000)),
            ("UNLIMITED", Some(UNLIMITED_REVIEW)),
            ("∞", Some(UNLIMITED_REVIEW)),
            ("1000001", None),
            ("-1", None),
            ("many", None),
        ] {
            assert_eq!(parse_review_count(text), expected, "{text}");
        }
        for count in [0, 7, UNLIMITED_REVIEW] {
            assert_eq!(parse_review_count(&format_review_count(count)), Some(count));
        }
    }
}
