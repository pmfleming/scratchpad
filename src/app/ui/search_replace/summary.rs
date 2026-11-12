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

pub(super) fn footer(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    if let Some(outcome) = &app.state.search_state.replace.outcome {
        notice::show(ui, egui_phosphor::regular::INFO, outcome, "");
        if let Some((operation, files)) = app.state.search_state.replace.last_operation {
            let allowed = crate::app::app_state::search_state::replace::transaction::can_undo(
                app, operation, files,
            );
            if ui.add_enabled(allowed, egui::Button::new("Undo all")).on_hover_text("Undo every file in this operation. All affected files must be open and at this edit in History.").clicked() { actions.undo_all = Some((operation, files)); }
        }
    }
    if let Some(review) = &app
        .state
        .search_state
        .replace
        .pending_replace_all_confirmation
    {
        group_heading(ui, "Review replace all");
        egui::ScrollArea::vertical()
            .id_salt("search.replace.review")
            .max_height(144.0)
            .show_rows(ui, 24.0, review.plan.targets.len(), |ui, range| {
                for target in &review.plan.targets[range] {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&target.buffer_label)
                                .size(12.5)
                                .color(text_muted(ui)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format!("{} matches", target.replacements.len()));
                        });
                    });
                }
            });
        ui.horizontal(|ui| {
            actions.replace_all_requested |= ui
                .button(format!(
                    "Replace {} in {} files",
                    review.total_match_count, review.affected_buffer_count
                ))
                .clicked();
            actions.cancel_review |= ui.button("Cancel").clicked();
        });
    } else if state.replace_open
        && state.progress.status
            == SearchStatus::Error(
                crate::app::app_state::search_state::SEARCH_RESULT_LIMIT_MESSAGE.into(),
            )
    {
        actions.replace_all_requested |= ui
            .add_enabled(
                app.state.search_state.replace.planning.is_none()
                    && app.state.search_state.replace.pending.is_none(),
                egui::Button::new("Review all matches beyond the result limit"),
            )
            .clicked();
    } else if state.replace_open {
        ui.separator();
        let included = state
            .match_count
            .saturating_sub(app.state.search_state.replace.excluded.len());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{included} included"))
                    .size(12.5)
                    .color(text_muted(ui)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                actions.replace_all_requested |= ui
                    .add_enabled(
                        included > 0 && state.replace_availability.allows_actions(),
                        egui::Button::new("Replace all"),
                    )
                    .accessible_name("Replace all included matches")
                    .on_hover_text(crate::app::shortcut_tooltips::REPLACE_ALL_MATCHES)
                    .clicked();
                actions.replace_current_requested |= ui
                    .add_enabled(
                        state.replace_availability.allows_actions(),
                        egui::Button::new("Replace"),
                    )
                    .on_hover_text(crate::app::shortcut_tooltips::REPLACE_CURRENT_MATCH)
                    .clicked();
            });
        });
    }
}
