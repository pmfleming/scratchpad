//! One muted status line above the results, like History's list header.
use super::result_model::{file_count_label, match_count_label};
use super::state::{SearchStripActions, SearchStripState};
use crate::app::{
    app_state::{
        ScratchpadApp, SearchFreshness, SearchScopeOrigin, SearchStatus,
        search_state::SEARCH_RESULT_LIMIT_MESSAGE,
    },
    shortcut_tooltips,
    theme::text_muted,
    ui::panel_style,
};
use eframe::egui;
use egui_phosphor::regular as icons;

const STATUS_TEXT_SIZE: f32 = 12.0;

pub(super) fn show(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let limited = state.progress.status == SearchStatus::Error(SEARCH_RESULT_LIMIT_MESSAGE.into());
    let updating = state.progress.searching || state.progress.freshness == SearchFreshness::Stale;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            navigation(ui, state, actions);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(status_text(app, state, limited, updating))
                            .size(STATUS_TEXT_SIZE)
                            .color(text_muted(ui)),
                    )
                    .truncate(),
                );
                if updating {
                    response.on_hover_text("Replace is available when the search finishes.");
                }
            });
        });
    });
    if updating {
        let fraction = if state.progress.target_count > 0 {
            state.progress.scanned_targets as f32 / state.progress.target_count as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_height(3.0)
                .corner_radius(0)
                .animate(true),
        );
    } else if let SearchStatus::Error(message) = &state.progress.status {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(icons::WARNING)
                    .font(egui_phosphor::font_id(STATUS_TEXT_SIZE + 2.0))
                    .color(ui.visuals().warn_fg_color),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(message)
                        .size(STATUS_TEXT_SIZE)
                        .color(ui.visuals().warn_fg_color),
                )
                .wrap(),
            );
            actions.settings_requested |= ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("Search settings").size(STATUS_TEXT_SIZE),
                    )
                    .frame(false),
                )
                .clicked();
        });
    }
}

fn navigation(ui: &mut egui::Ui, state: &SearchStripState, actions: &mut SearchStripActions) {
    let enabled = state.replace_availability.allows_actions();
    actions.next_requested |= panel_style::icon_button_with_hint(
        ui,
        "search.next",
        icons::ARROW_DOWN,
        "Next match",
        shortcut_tooltips::SEARCH_NEXT_MATCH,
        enabled,
    )
    .clicked();
    actions.previous_requested |= panel_style::icon_button_with_hint(
        ui,
        "search.previous",
        icons::ARROW_UP,
        "Previous match",
        shortcut_tooltips::SEARCH_PREVIOUS_MATCH,
        enabled,
    )
    .clicked();
    ui.label(
        egui::RichText::new(format!(
            "{} of {}",
            state.active_match_index.map_or(0, |i| i + 1),
            state.match_count
        ))
        .size(STATUS_TEXT_SIZE)
        .color(text_muted(ui)),
    );
}

fn status_text(
    app: &ScratchpadApp,
    state: &SearchStripState,
    limited: bool,
    updating: bool,
) -> String {
    let count = if limited {
        format!("{}+ matches", state.match_count)
    } else {
        match_count_label(state.match_count)
    };
    let selection = crate::app::app_state::search_state::selection::resolve(app).and_then(
        |(index, buffer, _, range)| {
            let b = app.tab_manager.tabs.as_slice()[index].buffer_by_id(buffer)?;
            Some(format!(
                "lines {}–{} of {}",
                b.preview_for_match(&(range.start..range.start)).0,
                b.preview_for_match(&(range.end..range.end)).0,
                b.name
            ))
        },
    );
    let mut text = match (state.query.is_empty(), selection) {
        // Before typing, only a captured selection is worth describing.
        (true, Some(place)) => format!("In {place}"),
        (true, None) => String::new(),
        (false, Some(place)) => format!("{count} in {place}"),
        (false, None) => format!("{count} in {}", file_count_label(state.result_groups.len())),
    };
    if app.state.search_state.scope_origin() == SearchScopeOrigin::SelectionDefault {
        text.push_str(" · selected text");
    }
    if updating {
        let progress = app.state.search_state.progress();
        text = format!(
            "{} matches found · {} previews · updating…",
            progress.total_match_count, progress.displayed_match_count
        );
    }
    text
}
