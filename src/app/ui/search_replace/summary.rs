use super::state::{SearchStripActions, SearchStripState};
use crate::app::{
    app_state::{ScratchpadApp, SearchFreshness, SearchStatus},
    theme::text_muted,
    ui::{notice, settings::widgets::group_heading},
};
use eframe::egui;

pub(super) fn show(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    group_heading(ui, "Results");
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(
                if state.progress.status
                    == SearchStatus::Error(
                        crate::app::app_state::search_state::SEARCH_RESULT_LIMIT_MESSAGE.into(),
                    )
                {
                    format!("{}+", state.match_count)
                } else {
                    state.match_count.to_string()
                },
            )
            .size(30.0),
        );
        ui.label(
            egui::RichText::new("matches")
                .size(12.5)
                .color(text_muted(ui)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let enabled = state.replace_availability.allows_actions();
            actions.next_requested |= ui
                .add_enabled(enabled, egui::Button::new("↓"))
                .accessible_name("Next match")
                .on_hover_text(crate::app::shortcut_tooltips::SEARCH_NEXT_MATCH)
                .clicked();
            actions.previous_requested |= ui
                .add_enabled(enabled, egui::Button::new("↑"))
                .accessible_name("Previous match")
                .on_hover_text(crate::app::shortcut_tooltips::SEARCH_PREVIOUS_MATCH)
                .clicked();
            ui.label(
                egui::RichText::new(format!(
                    "{} of {}",
                    state.active_match_index.map_or(0, |i| i + 1),
                    state.match_count
                ))
                .size(12.5)
                .color(text_muted(ui)),
            );
        });
    });
    let detail = crate::app::app_state::search_state::selection::resolve(app)
        .and_then(|(index, buffer, _, range)| {
            let b = app.tab_manager.tabs.as_slice()[index].buffer_by_id(buffer)?;
            Some(format!(
                "in lines {}–{} of {}",
                b.preview_for_match(&(range.start..range.start)).0,
                b.preview_for_match(&(range.end..range.end)).0,
                b.name
            ))
        })
        .unwrap_or_else(|| format!("in {} files", state.result_groups.len()));
    let detail = if app.state.search_state.scope_origin()
        == crate::app::app_state::SearchScopeOrigin::SelectionDefault
    {
        format!("{detail} · selected text")
    } else {
        detail
    };
    ui.label(egui::RichText::new(detail).size(12.5).color(text_muted(ui)));
    if state.progress.searching || state.progress.freshness == SearchFreshness::Stale {
        let progress = app.state.search_state.progress();
        ui.label(
            egui::RichText::new(format!(
                "{} matches found · {} previews",
                progress.total_match_count, progress.displayed_match_count
            ))
            .size(12.5)
            .color(text_muted(ui)),
        );
        let fraction = if state.progress.target_count > 0 {
            state.progress.scanned_targets as f32 / state.progress.target_count as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_height(7.0)
                .animate(true),
        );
        notice::show(
            ui,
            egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
            "Updating results…",
            "Replace is available when the search finishes.",
        );
    } else if let SearchStatus::Error(message) = &state.progress.status {
        notice::show(
            ui,
            egui_phosphor::regular::WARNING,
            "Search needs attention",
            message,
        );
        actions.settings_requested |= ui.button("Search settings").clicked();
    }
}
